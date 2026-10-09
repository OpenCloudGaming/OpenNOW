use super::catalog_actions::{
    LaunchDecision, LaunchStatus, PlayabilityMetadata, account_decision, game_play_denial,
    readiness_decision, selected_variant,
};
use super::*;

const STORE_GAME_QUERY: &str = r#"query OpenNowStoreClientApps($vpcId: String!, $locale: String!) {
  apps(vpcId: $vpcId, language: $locale, filters: {
    type: {
      in: ["PLATFORM_CLIENT"]
    }
  }) {
    items {
      id
      title
      images {
        HERO_IMAGE
      }
      variants {
        appStore
        id
        gfn {
          status
        }
      }
      gfn {
        playType
      }
    }
  }
}"#;

const SUPPORTED_STORES: [&str; 1] = ["STEAM"];

pub(super) fn store_launch_intent(params: &Value) -> Result<bool, ServiceError> {
    match params.get("storeLaunch") {
        None | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) => Ok(true),
        Some(_) => Err(ServiceError::invalid("storeLaunch must be a boolean")),
    }
}

fn requested_store(params: &Value) -> Result<&'static str, ServiceError> {
    match params.get("store") {
        None => Ok(SUPPORTED_STORES[0]),
        Some(Value::String(store)) if store == SUPPORTED_STORES[0] => Ok(SUPPORTED_STORES[0]),
        Some(Value::String(_)) => Err(ServiceError::invalid(
            "Only Steam store launches are supported",
        )),
        Some(_) => Err(ServiceError::invalid("store must be a string")),
    }
}

fn launch_id(variant: &Value) -> Option<&str> {
    variant["id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256)
}

pub(super) fn eligible_store_target<'a>(
    games: &'a [Value],
    store: &str,
) -> Option<(&'a Value, &'a Value)> {
    games.iter().find_map(|game| {
        let variant = game["variants"]
            .as_array()?
            .iter()
            .find(|variant| variant["store"] == store && launch_id(variant).is_some())?;
        Some((game, variant))
    })
}

fn resolved_store_target(
    candidates: &[Value],
    store: &str,
    target: Option<(&str, &str)>,
) -> Option<(Value, Value)> {
    match target {
        Some((app_id, variant_id)) => {
            let game = candidates.iter().find(|game| game["id"] == app_id)?;
            let variant = selected_variant(game, variant_id)?;
            Some((game.clone(), variant.clone()))
        }
        None => eligible_store_target(candidates, store)
            .map(|(game, variant)| (game.clone(), variant.clone())),
    }
}

pub(super) fn store_launch_decision(
    game: &Value,
    app_id: &str,
    variant_id: &str,
    store: &str,
    subscription: &Value,
    access: &Value,
    exact: bool,
) -> LaunchDecision {
    use LaunchStatus::*;
    let decide = |status, message| LaunchDecision { status, message };
    if game["id"] != app_id || variant_id.parse::<i32>().ok().is_none_or(|id| id <= 0) {
        return decide(
            MetadataUnconfirmed,
            "The exact store version could not be confirmed. Refresh and try again.",
        );
    }
    let Some(variant) = selected_variant(game, variant_id) else {
        return decide(
            MetadataUnconfirmed,
            "The selected store version is no longer available. Refresh and try again.",
        );
    };
    if variant["store"] != store {
        return decide(
            MetadataUnconfirmed,
            "The selected store version changed. Refresh and try again.",
        );
    }
    let playability = if exact {
        PlayabilityMetadata::Required
    } else {
        PlayabilityMetadata::WhenPresent
    };
    if let Some(decision) = readiness_decision(game, variant, playability) {
        return decision;
    }
    if !subscription["storageAddon"].is_object() {
        return decide(
            SubscriptionRequired,
            "Launching the Steam store for persistent game management requires the GeForce NOW persistent storage add-on.",
        );
    }
    if let Some(decision) = game_play_denial(subscription) {
        return decision;
    }
    if let Some(decision) = account_decision(game, variant, variant_id, access, subscription) {
        return decision;
    }
    decide(
        Ready,
        "Ready to open the Steam store for persistent game management.",
    )
}

impl GfnService {
    fn store_games(&self, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let revision = self.catalog_revision.load(std::sync::atomic::Ordering::Acquire);
            let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
            let token = session
                .tokens
                .id_token
                .as_deref()
                .unwrap_or(&session.tokens.access_token);
            let vpc = self.vpc_id(
                &client,
                session,
                generation,
                settings,
                token,
                Some(&self.store_cache.requests),
            )?;
            let payload = self.catalog_document(
                &client,
                token,
                STORE_GAME_QUERY,
                json!({"vpcId":vpc,"locale":"en_US"}),
            )?;
            let games = complete_games(&payload["data"]["apps"]["items"])?;
            self.check_scope(session, generation)?;
            self.check_catalog_revision(revision)?;
            Ok(json!({"games":games,"catalogRevision":revision,"freshness":"fresh","fetchedAt":now_ms()}))
        })
    }

    pub(crate) fn store_launch_inspect(
        &self,
        params: &Value,
        target: Option<(&str, &str)>,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let store = requested_store(params)?;
        let games = self.store_games(settings)?;
        let subscription = self.subscription(settings)?;
        if subscription["scope"] != games["scope"] {
            return Err(ServiceError {
                code: "stale_account",
                message: "The membership account changed.".into(),
            });
        }
        let candidates = games["games"].as_array().cloned().unwrap_or_default();
        let (app_id, variant_id, decision, game) = match resolved_store_target(
            &candidates,
            store,
            target,
        ) {
            Some((game, variant)) => {
                let app_id = game["id"].as_str().unwrap_or_default().to_owned();
                let variant_id = variant["id"].as_str().unwrap_or_default().to_owned();
                let access = self.account_connections(settings)?;
                if access["scope"] != games["scope"] {
                    return Err(ServiceError {
                        code: "stale_account",
                        message: "The store account changed.".into(),
                    });
                }
                let detail = self.catalog_game(&json!({"appId":app_id}), settings);
                let (metadata, exact) = match detail {
                    Ok(detail) => (detail["game"].clone(), true),
                    Err(error) if error.code == "catalog_game_not_found" => (game, false),
                    Err(error) => return Err(error),
                };
                let decision = store_launch_decision(
                    &metadata,
                    &app_id,
                    &variant_id,
                    store,
                    &subscription["subscription"],
                    &access,
                    exact,
                );
                (app_id, variant_id, decision, metadata)
            }
            None => match target {
                Some((app_id, variant_id)) => (
                    app_id.to_owned(),
                    variant_id.to_owned(),
                    LaunchDecision {
                        status: LaunchStatus::MetadataUnconfirmed,
                        message: "The exact store version could not be confirmed. Refresh and try again.",
                    },
                    Value::Null,
                ),
                None => (
                    String::new(),
                    String::new(),
                    LaunchDecision {
                        status: LaunchStatus::Unavailable,
                        message: "No Steam store launch is available for this account.",
                    },
                    Value::Null,
                ),
            },
        };
        let mut result = json!({
            "store":store,
            "appId":if app_id.is_empty() { Value::Null } else { json!(app_id) },
            "variantId":if variant_id.is_empty() { Value::Null } else { json!(variant_id) },
            "game":game,
            "decision":decision,
            "catalogRevision":games["catalogRevision"],
            "fetchedAt":games["fetchedAt"],
            "freshness":games["freshness"],
        });
        result["scope"] = games["scope"].clone();
        Ok(result)
    }
}
