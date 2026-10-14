use crate::requests::Cancellation;
pub use opennow_plugin_api::{
    CatalogItem, CatalogPage, CatalogQuery, Coverage, PluginDescriptor, PluginId, PluginSnapshot,
    PluginState, PluginTrust, SourceCatalogPage,
};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceError {
    pub code: String,
    pub message: String,
}

impl SourceError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        let mut code = code.into();
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            code = "source_error".to_owned();
        }
        let mut message = message.into();
        if message.len() > 1024 {
            let mut end = 1024;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }
        Self { code, message }
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "Request cancelled")
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

impl From<SourceError> for (String, String) {
    fn from(error: SourceError) -> Self {
        (error.code, error.message)
    }
}

impl From<crate::service_error::ServiceError> for SourceError {
    fn from(error: crate::service_error::ServiceError) -> Self {
        Self::new(error.code, error.message)
    }
}

pub trait CatalogSource: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;
    fn generation(&self) -> u64;
    fn catalog_page(
        &self,
        query: &CatalogQuery,
        cancellation: &Cancellation,
    ) -> Result<CatalogPage, SourceError>;
}

pub struct ProviderContext<'a> {
    pub cancellation: &'a Cancellation,
    pub runtime_capabilities: Option<&'a Value>,
    pub gfn_settings: Option<&'a Value>,
}

pub enum NativePreparation {
    Gfn(Value),
    External(opennow_plugin_api::media::PreparedWorker),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllocationDisposition {
    NotDispatched,
    MayHaveAllocated,
    Rejected,
    Allocated { session_id: String },
    CleanedUp { session_id: String },
}

pub struct ProviderCompletion {
    pub result: Result<opennow_plugin_api::provider::ProviderReply, SourceError>,
    pub effects: Vec<opennow_plugin_api::provider::ProviderEffect>,
    pub allocation: Option<opennow_plugin_api::provider::AllocationTicket>,
    pub dispatched: bool,
    pub allocation_disposition: Option<AllocationDisposition>,
    pub dispatched_generation: Option<u64>,
}

pub struct ProviderNotification {
    pub request: opennow_plugin_api::provider::ProviderRequest,
    pub response: opennow_plugin_api::provider::ProviderResponseV2,
}

impl ProviderCompletion {
    #[cfg(test)]
    pub fn reply(reply: opennow_plugin_api::provider::ProviderReply) -> Self {
        Self {
            result: Ok(reply),
            effects: Vec::new(),
            allocation: None,
            dispatched: true,
            allocation_disposition: None,
            dispatched_generation: None,
        }
    }

    #[cfg(test)]
    pub fn failed(error: SourceError) -> Self {
        Self {
            result: Err(error),
            effects: Vec::new(),
            allocation: None,
            dispatched: true,
            allocation_disposition: None,
            dispatched_generation: None,
        }
    }

    pub fn not_dispatched(error: SourceError) -> Self {
        Self {
            result: Err(error),
            effects: Vec::new(),
            allocation: None,
            dispatched: false,
            allocation_disposition: None,
            dispatched_generation: None,
        }
    }
}

pub trait ProviderSource: CatalogSource {
    fn provider_capabilities(&self) -> Vec<opennow_plugin_api::provider::Capability>;
    fn auth_kinds(&self) -> Vec<opennow_plugin_api::provider::AuthKind>;
    fn provider_call(
        &self,
        request: &opennow_plugin_api::provider::ProviderRequest,
        context: &ProviderContext<'_>,
    ) -> ProviderCompletion;
    fn prepare_native(
        &self,
        request: &opennow_plugin_api::provider::PrepareSession,
        context: &ProviderContext<'_>,
    ) -> Result<NativePreparation, SourceError>;
    fn take_notifications(&self) -> Vec<ProviderNotification> {
        Vec::new()
    }
}

pub type CoreEvent = (&'static str, Value);
pub type LegacyResult = Result<(Value, Option<CoreEvent>), (String, String)>;

pub struct Completion {
    pub result: LegacyResult,
    pub required_events: Vec<CoreEvent>,
    pub receipt: Option<Box<dyn AllocationReceipt>>,
    pub reporting: Vec<ReportingEffect>,
    pub allocation_disposition: Option<AllocationDisposition>,
}

pub enum ReportingEffect {
    LaunchRequested {
        params: Value,
    },
    SessionStopped,
    RuntimeObserved {
        capabilities: Value,
    },
    SignedIn {
        restored: bool,
    },
    SignedOut,
    RpcFailure {
        method: String,
        code: String,
        message: String,
    },
}

impl From<LegacyResult> for Completion {
    fn from(result: LegacyResult) -> Self {
        Self {
            result,
            required_events: Vec::new(),
            receipt: None,
            reporting: Vec::new(),
            allocation_disposition: None,
        }
    }
}

pub struct ReceiptOutcome {
    pub result: Result<(), SourceError>,
    pub required_events: Vec<CoreEvent>,
}

pub trait AllocationReceipt: Send {
    fn settle(self: Box<Self>, accepted: bool) -> ReceiptOutcome;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionOccupancy {
    Idle,
    InUse,
    Unknown,
}

pub trait BuiltinModule: ProviderSource {
    fn routes(&self) -> &'static [&'static str];
    fn core_capabilities(&self) -> &'static [&'static str];
    fn dispatch(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion>;
    fn reporting_identity(&self) -> Value;
    fn session_occupancy(&self) -> SessionOccupancy;
    fn settings_changed(&self);
    fn shutdown(&self);
    fn set_enabled(&self, enabled: bool);
    fn legacy_account(
        &self,
        params: &Value,
    ) -> Result<opennow_plugin_api::provider::AccountKey, SourceError>;
    fn legacy_session_result(
        &self,
        method: &str,
        params: &Value,
        value: &Value,
    ) -> Result<Option<(opennow_plugin_api::provider::SessionKey, bool)>, SourceError>;
    fn apply_profile(
        &self,
        base: &Value,
        profile: &opennow_plugin_api::provider::StreamPreferences,
    ) -> Value;
    fn legacy_control_session(
        &self,
        params: &Value,
    ) -> Result<Option<opennow_plugin_api::provider::SessionKey>, SourceError>;
}
