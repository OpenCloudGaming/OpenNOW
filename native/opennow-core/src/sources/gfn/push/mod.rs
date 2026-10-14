mod config;
mod decrypt;
mod event;
mod owner;
mod protocol;
mod registration;
mod store;
mod transport;
mod vendor_default;

#[cfg(test)]
pub use config::FILE_NAME as PUSH_CONFIG_FILE;
pub use config::{PushConfig, load_for_provider as push_config_for_provider};
pub use event::{EVENT_NAME as PUSH_EVENT_NAME, payload as push_event_payload};
pub use owner::{
    GfnTokenSource, PushEvent, PushOwner, PushOwnerConfig, PushOwnerDeps, PushScope, PushSink,
    ScopeSource,
};
pub use registration::ReqwestPushHttp;
#[cfg(test)]
pub use registration::{HttpRequest, HttpResponse, PushHttp, Registration};
#[cfg(test)]
pub use store::PushStateStore;
pub use store::RegistrationStore;
pub use vendor_default::config_for_provider as bundled_push_config;

pub use transport::TlsPushTransportFactory;
#[cfg(test)]
pub use transport::{PushTransport, PushTransportFactory};

#[derive(Debug, Clone)]
pub struct PushError {
    pub code: &'static str,
    pub message: String,
}

impl PushError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageType {
    ProductChange,
    AssetChange,
    SessionChange,
    LayoutChange,
    GswsSync,
    SectionChange,
    LinkedAccountChange,
    AppChange,
    ServerInfoSync,
    ConfigurationChange,
    SubscriptionChange,
    PatchingEvent,
    KvStoreChange,
    LibraryChange,
    PlatformSyncChange,
    CampaignChange,
    FavoritesChange,
}

impl MessageType {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "PRODUCT_CHANGE" => Self::ProductChange,
            "ASSET_CHANGE" => Self::AssetChange,
            "SESSION_CHANGE" => Self::SessionChange,
            "LAYOUT_CHANGE" => Self::LayoutChange,
            "GSWS_SYNC" => Self::GswsSync,
            "SECTION_CHANGE" => Self::SectionChange,
            "LINKEDACCOUNT_CHANGE" => Self::LinkedAccountChange,
            "APP_CHANGE" => Self::AppChange,
            "SERVER_INFO_SYNC" => Self::ServerInfoSync,
            "CONFIGURATION_CHANGE" => Self::ConfigurationChange,
            "SUBSCRIPTION_CHANGE" => Self::SubscriptionChange,
            "PATCHING_EVENT" => Self::PatchingEvent,
            "KV_STORE_CHANGE" => Self::KvStoreChange,
            "LIBRARY_CHANGE" => Self::LibraryChange,
            "PLATFORM_SYNC_CHANGE" => Self::PlatformSyncChange,
            "CAMPAIGN_CHANGE" => Self::CampaignChange,
            "FAVORITES_CHANGE" => Self::FavoritesChange,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushMessage {
    pub message_type: Option<MessageType>,
    pub region_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub change_id_type: Option<String>,
    pub platform_code: Option<String>,
    pub sync_state: Option<String>,
    pub sync_date: Option<String>,
    pub sync_game_count: Option<u64>,
    pub account_type: Option<String>,
    pub account_linked: Option<bool>,
}

impl PushMessage {
    pub fn parse(data: &serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        let mut message = PushMessage::default();
        let mut saw_type = false;
        for (key, value) in data {
            match key.as_str() {
                "messageType" => {
                    let text = json_string(value)?;
                    message.message_type = MessageType::parse(&text);
                    message.message_type?;
                    saw_type = true;
                }
                "changedIds" => message.changed_ids = json_string_list(value),
                "changeIdType" => message.change_id_type = json_string(value),
                "regionIds" => message.region_ids = json_string_list(value),
                "platformSyncInfo" => {
                    let text = json_string(value)?;
                    let info: serde_json::Value = serde_json::from_str(&text).ok()?;
                    message.platform_code = info["platformCode"].as_str().map(str::to_owned);
                    message.sync_state = info["syncState"].as_str().map(str::to_owned);
                    message.sync_date = info["syncDate"].as_str().map(str::to_owned);
                    message.sync_game_count = info["syncGameCount"].as_u64();
                }
                "accountLinkedInfo" => {
                    let text = json_string(value)?;
                    let info: serde_json::Value = serde_json::from_str(&text).ok()?;
                    message.account_type = info["accountType"].as_str().map(str::to_owned);
                    message.account_linked = info["linked"].as_bool();
                }
                _ => {}
            }
        }
        saw_type.then_some(message)
    }
}

fn json_string(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        if let Ok(inner) = serde_json::from_str::<String>(text) {
            return Some(inner);
        }
        return Some(text.to_owned());
    }
    Some(value.to_string())
}

fn json_string_list(value: &serde_json::Value) -> Vec<String> {
    let text = json_string(value).unwrap_or_default();
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default()
}

pub const MAXIMUM_FRAME_BYTES: usize = 64 * 1024;
pub const MAXIMUM_CHANGED_IDS: usize = 64;

#[cfg(test)]
mod tests;
