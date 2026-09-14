use super::*;

pub(super) const STORE_PANELS_QUERY: &str = r#"query GetStorePanels($vpcId: String!, $locale: String!, $panelNames: [String]!) {
  panels(vpcId: $vpcId, language: $locale, names: $panelNames) {
    id
    name
    sections {
      id
      title
      items {
        __typename
        ... on GameItem {
          app {
            id
            title
            publisherName
            images { GAME_BOX_ART KEY_IMAGE KEY_ART HERO_IMAGE TV_BANNER MARQUEE_HERO_IMAGE }
            itemMetadata { campaignIds }
            variants {
              id
              appStore
              storeUrl
              supportedControls
              gfn {
                status
                library { status selected }
              }
            }
            gfn { playType playabilityState minimumMembershipTierLabel }
          }
        }
      }
    }
  }
}"#;

const STORE_MARQUEE_QUERY: &str = r#"query GetStoreMarquee($vpcId: String!, $locale: String!, $panelNames: [String]!) {
  panels(vpcId: $vpcId, language: $locale, names: $panelNames) {
    id
    name
    sections {
      id
      title
      items {
        __typename
        ... on MarketingItem {
          id
          title
          body
          images { MARQUEE_HERO_IMAGE HERO_IMAGE }
          action { uri label }
        }
        ... on GameItem {
          app {
            id
            title
            publisherName
            images { GAME_BOX_ART KEY_IMAGE KEY_ART HERO_IMAGE TV_BANNER MARQUEE_HERO_IMAGE }
            itemMetadata { campaignIds }
            variants {
              id
              appStore
              storeUrl
              supportedControls
              gfn {
                status
                library { status selected }
              }
            }
            gfn { playType playabilityState minimumMembershipTierLabel }
          }
        }
      }
    }
  }
}"#;

const STORE_DEFINITIONS_QUERY: &str = r#"query GetStoreFilterDefinitions($locale: String!) {
  filterGroupDefinitions(language: $locale) {
    id
    label
    filters {
      id
      label
      filters
    }
  }
  sortOrderDefinitions(language: $locale) {
    id
    label
    orderBy
  }
}"#;

const STORE_MARQUEE_SHA: &str = "dd4bddfdef4707dfe340cc2040d6bb9c4c45f706976fca15b2ef33221c385d7f";
const STORE_PANELS_SHA: &str = "46ec15f267a056e7d5e46e629efa929529e5e7542a4850faece90b9f8fa5f810";

const STORE_BROWSE_QUERY: &str = r#"query GetStoreBrowseApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      library { favorited }
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls paymentModels { __typename } subscriptions
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate playStatus installed subscription }
          stateDetails { __typename ... on VariantGfnAutoPatchingMetadata { subType startTime endTime historicalEtaMins etaPredictionType } ... on VariantGfnManualPatchingMetadata { subType startTime endTime } ... on VariantGfnMaintenanceMetadata { subType } }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

const STORE_SEARCH_QUERY: &str = r#"query GetStoreSearchApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $searchString: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, searchQuery: $searchString, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      library { favorited }
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls paymentModels { __typename } subscriptions
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate playStatus installed subscription }
          stateDetails { __typename ... on VariantGfnAutoPatchingMetadata { subType startTime endTime historicalEtaMins etaPredictionType } ... on VariantGfnManualPatchingMetadata { subType startTime endTime } ... on VariantGfnMaintenanceMetadata { subType } }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

const LIBRARY_QUERY: &str = r#"query GetLibraryApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      library { favorited }
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls paymentModels { __typename } subscriptions
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate playStatus installed subscription }
          stateDetails { __typename ... on VariantGfnAutoPatchingMetadata { subType startTime endTime historicalEtaMins etaPredictionType } ... on VariantGfnManualPatchingMetadata { subType startTime endTime } ... on VariantGfnMaintenanceMetadata { subType } }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

impl GfnService {
    pub fn catalog_languages(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let (generation, provider) = {
            let state = self.state.lock().expect("GFN state poisoned");
            (
                state.generation,
                state
                    .session
                    .as_ref()
                    .map(|session| session.provider.clone()),
            )
        };
        let proxy = config_from_settings(settings).map_err(ServiceError::invalid)?;
        let scope = json!([
            "languages-v1",
            self.endpoints.public_graphql,
            provider,
            proxy.map(|config| config.cache_scope)
        ]);
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let mut result = self.store_cache.load_with_policy(&scope, &json!(["languages"]), params["refresh"] == true, crate::store_cache::CachePolicy {ttl_ms:14 * 24 * 60 * 60 * 1000,allow_stale:true}, || {
            let response = self.store_cache.requests.send(client.post(&self.endpoints.public_graphql)
                .header(ACCEPT, "application/json")
                .json(&json!({"query":"{ overallGfnSupportedLanguages { language } }"})), "Supported-language query failed")?;
            let payload = catalog_payload(response)?;
            let items = payload["data"]["overallGfnSupportedLanguages"].as_array()
                .filter(|items| !items.is_empty() && items.len() <= 512).ok_or_else(crate::catalog_types::invalid_metadata)?;
            let mut languages = Vec::new();
            for item in items {
                let language = item["language"].as_str().filter(|value| crate::language::valid_game_language(value))
                    .ok_or_else(crate::catalog_types::invalid_metadata)?;
                if !languages.contains(&language) { languages.push(language); }
            }
            if self.auth_generation() != generation { return Err(stale_catalog_scope()); }
            Ok(json!({"metadataKind":"languages","languages":languages,"source":"overallGfnSupportedLanguages"}))
        })?;
        if self.auth_generation() != generation {
            return Err(stale_catalog_scope());
        }
        result["scopeGeneration"] = json!(generation);
        result["source"] = json!("overallGfnSupportedLanguages");
        if !result["languages"].is_array() {
            result["languages"] = json!([]);
        }
        if result["languages"].as_array().is_some_and(|languages| {
            languages.len() > 512
                || languages.iter().any(|language| {
                    !language
                        .as_str()
                        .is_some_and(crate::language::valid_game_language)
                })
        }) {
            return Err(crate::catalog_types::invalid_metadata());
        }
        Ok(result)
    }

    pub fn catalog_definitions(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            self.definitions_for(session, generation, params, settings)
        })
    }

    pub(super) fn definitions_for(
        &self,
        session: &AuthSession,
        generation: u64,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let token = session
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&session.tokens.access_token);
        let scope = json!([
            "definitions-v1",
            session.user.user_id,
            session.provider,
            generation,
            self.endpoints.graphql,
            config_from_settings(settings)
                .map_err(ServiceError::invalid)?
                .map(|proxy| proxy.cache_scope),
            "en_US"
        ]);
        let mut sections = json!({});
        for (name, fields) in [
            (
                "stores",
                "appStoreDefinitions(language:$locale) { store label sortOrder smallImageUrl features { __typename ... on AccountLinkingSso { supported displayProposition } ... on AccountGamesSyncing { supported displayProposition } ... on AccountSubscriptions { displayProposition } } accountLinkingMetadata { supportedVariantIds isSupported isRequired label } }",
            ),
            (
                "genres",
                "genreDefinitions(language:$locale) { genre label }",
            ),
            (
                "subscriptions",
                "subscriptionDefinitions(language:$locale) { subscription label logoURL buySubscriptionURL primaryStore additionalStores { store thirdPartyLinkingRedirectKeyword thirdPartyLinkingShortURL } }",
            ),
        ] {
            self.check_scope(session, generation)?;
            let section =
                self.store_cache.load_with_policy(
                    &scope,
                    &json!(["definitions", name]),
                    params["refresh"] == true,
                    crate::store_cache::CachePolicy {
                        ttl_ms: 24 * 60 * 60 * 1000,
                        allow_stale: true,
                    },
                    || {
                        let payload = self.catalog_document(
                            &client,
                            token,
                            &format!("query OpenNowDefinitions($locale:String!) {{ {fields} }}"),
                            json!({"locale":"en_US"}),
                        )?;
                        let items = match name {
                            "stores" => crate::catalog_types::parse_list::<
                                crate::catalog_types::StoreDefinition,
                            >(
                                &payload["data"]["appStoreDefinitions"]
                            ),
                            "genres" => crate::catalog_types::parse_list::<
                                crate::catalog_types::GenreDefinition,
                            >(
                                &payload["data"]["genreDefinitions"]
                            ),
                            _ => crate::catalog_types::parse_list::<
                                crate::catalog_types::SubscriptionDefinition,
                            >(
                                &payload["data"]["subscriptionDefinitions"]
                            ),
                        }?;
                        self.check_scope(session, generation)?;
                        Ok(json!({"metadataKind":name,"items":items,"source":"server"}))
                    },
                )?;
            sections[name] = section;
        }
        Ok(sections)
    }

    pub fn catalog_game(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let app_id = params
            .get("appId")
            .map(|_| bounded_id(params, "appId", true))
            .transpose()?;
        let variant_id = params
            .get("variantId")
            .map(|_| bounded_id(params, "variantId", true))
            .transpose()?;
        if app_id.is_some() == variant_id.is_some() {
            return Err(ServiceError::invalid(
                "Provide exactly one appId or variantId",
            ));
        }
        let cms_id = variant_id
            .as_ref()
            .map(|value| {
                value
                    .parse::<i32>()
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| {
                        ServiceError::invalid("variantId must be a positive GraphQL Int")
                    })
            })
            .transpose()?;
        self.authenticated_read(|session,generation| {
            let revision = self.catalog_revision.load(std::sync::atomic::Ordering::Acquire);
            let client = client_for_settings(&self.client,settings).map_err(ServiceError::invalid)?;
            let token = session.tokens.id_token.as_deref().unwrap_or(&session.tokens.access_token);
            let vpc = self.vpc_id(&client,session,generation,settings,token,Some(&self.store_cache.requests))?;
            let (kind, ty, ids) = if let Some(id) = &app_id { ("appIds","String",json!([id])) } else { ("variantIds","Int",json!([cms_id])) };
            let fields = STORE_BROWSE_QUERY.split("    items {").nth(1).expect("catalog item selection");
            let fields = fields.strip_suffix("  }\n}").expect("catalog query suffix");
            let fields = fields.replace("id title developerName", "shortDescription computedValues { paymentModels { __typename } } id title developerName")
                .replace("itemMetadata { campaignIds }", "")
                .replace("status\n          features", "status\n          supportedLanguages { language ... on GfnLanguageSettings { availableFeatures setMethod } }\n          features");
            let query = format!("query OpenNowGame($vpcId:String!,$locale:String!,$ids:[{ty}]!) {{ apps(vpcId:$vpcId,language:$locale,{kind}:$ids) {{ items {{ {fields} }} }}");
            let payload = self.catalog_document(&client,token,&query,json!({"vpcId":vpc,"locale":"en_US","ids":ids}))?;
            let games = complete_games(&payload["data"]["apps"]["items"])?;
            let game = games.into_iter().find(|game| {
                app_id.as_ref().is_some_and(|id| game["id"] == *id)
                    || variant_id.as_ref().is_some_and(|id| game["variants"].as_array().is_some_and(|variants| variants.iter().any(|variant| variant["id"] == *id)))
            }).ok_or_else(|| ServiceError { code:"catalog_game_not_found", message:"The exact catalog game was not returned".into() })?;
            self.check_scope(session,generation)?;
            self.check_catalog_revision(revision)?;
            crate::store_catalog_page::bounded_result(json!({"game":game,"catalogRevision":revision,"freshness":"fresh","fetchedAt":now_ms()}))
        })
    }

    pub fn library_catalog(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let page = crate::store_catalog_page::PageRequest::parse(params)?;
            let traversal = bounded_id(params, "traversalId", false)?;
            let revision = self.catalog_revision.load(std::sync::atomic::Ordering::Acquire);
            if !page.cursor.is_empty() && params["catalogRevision"].as_u64() != Some(revision) {
                return Err(ServiceError { code: "catalog_changed", message: "The catalog changed. Restart the library refresh.".into() });
            }
            let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
            let token = session.tokens.id_token.as_deref().unwrap_or(&session.tokens.access_token);
            let vpc = self.vpc_id(&client, session, generation, settings, token, None)?;
            let context = format!("{:x}", Sha256::digest(json!([session.user.user_id,session.provider,generation,
                self.endpoints.graphql,vpc,config_from_settings(settings).map_err(ServiceError::invalid)?.map(|proxy| proxy.cache_scope),revision,"en_US"]).to_string().as_bytes()));
            if !page.cursor.is_empty() && params["catalogContext"] != context {
                return Err(ServiceError { code:"catalog_changed", message:"The catalog route changed. Restart the library refresh.".into() });
            }
            crate::store_catalog_page::fetch_bounded_page(page.limit, |count| {
                self.check_scope(session,generation)?;
                self.check_catalog_revision(revision)?;
                let payload = self.catalog_document(&client, token, LIBRARY_QUERY, json!({
                    "vpcId":vpc, "locale":"en_US", "sortString":"variants.gfn.library.lastPlayedDate:DESC,computedValues.libraryAddedDate:DESC,sortName:ASC",
                    "fetchCount":count, "cursor":page.cursor,
                    "filters":{"variants":{"gfn":{"library":{"status":{"notEquals":"NOT_OWNED"}}}}}
                }))?;
                self.check_scope(session, generation)?;
                self.check_catalog_revision(revision)?;
                let apps = &payload["data"]["apps"];
                let games = complete_games(&apps["items"])?;
                let mut result = crate::store_catalog_page::page_result(&page.cursor, games, &apps["pageInfo"], now_ms())?;
                result["source"] = json!("account-library");
                result["freshness"] = json!("fresh");
                result["catalogRevision"] = json!(revision);
                result["catalogContext"] = json!(context);
                result["traversalId"] = json!(traversal);
                Ok(result)
            })
        })
    }

    fn check_catalog_revision(&self, revision: u64) -> Result<(), ServiceError> {
        if self
            .catalog_revision
            .load(std::sync::atomic::Ordering::Acquire)
            != revision
        {
            return Err(ServiceError {
                code: "catalog_changed",
                message: "The catalog changed. Restart this request.".into(),
            });
        }
        Ok(())
    }

    pub(super) fn invalidate_catalog(&self) -> Result<(), ServiceError> {
        let revision = self
            .catalog_revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.store_cache.invalidate(revision)
    }

    pub(super) fn catalog_document(
        &self,
        client: &Client,
        token: &str,
        query: &str,
        variables: Value,
    ) -> Result<Value, ServiceError> {
        let response = self.store_cache.requests.send(
            client
                .post(&self.endpoints.graphql)
                .headers(graphql_headers(token)?)
                .json(&json!({"query":query,"variables":variables})),
            "Catalog query failed",
        )?;
        if !response.status().is_success() {
            let status = response.status();
            let mut bytes = Vec::new();
            let payload = response
                .take(16 * 1024 + 1)
                .read_to_end(&mut bytes)
                .ok()
                .filter(|_| bytes.len() <= 16 * 1024)
                .and_then(|_| serde_json::from_slice::<Value>(&bytes).ok());
            let messages = payload
                .as_ref()
                .into_iter()
                .flat_map(|value| value["errors"].as_array().into_iter().flatten())
                .take(4)
                .filter_map(|error| error["message"].as_str())
                .map(|message| {
                    crate::diagnostics::runtime_failure_reason(
                        &message.replace(token, "[redacted]"),
                    )
                })
                .collect::<Vec<_>>();
            return Err(ServiceError {
                code: if status.as_u16() == 401 {
                    "http_unauthorized"
                } else {
                    "upstream_error"
                },
                message: format!("Catalog query failed ({status}): {}", messages.join("; ")),
            });
        }
        catalog_payload(response)
    }

    pub fn store_local_catalog(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let revision = self
                .catalog_revision
                .load(std::sync::atomic::Ordering::Acquire);
            let mut scope = self.store_cache_scope(session, generation, settings)?;
            let refreshed_scope = || {
                let current =
                    self.authenticated_snapshot_for(session, generation, TokenPurpose::ServiceId)?;
                self.store_cache_scope(&current, generation, settings)
            };
            if params["refresh"] == true {
                self.store_catalog(&json!({"limit":100,"refresh":true}), settings)?;
                scope = refreshed_scope()?;
            }
            let mut result = match self.store_cache.local_query(&scope, params) {
                Err(error) if error.code == "store_cache_missing" => {
                    self.store_catalog(
                        &json!({"limit":100,"cursor":"","searchQuery":""}),
                        settings,
                    )?;
                    scope = refreshed_scope()?;
                    self.store_cache.local_query(&scope, params)?
                }
                result => result?,
            };
            // A cold/partial cache grows by at most one upstream page per explicit
            // demand. Never crawl the entire catalog on startup or a search keypress.
            if result["count"] == 0 && result["cacheComplete"] == false {
                if let Some(cursor) = result["upstreamCursor"].as_str().filter(|s| !s.is_empty()) {
                    self.store_catalog(
                        &json!({"limit":100,"cursor":cursor,"searchQuery":""}),
                        settings,
                    )?;
                    scope = refreshed_scope()?;
                    result = self.store_cache.local_query(&scope, params)?;
                }
            }
            result
                .as_object_mut()
                .expect("Local Store response")
                .remove("upstreamCursor");
            self.check_scope(session, generation)?;
            self.check_catalog_revision(revision)?;
            result["catalogRevision"] = json!(revision);
            Ok(result)
        })
    }

    pub fn store_catalog(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let revision = self
                .catalog_revision
                .load(std::sync::atomic::Ordering::Acquire);
            self.check_scope(session, generation)?;
            let page = crate::store_catalog_page::PageRequest::parse(params)?;
            let client =
                client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
            let token = session
                .tokens
                .id_token
                .as_deref()
                .unwrap_or(&session.tokens.access_token);
            let scope = self.store_cache_scope(session, generation, settings)?;
            let mut filters = json!({});
            let mut sort = "itemMetadata.relevance:DESC,sortName:ASC".to_owned();
            if params.get("filterId").is_some() || params.get("sortId").is_some() {
                let definitions =
                    self.store_presentation(&json!({"section":"filters"}), settings)?;
                if params.get("filterId").is_some() {
                    let id = bounded_id(params, "filterId", true)?;
                    filters = definitions["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|group| group["options"].as_array().into_iter().flatten())
                        .find(|option| option["id"] == id)
                        .map(|option| option["expression"].clone())
                        .filter(Value::is_object)
                        .ok_or_else(|| {
                            ServiceError::invalid("The server filter expression is unavailable")
                        })?;
                }
                if params.get("sortId").is_some() {
                    let id = bounded_id(params, "sortId", true)?;
                    sort = definitions["sortOrders"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|option| option["id"] == id)
                        .and_then(|option| option["orderBy"].as_str())
                        .filter(|value| !value.is_empty() && value.len() <= 1024)
                        .ok_or_else(|| {
                            ServiceError::invalid("The server sort expression is unavailable")
                        })?
                        .to_owned();
                }
            }
            let key = if filters == json!({}) && sort == "itemMetadata.relevance:DESC,sortName:ASC"
            {
                json!(["page", page.limit, page.cursor, page.search])
            } else {
                json!(["page", page.limit, page.cursor, page.search, filters, sort])
            };
            let refresh = params["refresh"].as_bool() == Some(true) && page.cursor.is_empty();
            if params["revalidate"] == true {
                self.store_cache.invalidate_key(&scope, &key);
            }
            let mut result = self.store_cache.load_or_fetch(&scope, &key, refresh, || {
                let vpc_id = self.vpc_id(
                    &client,
                    session,
                    generation,
                    settings,
                    token,
                    Some(&self.store_cache.requests),
                )?;
                // Each retry starts at the SAME cursor. Never truncate a fetched page:
                // doing so would skip games when returning NVIDIA's end cursor.
                crate::store_catalog_page::fetch_bounded_page(page.limit, |fetch_count| {
                    let searching = !page.search.is_empty();
                    let query = if searching {
                        STORE_SEARCH_QUERY
                    } else {
                        STORE_BROWSE_QUERY
                    };
                    let mut variables = json!({
                        "vpcId":vpc_id, "locale":"en_US",
                        "sortString":sort,
                        "fetchCount":fetch_count, "cursor":page.cursor, "filters":filters
                    });
                    if searching {
                        variables["searchString"] = Value::String(page.search.clone());
                    }
                    let response = self.store_cache.requests.send(
                        client
                            .post(&self.endpoints.graphql)
                            .headers(graphql_headers(token)?)
                            .json(&json!({"query":query,"variables":variables})),
                        "GFN store query failed",
                    )?;
                    let payload = catalog_payload(response)?;
                    let apps = &payload["data"]["apps"];
                    let items = apps["items"].as_array().ok_or_else(|| ServiceError {
                        code: "invalid_upstream_response",
                        message: "Store response has no games array".to_owned(),
                    })?;
                    let games = complete_games(&Value::Array(items.clone()))?;
                    self.check_scope(session, generation)?;
                    self.check_catalog_revision(revision)?;
                    crate::store_catalog_page::page_result(
                        &page.cursor,
                        games,
                        &apps["pageInfo"],
                        now_ms(),
                    )
                })
            })?;
            self.check_catalog_revision(revision)?;
            result["catalogRevision"] = json!(revision);
            Ok(result)
        })
    }

    fn store_cache_scope(
        &self,
        session: &AuthSession,
        generation: u64,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        let proxy = config_from_settings(settings).map_err(ServiceError::invalid)?;
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
        Ok(json!([
            session.user.user_id,
            session.provider,
            generation,
            session.user.membership_tier,
            proxy
                .map(|config| config.cache_scope)
                .unwrap_or_else(|| "direct".into()),
            "en_US",
            self.endpoints.graphql,
            vpc,
            self.catalog_revision
                .load(std::sync::atomic::Ordering::Acquire),
            "catalog-v2"
        ]))
    }

    pub fn store_presentation(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let revision = self.catalog_revision.load(std::sync::atomic::Ordering::Acquire);
            let section = params["section"].as_str().unwrap_or("");
            if !matches!(section, "marquee" | "panels" | "filters") {
                return Err(ServiceError::invalid(
                    "Store presentation requires marquee, panels or filters",
                ));
            }
            let client =
                client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
            let scope = self.store_cache_scope(session, generation, settings)?;
            let mut result = self.store_cache.load_or_fetch(
                &scope,
                &json!(["presentation", section]),
                false,
                || {
                    let token = session
                        .tokens
                        .id_token
                        .as_deref()
                        .unwrap_or(&session.tokens.access_token);
                    let vpc_id = self.vpc_id(
                        &client,
                        session,
                        generation,
                        settings,
                        token,
                        Some(&self.store_cache.requests),
                    )?;
                    let (variables, request_type, sha, query) = match section {
                        "panels" => (
                            json!({"vpcId":vpc_id,"locale":"en_US","panelNames":["MAIN"]}),
                            "panels/MainV2",
                            STORE_PANELS_SHA,
                            STORE_PANELS_QUERY,
                        ),
                        "marquee" => (
                            json!({"vpcId":vpc_id,"locale":"en_US","panelNames":["MARQUEE"]}),
                            "panels/Marquee",
                            STORE_MARQUEE_SHA,
                            STORE_MARQUEE_QUERY,
                        ),
                        _ => (
                            json!({"locale":"en_US"}),
                            "filterGroupAndSortOrderDefinitions",
                            "ef725de5e93b093de1ac7418fed0ffb4f6ae2b9c14f743ab274a791521488eb9",
                            STORE_DEFINITIONS_QUERY,
                        ),
                    };
                    let payload = if section == "panels" {
                        // The persisted MainV2 document only supplies landscape hero art.
                        // Execute our document so GAME_BOX_ART is actually requested, just
                        // like Library/browse, rather than silently ignoring these fields.
                        self.check_scope(session, generation)?;
                        let response = self.store_cache.requests.send(
                            client
                                .post(&self.endpoints.graphql)
                                .headers(graphql_headers(token)?)
                                .json(&json!({"query":query,"variables":variables})),
                            "GFN storefront query failed",
                        )?;
                        if !response.status().is_success() {
                            return Err(ServiceError::response(
                                "GFN storefront query failed",
                                response,
                            ));
                        }
                        let payload = response.json::<Value>().map_err(|error| {
                            ServiceError::network("Invalid GFN storefront response", error)
                        })?;
                        if let Some(message) = graphql_error_message(&payload) {
                            return Err(ServiceError {
                                code: "graphql_error",
                                message,
                            });
                        }
                        payload
                    } else {
                        fetch_panels_document(
                            &self.store_cache.requests,
                            &client,
                            token,
                            variables,
                            request_type,
                            sha,
                            query,
                        )?
                    };
                    self.check_scope(session, generation)?;
                    let empty_index = HashMap::new();
                    let items = match section {
                        "panels" => parse_store_panels(&payload, &empty_index),
                        "marquee" => parse_store_marquee(&payload, &empty_index),
                        _ => parse_store_definitions(&payload),
                    };
                    // Optional chrome must not enlarge the games response or restart the core.
                    // An oversized/failed section is reported independently by the shell.
                    crate::store_catalog_page::bounded_result(
                        json!({"section":section,"items":items,"sortOrders":payload["data"]["sortOrderDefinitions"]}),
                    )
                },
            )?;
            if section == "panels" && params["metadataOnly"] == true {
                for panel in result["items"].as_array_mut().into_iter().flatten() {
                    for section in panel["sections"].as_array_mut().into_iter().flatten() {
                        let count = section["games"].as_array().map_or(0, Vec::len);
                        section["totalCount"] = json!(count);
                        section["games"] = json!([]);
                    }
                }
            }
            self.check_catalog_revision(revision)?;
            result["catalogRevision"] = json!(revision);
            Ok(result)
        })
    }
}

pub(super) fn app_to_game(app: &Value) -> Option<Value> {
    let id = app["id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256)?
        .to_owned();
    let title = app["title"].as_str()?.trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let mut variants = app["variants"].as_array().into_iter().flatten().filter_map(|variant| {
        let variant_id = variant["id"].as_str()?.to_owned();
        let store = variant["appStore"].as_str().unwrap_or("Unknown").to_owned();
        let library_status = variant["gfn"]["library"]["status"].as_str().map(ToOwned::to_owned);
        let in_library = library_status.as_deref().is_some_and(|status| matches!(status, "MANUAL" | "PLATFORM_SYNC" | "IN_LIBRARY"));
        let supports_persistence = gfn_feature_enabled(
            &variant["gfn"]["features"],
            "IN_GAME_SETTINGS_PERSISTENCE_ENABLED",
        );
        Some(json!({
            "id":variant_id,
            "store":store,
            "storeUrl":variant["storeUrl"],
            "supportedControls":variant["supportedControls"].as_array().cloned().unwrap_or_default(),
            "librarySelected":variant["gfn"]["library"]["selected"],
            "inLibrary":in_library,
            "libraryStatus":library_status,
            "lastPlayedDate":variant["gfn"]["library"]["lastPlayedDate"],
            "playStatus":variant["gfn"]["library"]["playStatus"],
            "installed":variant["gfn"]["library"]["installed"],
            "subscription":variant["gfn"]["library"]["subscription"],
            "paymentModels":variant["paymentModels"],
            "subscriptions":variant["subscriptions"],
            "supportedLanguages":variant["gfn"]["supportedLanguages"],
            "stateDetails":serde_json::from_value::<crate::catalog_types::PatchDetails>(variant["gfn"]["stateDetails"].clone()).ok(),
            "gfnStatus":variant["gfn"]["status"],
            "supportsInGameSettingsPersistence":supports_persistence,
        }))
    }).collect::<Vec<_>>();
    let mut variant_ids = std::collections::HashSet::new();
    variants.retain(|variant| variant_ids.insert(variant["id"].as_str().unwrap().to_owned()));
    if variants.is_empty() {
        return None;
    }
    let selected_index = variants
        .iter()
        .position(|variant| variant["librarySelected"].as_bool() == Some(true))
        .or_else(|| {
            variants
                .iter()
                .position(|variant| variant["inLibrary"].as_bool() == Some(true))
        })
        .unwrap_or(0);
    let launch_id = variants
        .get(selected_index)
        .and_then(|variant| variant["id"].as_str())
        .filter(|value| value.chars().all(|character| character.is_ascii_digit()))
        .or_else(|| {
            variants
                .iter()
                .filter_map(|variant| variant["id"].as_str())
                .find(|value| value.chars().all(|character| character.is_ascii_digit()))
        })
        .or_else(|| {
            id.chars()
                .all(|character| character.is_ascii_digit())
                .then_some(id.as_str())
        })
        .map(ToOwned::to_owned);
    let available_stores = variants
        .iter()
        .filter_map(|variant| variant["store"].as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let genres = string_array(&app["genres"]);
    let controls = string_array(&app["supportedControls"]);
    let image_url = first_image(
        &app["images"],
        &[
            "GAME_BOX_ART",
            "KEY_IMAGE",
            "KEY_ART",
            "HERO_IMAGE",
            "TV_BANNER",
        ],
        900,
    );
    let hero_image_url = first_image(
        &app["images"],
        &[
            "MARQUEE_HERO_IMAGE",
            "HERO_IMAGE",
            "TV_BANNER",
            "FEATURE_IMAGE",
            "KEY_IMAGE",
            "KEY_ART",
        ],
        1200,
    );
    let key_art_url = first_image(&app["images"], &["KEY_ART", "KEY_IMAGE"], 900);
    let screenshots = image_values(&app["images"]["SCREENSHOTS"], 1200);
    let publisher = app["publisherName"].as_str().map(ToOwned::to_owned);
    let developer = app["developerName"].as_str().map(ToOwned::to_owned);
    let search_text = [
        vec![title.clone()],
        publisher.clone().into_iter().collect(),
        developer.clone().into_iter().collect(),
        available_stores.clone(),
        genres.clone(),
    ]
    .concat()
    .join(" ")
    .to_lowercase();
    let is_in_library = variants
        .iter()
        .any(|variant| variant["inLibrary"].as_bool() == Some(true));
    let last_played = variants
        .iter()
        .filter_map(|variant| variant["lastPlayedDate"].as_str())
        .next()
        .map(ToOwned::to_owned);
    Some(json!({
        "id":id,
        "uuid":id,
        "launchAppId":launch_id,
        "title":title,
        "developerName":developer,
        "publisherName":publisher,
        "genres":genres,
        "supportedControls":controls,
        "imageUrl":image_url,
        "heroImageUrl":hero_image_url,
        "keyArtUrl":key_art_url,
        "screenshotUrl":screenshots.first(),
        "screenshotUrls":screenshots,
        "playType":app["gfn"]["playType"],
        "favorited":app["library"]["favorited"],
        "catalogSkuStrings":app["gfn"]["catalogSkuStrings"],
        "campaignIds":app["itemMetadata"]["campaignIds"],
        "paymentModels":app["computedValues"]["paymentModels"],
        "description":app["shortDescription"],
        "membershipTierLabel":app["gfn"]["minimumMembershipTierLabel"],
        "playabilityState":app["gfn"]["playabilityState"],
        "availableStores":available_stores,
        "searchText":search_text,
        "lastPlayed":last_played,
        "isInLibrary":is_in_library,
        "selectedVariantIndex":selected_index,
        "variants":variants,
    }))
}

pub(super) fn bounded_id(
    params: &Value,
    key: &str,
    required: bool,
) -> Result<String, ServiceError> {
    match params.get(key) {
        None if !required => Ok(String::new()),
        Some(Value::String(value))
            if !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control) =>
        {
            Ok(value.clone())
        }
        _ => Err(ServiceError::invalid(format!(
            "{key} must be a bounded nonempty identifier"
        ))),
    }
}

fn stale_catalog_scope() -> ServiceError {
    ServiceError {
        code: "stale_account",
        message: "The catalog context changed. Retry this request.".into(),
    }
}

pub(crate) fn catalog_payload(response: Response) -> Result<Value, ServiceError> {
    if !response.status().is_success() {
        return Err(ServiceError::response("Catalog query failed", response));
    }
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ServiceError {
            code: "network_error",
            message: "Catalog response could not be read".into(),
        })?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(ServiceError {
            code: "catalog_response_too_large",
            message: "Catalog response exceeds the receive budget".into(),
        });
    }
    let payload: Value = serde_json::from_slice(&bytes).map_err(|_| ServiceError {
        code: "invalid_upstream_response",
        message: "Catalog response is not valid JSON".into(),
    })?;
    if payload["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(ServiceError {
            code: "graphql_error",
            message: "The catalog query returned GraphQL errors".into(),
        });
    }
    Ok(payload)
}

fn complete_games(items: &Value) -> Result<Vec<Value>, ServiceError> {
    let items = items.as_array().ok_or_else(|| ServiceError {
        code: "invalid_upstream_response",
        message: "Catalog response has no games array".into(),
    })?;
    let mut games: Vec<Value> = Vec::new();
    let mut ids = HashMap::new();
    for item in items {
        let variants = item["variants"]
            .as_array()
            .filter(|variants| !variants.is_empty() && variants.len() <= 128)
            .ok_or_else(crate::catalog_types::invalid_metadata)?;
        if variants.iter().any(|variant| {
            variant["id"]
                .as_str()
                .is_none_or(|id| id.is_empty() || id.len() > 256)
        }) {
            return Err(crate::catalog_types::invalid_metadata());
        }
        let mut game = app_to_game(item).ok_or_else(|| ServiceError {
            code: "invalid_upstream_response",
            message: "Catalog page contains an invalid game identity".into(),
        })?;
        let id = game["id"].as_str().unwrap().to_owned();
        if let Some(index) = ids.get(&id).copied() {
            let existing: &Value = &games[index];
            let mut variants = existing["variants"].as_array().unwrap().clone();
            let selected = game["variants"]
                [game["selectedVariantIndex"].as_u64().unwrap_or(0) as usize]["id"]
                .clone();
            for variant in game["variants"].as_array().unwrap() {
                if let Some(index) = variants
                    .iter()
                    .position(|existing| existing["id"] == variant["id"])
                {
                    variants[index] = variant.clone();
                } else {
                    variants.push(variant.clone());
                }
            }
            game["selectedVariantIndex"] = json!(
                variants
                    .iter()
                    .position(|variant| variant["id"] == selected)
                    .unwrap_or(0)
            );
            game["availableStores"] = json!(
                variants
                    .iter()
                    .map(|variant| variant["store"].clone())
                    .collect::<Vec<_>>()
            );
            game["isInLibrary"] =
                json!(variants.iter().any(|variant| variant["inLibrary"] == true));
            game["variants"] = json!(variants);
            games[index] = game;
        } else {
            ids.insert(id, games.len());
            games.push(game);
        }
    }
    Ok(games)
}
