use crate::gfn::ServiceError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "__typename")]
pub enum StoreFeature {
    AccountLinkingSso {
        supported: Option<bool>,
        #[serde(rename = "displayProposition")]
        display_proposition: Option<String>,
    },
    AccountGamesSyncing {
        supported: Option<bool>,
        #[serde(rename = "displayProposition")]
        display_proposition: Option<String>,
    },
    AccountSubscriptions {
        #[serde(rename = "displayProposition")]
        display_proposition: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkingMetadata {
    pub supported_variant_ids: Option<Vec<String>>,
    pub is_supported: Option<bool>,
    pub is_required: Option<bool>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreDefinition {
    pub store: String,
    pub label: String,
    pub sort_order: Option<i64>,
    pub small_image_url: Option<String>,
    #[serde(default)]
    pub features: Vec<StoreFeature>,
    pub account_linking_metadata: Option<LinkingMetadata>,
}

impl StoreDefinition {
    pub fn connection_definition(&self) -> Value {
        let provider = normalize_store(&self.store);
        let known = matches!(
            provider.as_str(),
            "UPLAY" | "BATTLENET" | "EPIC" | "GAIJIN" | "STEAM" | "XBOX"
        );
        json!({
            "provider":provider,"store":self.store,"label":self.label,"sortOrder":self.sort_order,
            "supportsLinking":known && self.features.iter().any(|feature| matches!(feature, StoreFeature::AccountLinkingSso { supported:Some(true), .. })),
            "supportsSync":known && self.features.iter().any(|feature| matches!(feature, StoreFeature::AccountGamesSyncing { supported:Some(true), .. })),
            "isRequired":self.account_linking_metadata.as_ref().and_then(|metadata| metadata.is_required),
            "accountLinkingMetadata":self.account_linking_metadata,"features":self.features,
            "capabilitySource":"server"
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GenreDefinition {
    pub genre: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDefinition {
    pub subscription: String,
    pub label: String,
    #[serde(rename = "logoURL")]
    pub logo_url: Option<String>,
    #[serde(rename = "buySubscriptionURL")]
    pub buy_subscription_url: Option<String>,
    pub primary_store: Option<String>,
    pub additional_stores: Option<Vec<SubscriptionStore>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionStore {
    pub store: String,
    pub third_party_linking_redirect_keyword: Option<String>,
    #[serde(rename = "thirdPartyLinkingShortURL")]
    pub third_party_linking_short_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "__typename")]
pub enum PatchDetails {
    VariantGfnAutoPatchingMetadata {
        #[serde(rename = "subType")]
        sub_type: Option<String>,
        #[serde(rename = "startTime")]
        start_time: Option<String>,
        #[serde(rename = "endTime")]
        end_time: Option<String>,
        #[serde(rename = "historicalEtaMins")]
        historical_eta_mins: Option<f64>,
        #[serde(rename = "etaPredictionType")]
        eta_prediction_type: Option<String>,
    },
    VariantGfnManualPatchingMetadata {
        #[serde(rename = "subType")]
        sub_type: Option<String>,
        #[serde(rename = "startTime")]
        start_time: Option<String>,
        #[serde(rename = "endTime")]
        end_time: Option<String>,
    },
    VariantGfnMaintenanceMetadata {
        #[serde(rename = "subType")]
        sub_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

pub fn normalize_store(value: &str) -> String {
    match value
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '-'], "_")
        .as_str()
    {
        "UBISOFT" | "UBISOFT_CONNECT" => "UPLAY".into(),
        "BATTLE_NET" | "BLIZZARD" => "BATTLENET".into(),
        "EPIC_GAMES" | "EPIC_GAMES_STORE" => "EPIC".into(),
        value => value.into(),
    }
}

pub fn parse_list<T: serde::de::DeserializeOwned + Serialize>(
    value: &Value,
) -> Result<Value, ServiceError> {
    let values = value
        .as_array()
        .filter(|items| items.len() <= 512)
        .ok_or_else(invalid_metadata)?;
    if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > 256 * 1024) {
        return Err(invalid_metadata());
    }
    let parsed: Vec<T> = values
        .iter()
        .map(|item| serde_json::from_value(item.clone()))
        .collect::<Result<_, _>>()
        .map_err(|_| invalid_metadata())?;
    serde_json::to_value(parsed).map_err(|_| invalid_metadata())
}

pub fn invalid_metadata() -> ServiceError {
    ServiceError {
        code: "invalid_upstream_response",
        message: "Catalog metadata is missing, invalid, or exceeds its size limit".into(),
    }
}

pub fn filter_expression(value: &Value) -> Option<Value> {
    fn merge(target: &mut Value, source: Value, depth: usize) -> Option<()> {
        if depth > 8 {
            return None;
        }
        match (target, source) {
            (Value::Object(target), Value::Object(source)) => {
                if source.len() > 64 {
                    return None;
                }
                for (key, value) in source {
                    if let Some(target) = target.get_mut(&key) {
                        merge(target, value, depth + 1)?;
                    } else {
                        let mut field = if value.is_object() {
                            json!({})
                        } else if value.is_array() {
                            json!([])
                        } else {
                            Value::Null
                        };
                        merge(&mut field, value, depth + 1)?;
                        target.insert(key, field);
                    }
                }
            }
            (Value::Array(target), Value::Array(source)) => {
                if target.len() + source.len() > 128 {
                    return None;
                }
                target.extend(source);
            }
            (target, source) => *target = source,
        }
        Some(())
    }
    let inputs = value.as_array().filter(|items| items.len() <= 32)?;
    let mut output = json!({});
    for input in inputs {
        let text = input.as_str().filter(|text| text.len() <= 8192)?;
        let parsed: Value = serde_json::from_str(text).ok()?;
        if !parsed.is_object() {
            return None;
        }
        merge(&mut output, parsed, 0)?;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_definitions_preserve_nullable_variant_scope_and_link_requirements() {
        for variants in [Value::Null, json!([]), json!(["123"])] {
            let input = json!([{"store":"XBOX","label":"Xbox","features":[],
                "accountLinkingMetadata":{"supportedVariantIds":variants,"isRequired":true}}]);
            let parsed = parse_list::<StoreDefinition>(&input).unwrap();
            let store: StoreDefinition = serde_json::from_value(parsed[0].clone()).unwrap();
            let connection = store.connection_definition();
            assert_eq!(
                connection["accountLinkingMetadata"]["supportedVariantIds"],
                variants
            );
            assert_eq!(connection["isRequired"], true);
        }
        for invalid in [json!("123"), json!([123]), json!([null])] {
            assert!(
                parse_list::<StoreDefinition>(&json!([{"store":"XBOX","label":"Xbox",
                "accountLinkingMetadata":{"supportedVariantIds":invalid}}]))
                .is_err()
            );
        }
    }

    #[test]
    fn subscription_definitions_accept_null_but_reject_invalid_additional_stores() {
        for stores in [Value::Null, json!([]), json!([{"store":"UPLAY"}])] {
            let parsed = parse_list::<SubscriptionDefinition>(&json!([{
                "subscription":"STORE_PASS","label":"Store Pass","additionalStores":stores
            }]))
            .unwrap();
            assert_eq!(parsed[0]["additionalStores"].is_null(), stores.is_null());
            if let Some(stores) = stores.as_array() {
                assert_eq!(
                    parsed[0]["additionalStores"].as_array().unwrap().len(),
                    stores.len()
                );
                for (index, store) in stores.iter().enumerate() {
                    assert_eq!(
                        parsed[0]["additionalStores"][index]["store"],
                        store["store"]
                    );
                }
            }
        }
        for invalid in [json!("UPLAY"), json!(["UPLAY"]), json!([null])] {
            assert!(
                parse_list::<SubscriptionDefinition>(&json!([{
                    "subscription":"STORE_PASS","label":"Store Pass","additionalStores":invalid
                }]))
                .is_err()
            );
        }
    }
}
