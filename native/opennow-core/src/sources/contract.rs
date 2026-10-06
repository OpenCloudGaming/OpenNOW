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

pub type CoreEvent = (&'static str, Value);
pub type LegacyResult = Result<(Value, Option<CoreEvent>), (String, String)>;

pub struct Completion {
    pub result: LegacyResult,
    pub required_events: Vec<CoreEvent>,
    pub receipt: Option<Box<dyn AllocationReceipt>>,
    pub reporting: Vec<ReportingEffect>,
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

pub trait BuiltinModule: CatalogSource {
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
}
