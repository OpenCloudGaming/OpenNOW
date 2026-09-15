use super::*;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum LaunchStatus {
    Ready,
    OwnershipRequired,
    SelectionRequired,
    LinkRequired,
    SubscriptionRequired,
    Patching,
    Maintenance,
    Unavailable,
    MetadataUnconfirmed,
}

#[derive(Serialize)]
pub(super) struct LaunchDecision {
    pub(super) status: LaunchStatus,
    pub(super) message: &'static str,
}

pub(super) enum PlayabilityMetadata {
    Required,
    WhenPresent,
}

pub(super) fn selected_variant<'a>(game: &'a Value, variant_id: &str) -> Option<&'a Value> {
    game["variants"]
        .as_array()?
        .iter()
        .find(|variant| variant["id"] == variant_id)
}

pub(super) fn readiness_decision(
    game: &Value,
    variant: &Value,
    playability: PlayabilityMetadata,
) -> Option<LaunchDecision> {
    use LaunchStatus::*;
    let decide = |status, message| LaunchDecision { status, message };
    match game["playabilityState"].as_str() {
        Some("PLAYABLE") => (),
        Some("UNPLAYABLE_DUE_TO_UPGRADE" | "UNPLAYABLE_DUE_TO_TIME_CAPPED_LIMIT") => {
            return Some(decide(
                SubscriptionRequired,
                "Your GeForce NOW membership does not currently allow this game. Check your membership and available playtime.",
            ));
        }
        Some("UNKNOWN") => {
            return Some(decide(
                MetadataUnconfirmed,
                "Game availability could not be confirmed. Refresh and try again.",
            ));
        }
        None if matches!(playability, PlayabilityMetadata::Required) => {
            return Some(decide(
                MetadataUnconfirmed,
                "Game availability could not be confirmed. Refresh and try again.",
            ));
        }
        None => (),
        _ => {
            return Some(decide(
                Unavailable,
                "This game is currently unavailable on GeForce NOW.",
            ));
        }
    }
    match variant["stateDetails"]["__typename"].as_str() {
        Some("VariantGfnAutoPatchingMetadata" | "VariantGfnManualPatchingMetadata") => {
            return Some(decide(
                Patching,
                "This store version is being patched. Try again after the patch finishes.",
            ));
        }
        Some("VariantGfnMaintenanceMetadata") => {
            return Some(decide(
                Maintenance,
                "This store version is under maintenance.",
            ));
        }
        _ => (),
    }
    match variant["gfnStatus"].as_str() {
        Some("AVAILABLE") => (),
        Some("PATCHING") => {
            return Some(decide(
                Patching,
                "This store version is being patched. Try again later.",
            ));
        }
        Some("SERVER_MAINTENANCE") => {
            return Some(decide(
                Maintenance,
                "This store version is under maintenance.",
            ));
        }
        None | Some("UNKNOWN") => {
            return Some(decide(
                MetadataUnconfirmed,
                "Store version readiness could not be confirmed. Refresh and try again.",
            ));
        }
        _ => {
            return Some(decide(
                Unavailable,
                "This store version is currently unavailable.",
            ));
        }
    }
    if variant["playStatus"] == "NOT_PLAYABLE" {
        return Some(decide(
            Unavailable,
            "This store version cannot currently be played. Check your store account, subscription, and library sync.",
        ));
    }
    None
}

pub(super) fn account_decision(
    game: &Value,
    variant: &Value,
    variant_id: &str,
    access: &Value,
    subscription: &Value,
) -> Option<LaunchDecision> {
    use LaunchStatus::*;
    let decide = |status, message| LaunchDecision { status, message };
    if access["definitions"]["stores"]["status"] != "success" {
        return Some(decide(
            MetadataUnconfirmed,
            "Store account requirements could not be confirmed. Refresh and try again.",
        ));
    }
    let store =
        crate::catalog_types::normalize_store(variant["store"].as_str().unwrap_or_default());
    let Some(account) = access["accounts"]
        .as_array()
        .and_then(|accounts| accounts.iter().find(|account| account["provider"] == store))
    else {
        return Some(decide(
            MetadataUnconfirmed,
            "The selected store's account requirements are unavailable. Refresh and try again.",
        ));
    };
    let linking = &account["accountLinkingMetadata"];
    let applies = linking["supportedVariantIds"]
        .as_array()
        .is_none_or(|ids| ids.is_empty() || ids.iter().any(|id| id == variant_id));
    if applies
        && account["isRequired"] == true
        && (account["isConnected"] != true || account["status"] == "expired")
    {
        return Some(decide(
            LinkRequired,
            "Link or reconnect the selected store account in Settings before launching this version.",
        ));
    }
    if applies && account["supportsLinking"] == true && !account["isRequired"].is_boolean() {
        return Some(decide(
            MetadataUnconfirmed,
            "The selected store's linking requirement could not be confirmed.",
        ));
    }
    if let Some(id) = variant["subscription"].as_str().filter(|id| !id.is_empty()) {
        if !access["subscriptions"]
            .as_array()
            .is_some_and(|subscriptions| {
                subscriptions
                    .iter()
                    .any(|subscription| subscription["id"] == id)
            })
        {
            return Some(decide(
                SubscriptionRequired,
                "The store subscription recorded for this version is not active. Check your store account and sync your library.",
            ));
        }
    }
    if game["membershipTierLabel"].as_str().is_some_and(|tier| {
        !tier.trim().is_empty() && !tier.trim().to_ascii_lowercase().starts_with("free")
    }) {
        match subscription["membershipTier"]
            .as_str()
            .filter(|tier| !tier.trim().is_empty())
        {
            None => {
                return Some(decide(
                    MetadataUnconfirmed,
                    "Membership details could not be confirmed. Refresh and try again.",
                ));
            }
            Some(tier) if tier.trim().to_ascii_lowercase().starts_with("free") => {
                return Some(decide(
                    SubscriptionRequired,
                    "This game requires a paid GeForce NOW membership.",
                ));
            }
            _ => (),
        }
        if let Some(decision) = game_play_denial(subscription) {
            return Some(decision);
        }
    }
    None
}

pub(super) fn game_play_denial(subscription: &Value) -> Option<LaunchDecision> {
    (subscription["isGamePlayAllowed"] == false).then_some(LaunchDecision {
        status: LaunchStatus::SubscriptionRequired,
        message: "Your membership does not currently allow gameplay. Check your available playtime.",
    })
}

fn launch_decision(
    game: &Value,
    app_id: &str,
    variant_id: &str,
    subscription: &Value,
    access: &Value,
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
            "The selected store version is no longer available. Choose a store version again.",
        );
    };
    if let Some(decision) = readiness_decision(game, variant, PlayabilityMetadata::Required) {
        return decision;
    }
    match variant["libraryStatus"].as_str() {
        Some("MANUAL" | "PLATFORM_SYNC") => (),
        Some("NOT_OWNED") => {
            return decide(
                OwnershipRequired,
                "Confirm that you already own this game on the selected store before adding it to your GeForce NOW library.",
            );
        }
        _ => {
            return decide(
                MetadataUnconfirmed,
                "Ownership of this store version could not be confirmed. Refresh or sync your library.",
            );
        }
    }
    if let Some(decision) = account_decision(game, variant, variant_id, access, subscription) {
        return decision;
    }
    if variant["librarySelected"] == false {
        return decide(
            SelectionRequired,
            "Choose this owned store version for play before launching.",
        );
    }
    if variant["librarySelected"] != true {
        return decide(
            MetadataUnconfirmed,
            "Your preferred store version could not be confirmed. Refresh and try again.",
        );
    }
    decide(Ready, "Ready to play this store version.")
}

#[derive(Clone, Copy)]
enum Mutation {
    AddFavorite,
    RemoveFavorite,
    AddOwned,
    RemoveOwned,
    SelectOwned,
}

impl Mutation {
    fn parse(method: &str) -> Result<Self, ServiceError> {
        match method {
            "catalog.favorites.add" => Ok(Self::AddFavorite),
            "catalog.favorites.remove" => Ok(Self::RemoveFavorite),
            "catalog.ownership.add" => Ok(Self::AddOwned),
            "catalog.ownership.remove" => Ok(Self::RemoveOwned),
            "catalog.ownership.select" => Ok(Self::SelectOwned),
            _ => Err(ServiceError::invalid("Unknown catalog mutation")),
        }
    }

    fn root(self) -> &'static str {
        match self {
            Self::AddFavorite => "addFavoriteApp",
            Self::RemoveFavorite => "removeFavoriteApp",
            Self::AddOwned => "addOwnedVariant",
            Self::RemoveOwned => "removeOwnedVariant",
            Self::SelectOwned => "selectOwnedVariant",
        }
    }

    fn favorite(self) -> bool {
        matches!(self, Self::AddFavorite | Self::RemoveFavorite)
    }

    fn confirmed(self, game: &Value, variant_id: &str) -> bool {
        let variant = selected_variant(game, variant_id);
        match self {
            Self::AddFavorite => game["favorited"] == true,
            Self::RemoveFavorite => game["favorited"] == false,
            Self::AddOwned => variant.is_some_and(|variant| {
                matches!(
                    variant["libraryStatus"].as_str(),
                    Some("MANUAL" | "PLATFORM_SYNC")
                )
            }),
            Self::RemoveOwned => {
                variant.is_some_and(|variant| variant["libraryStatus"] == "NOT_OWNED")
            }
            Self::SelectOwned => variant.is_some_and(|variant| {
                variant["librarySelected"] == true
                    && matches!(
                        variant["libraryStatus"].as_str(),
                        Some("MANUAL" | "PLATFORM_SYNC")
                    )
            }),
        }
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub(super) struct CatalogActionKey {
    provider_id: String,
    user_id: String,
    app_id: String,
}

pub(super) struct MutationPermit<'a> {
    pending: &'a Mutex<std::collections::HashSet<CatalogActionKey>>,
    key: CatalogActionKey,
}

impl Drop for MutationPermit<'_> {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("catalog mutations poisoned")
            .remove(&self.key);
    }
}

impl GfnService {
    pub(super) fn admit_catalog_action(
        &self,
        session: &AuthSession,
        app_id: &str,
    ) -> Result<MutationPermit<'_>, ServiceError> {
        let key = CatalogActionKey {
            provider_id: session.provider.idp_id.clone(),
            user_id: session.user.user_id.clone(),
            app_id: app_id.to_owned(),
        };
        let mut pending = crate::store_requests::lock(&self.catalog_mutations)?;
        if !pending.insert(key.clone()) {
            return Err(ServiceError {
                code: "catalog_mutation_busy",
                message: "An update or launch for this game is already in progress.".into(),
            });
        }
        Ok(MutationPermit {
            pending: &self.catalog_mutations,
            key,
        })
    }
    pub fn catalog_launch_inspect(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let app_id = bounded_id(params, "appId", true)?;
        let variant_id = bounded_id(params, "variantId", true)?;
        if super::store_launch::store_launch_intent(params)? {
            return self.store_launch_inspect(params, Some((&app_id, &variant_id)), settings);
        }
        let mut result = self.catalog_game(&json!({"appId":app_id}), settings)?;
        let subscription = if result["game"]["membershipTierLabel"]
            .as_str()
            .is_some_and(|tier| {
                !tier.trim().is_empty() && !tier.trim().to_ascii_lowercase().starts_with("free")
            }) {
            let subscription = self.subscription(settings)?;
            if subscription["scope"] != result["scope"] {
                return Err(ServiceError {
                    code: "stale_account",
                    message: "The membership account changed.".into(),
                });
            }
            subscription["subscription"].clone()
        } else {
            Value::Null
        };
        let access = if selected_variant(&result["game"], &variant_id).is_some_and(|variant| {
            matches!(
                variant["libraryStatus"].as_str(),
                Some("MANUAL" | "PLATFORM_SYNC")
            )
        }) {
            let access = self.account_connections(settings)?;
            if access["scope"] != result["scope"] {
                return Err(ServiceError {
                    code: "stale_account",
                    message: "The store account changed.".into(),
                });
            }
            access
        } else {
            Value::Null
        };
        result["decision"] = serde_json::to_value(launch_decision(
            &result["game"],
            &app_id,
            &variant_id,
            &subscription,
            &access,
        ))
        .expect("launch decision serializable");
        result["appId"] = json!(app_id);
        result["variantId"] = json!(variant_id);
        Ok(result)
    }

    pub fn catalog_mutate(
        &self,
        method: &str,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let mutation = Mutation::parse(method)?;
        let app_id = bounded_id(params, "appId", true)?;
        let variant_id = if mutation.favorite() {
            String::new()
        } else {
            bounded_id(params, "variantId", true)?
        };
        if matches!(mutation, Mutation::AddOwned) && params["confirmedExistingLicense"] != true {
            return Err(ServiceError::invalid(
                "Confirm existing ownership of the selected store license first",
            ));
        }
        let (session, generation) = self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        if params["scope"] != scoped_result(json!({}), &session, generation)["scope"] {
            return Err(ServiceError {
                code: "stale_account",
                message: "The catalog update belongs to a different account context.".into(),
            });
        }
        let _permit = self.admit_catalog_action(&session, &app_id)?;
        let before = self.catalog_game(&json!({"appId":app_id}), settings)?;
        self.check_scope(&session, generation)?;
        if !mutation.favorite() && selected_variant(&before["game"], &variant_id).is_none() {
            return Err(ServiceError::invalid(
                "The selected variant does not belong to this game",
            ));
        }
        if matches!(mutation, Mutation::SelectOwned)
            && !Mutation::AddOwned.confirmed(&before["game"], &variant_id)
        {
            return Err(ServiceError::invalid(
                "Only an owned store version can be selected for play",
            ));
        }
        let root = mutation.root();
        let operation = format!("{}{}", root[..1].to_uppercase(), &root[1..]);
        let (variable, argument, id) = if mutation.favorite() {
            ("appId", "appId", &app_id)
        } else {
            ("cmsId", "variantId", &variant_id)
        };
        let query = format!(
            "mutation {operation}(${variable}: String!, $locale: String!) {{ {root}(language: $locale, {argument}: ${variable}) {{ app {{ id }} }} }}"
        );
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let current =
            self.authenticated_snapshot_for(&session, generation, TokenPurpose::ServiceId)?;
        let token = current
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&current.tokens.access_token);
        self.check_scope(&session, generation)?;
        self.invalidate_catalog()?;
        let sent = self.store_cache.requests.send(
            client
                .post(&self.endpoints.graphql)
                .headers(graphql_headers(token)?)
                .json(&json!({"query":query,"variables":{variable:id,"locale":"en_US"}})),
            "Catalog update could not be confirmed",
        );
        let mut http_status = None;
        let mut graphql_errors = Vec::new();
        let error_code = match sent {
            Ok(response) => {
                http_status = Some(response.status().as_u16());
                let success = response.status().is_success();
                let mut bytes = Vec::new();
                let payload = response
                    .take(64 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .ok()
                    .filter(|_| bytes.len() <= 64 * 1024)
                    .and_then(|_| serde_json::from_slice::<Value>(&bytes).ok());
                if let Some(payload) = &payload {
                    for error in payload["errors"].as_array().into_iter().flatten().take(8) {
                        let safe = |value: &Value| {
                            value
                                .as_str()
                                .filter(|text| {
                                    text.len() <= 128
                                        && text
                                            .chars()
                                            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
                                })
                                .map(ToOwned::to_owned)
                        };
                        graphql_errors.push(json!({"code":safe(&error["extensions"]["code"]),"path":error["path"].as_array().map(|path| path.iter().take(8).filter_map(safe).collect::<Vec<_>>())}));
                    }
                }
                if !success {
                    Some(if http_status == Some(401) {
                        "http_unauthorized"
                    } else {
                        "upstream_error"
                    })
                } else if payload.as_ref().is_some_and(|payload| {
                    !payload["errors"].is_null()
                        && payload["errors"]
                            .as_array()
                            .is_none_or(|errors| !errors.is_empty())
                }) {
                    Some("graphql_error")
                } else if payload.is_none() {
                    Some("invalid_upstream_response")
                } else if payload
                    .as_ref()
                    .is_none_or(|payload| payload["data"][root]["app"]["id"] != app_id)
                {
                    Some("mutation_identity_mismatch")
                } else {
                    None
                }
            }
            Err(error) => Some(error.code),
        };
        let invalidation = self.invalidate_catalog();
        self.check_scope(&session, generation)?;
        let fresh = self.catalog_game(&json!({"appId":app_id}), settings);
        self.check_scope(&session, generation)?;
        let confirmed = invalidation.is_ok()
            && fresh
                .as_ref()
                .is_ok_and(|result| mutation.confirmed(&result["game"], &variant_id));
        let message = if confirmed {
            "The current cloud library state is confirmed."
        } else {
            "Could not confirm the update. Refresh this game before trying again; the update may already have reached GeForce NOW."
        };
        Ok(scoped_result(
            json!({"appId":app_id,"variantId":variant_id,"operation":method,
            "outcome":if error_code.is_none() {"acknowledged"} else {"unconfirmed"},
            "reconciliation":if confirmed {"confirmed"} else {"unconfirmed"},"message":message,
            "error":{"code":error_code,"httpStatus":http_status,"graphql":graphql_errors,
                "reconciliationCode":fresh.as_ref().err().map(|error| error.code),"invalidationCode":invalidation.as_ref().err().map(|error| error.code)},
            "game":fresh.as_ref().ok().map(|result| &result["game"]),
            "catalogRevision":self.catalog_revision.load(std::sync::atomic::Ordering::Acquire)}),
            &session,
            generation,
        ))
    }

    pub fn catalog_favorites(&self, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let revision = self.catalog_revision.load(std::sync::atomic::Ordering::Acquire);
            let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
            let token = session.tokens.id_token.as_deref().unwrap_or(&session.tokens.access_token);
            let vpc = self.vpc_id(&client, session, generation, settings, token, Some(&self.store_cache.requests))?;
            let query = STORE_PANELS_QUERY.replace("GetStorePanels", "GetGameSection")
                .replace("      title\n      items", "      title\n      seeMoreInfo { filterTileId title filterIds minTiles sortOrderId }\n      items");
            let payload = self.catalog_document(&client, token, &query, json!({"vpcId":vpc,"locale":"en_US","panelNames":["FAVORITES"]}))?;
            let panels = payload["data"]["panels"].as_array().ok_or_else(|| ServiceError::invalid("Favorites panels were not returned"))?;
            let mut games = Vec::new();
            let mut sections = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for panel in panels {
                if panel["name"] != "FAVORITES" {
                    return Err(ServiceError::invalid("The returned panel is not the requested favorites panel"));
                }
                for section in panel["sections"].as_array().ok_or_else(|| ServiceError::invalid("Favorites sections were not returned"))? {
                    sections.push(json!({"id":section["id"],"title":section["title"],"seeMoreInfo":section["seeMoreInfo"]}));
                    for item in section["items"].as_array().ok_or_else(|| ServiceError::invalid("Favorites items were not returned"))? {
                        if item["__typename"] != "GameItem" { continue; }
                        let game = app_to_game(&item["app"]).ok_or_else(|| ServiceError::invalid("A favorites game could not be parsed"))?;
                        if seen.insert(game["id"].as_str().unwrap().to_owned()) { games.push(game); }
                        if games.len() > 1000 { return Err(ServiceError::invalid("Favorites exceed the bounded panel budget")); }
                    }
                }
            }
            self.check_scope(session, generation)?;
            if revision != self.catalog_revision.load(std::sync::atomic::Ordering::Acquire) { return Err(ServiceError {code:"stale_account",message:"Favorites changed while loading. Refresh again.".into()}); }
            crate::store_catalog_page::bounded_result(json!({"games":games,"sections":sections,"coverage":"unknown","complete":false,"fetchedAt":now_ms(),"catalogRevision":revision}))
        })
    }
}
