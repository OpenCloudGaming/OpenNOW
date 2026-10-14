use super::*;
use crate::{CatalogQuery, Coverage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "auth.anonymous.v2")]
    AuthAnonymous,
    #[serde(rename = "auth.deviceCode.v2")]
    AuthDeviceCode,
    #[serde(rename = "auth.browser.v2")]
    AuthBrowser,
    #[serde(rename = "auth.pairing.v2")]
    AuthPairing,
    #[serde(rename = "accounts.v2")]
    Accounts,
    #[serde(rename = "accounts.pin.v2")]
    AccountPin,
    #[serde(rename = "catalog.public.v2")]
    PublicCatalog,
    #[serde(rename = "catalog.library.v2")]
    LibraryCatalog,
    #[serde(rename = "catalog.store.v2")]
    StoreCatalog,
    #[serde(rename = "catalog.details.v2")]
    CatalogDetails,
    #[serde(rename = "catalog.favorites.v2")]
    Favorites,
    #[serde(rename = "catalog.ownership.v2")]
    Ownership,
    #[serde(rename = "catalog.definitions.v2")]
    CatalogDefinitions,
    #[serde(rename = "launch.v2")]
    Launch,
    #[serde(rename = "settings.v2")]
    Settings,
    #[serde(rename = "sessions.v2")]
    Sessions,
    #[serde(rename = "media.worker.v1")]
    MediaWorker,
    #[serde(rename = "account.subscription.v2")]
    Subscription,
    #[serde(rename = "account.connections.v2")]
    Connections,
    #[serde(rename = "locations.v2")]
    Locations,
    #[serde(rename = "account.storage.v2")]
    Storage,
    #[serde(rename = "session.ads.v2")]
    SessionAds,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountKey {
    pub authority: AuthorityId,
    pub account: AccountId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountScope {
    pub account: AccountKey,
    #[serde(
        deserialize_with = "super::bounds::deserialize_public_revision",
        serialize_with = "super::bounds::serialize_public_revision"
    )]
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKey {
    pub account: Option<AccountKey>,
    pub remote_id: SessionId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Authority {
    pub id: AuthorityId,
    pub name: Text<128>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    Anonymous,
    DeviceCode,
    Browser,
    Pairing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Persistence {
    Temporary,
    Durable,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicAccount {
    pub key: AccountKey,
    pub name: Text<256>,
    pub persistence: Persistence,
    pub reauthentication_required: bool,
    pub pin_locked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AuthChallenge {
    DeviceCode {
        attempt: AttemptId,
        #[serde(rename = "userCode")]
        user_code: SecretString,
        #[serde(rename = "verificationUri")]
        verification_uri: PublicUrl,
        #[serde(rename = "expiresAtMs")]
        expires_at_ms: u64,
        #[serde(rename = "pollAfterMs")]
        poll_after_ms: u32,
    },
    Browser {
        attempt: AttemptId,
        authorization: SecretString,
        #[serde(rename = "expiresAtMs")]
        expires_at_ms: u64,
        #[serde(rename = "pollAfterMs")]
        poll_after_ms: u32,
    },
    Pairing {
        attempt: AttemptId,
        code: Option<SecretString>,
        #[serde(rename = "expiresAtMs")]
        expires_at_ms: u64,
        #[serde(rename = "pollAfterMs")]
        poll_after_ms: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AuthState {
    NotRequired,
    SignedOut,
    Pending {
        challenge: AuthChallenge,
    },
    Authorized {
        attempt: AttemptId,
    },
    SignedIn {
        account: PublicAccount,
        #[serde(
            deserialize_with = "super::bounds::deserialize_public_revision",
            serialize_with = "super::bounds::serialize_public_revision"
        )]
        revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginAuth {
    pub authority: Option<AuthorityId>,
    pub kind: AuthKind,
    pub remember: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthAttempt {
    pub attempt: AttemptId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteAuth {
    pub attempt: AttemptId,
    pub proof: Option<SecretString>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectAccount {
    pub account: AccountKey,
    pub pin: Option<SecretString>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Accounts {
    pub accounts: List<PublicAccount, 64>,
    pub selected: Option<AccountKey>,
    #[serde(
        deserialize_with = "super::bounds::deserialize_public_revision",
        serialize_with = "super::bounds::serialize_public_revision"
    )]
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CatalogScope {
    Public,
    Account { scope: AccountScope },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogRequest {
    pub scope: CatalogScope,
    pub query: CatalogQuery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameSummary {
    pub id: GameId,
    pub title: Text<256>,
    pub artwork: Option<PublicUrl>,
    pub subtitle: Option<Text<256>>,
    pub badges: List<Text<64>, 8>,
    pub availability: Availability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Available,
    Maintenance,
    Patching,
    SubscriptionRequired,
    OwnershipRequired,
    AccountLinkRequired,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GamePage {
    pub items: List<GameSummary, 100>,
    pub next_cursor: Option<Text<4096>>,
    pub coverage: Coverage,
    pub revision: Text<256>,
    pub scope: CatalogScope,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRequest {
    pub scope: CatalogScope,
    pub game: GameId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameVariant {
    pub id: VariantId,
    pub label: Text<128>,
    pub availability: Availability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameDetails {
    pub game: GameSummary,
    pub description: Option<Text<8192>>,
    pub variants: List<GameVariant, 64>,
    pub revision: Text<256>,
    pub scope: CatalogScope,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchTarget {
    pub game: GameId,
    pub variant: VariantId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectLaunch {
    pub scope: Option<AccountScope>,
    pub target: LaunchTarget,
    pub catalog_revision: Text<256>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LaunchDecision {
    Ready {
        target: LaunchTarget,
        revision: Text<256>,
    },
    Blocked {
        reason: Availability,
        message: Text<512>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RemoteSessionState {
    Allocating,
    Queued {
        position: Option<u32>,
        #[serde(rename = "waitSeconds")]
        wait_seconds: Option<u32>,
    },
    Ready,
    Suspended,
    Finished {
        reason: TerminalReason,
    },
    Failed {
        error: ProviderErrorCode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    UserStopped,
    RemoteEnded,
    Expired,
    AllocationRejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionView {
    pub key: SessionKey,
    pub target: LaunchTarget,
    pub state: RemoteSessionState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReply {
    pub session: SessionView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllocationTicket {
    pub operation: OperationId,
    pub receipt: ReceiptId,
    pub session: SessionKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Acceptance {
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveAllocation {
    pub operation: OperationId,
    pub receipt: ReceiptId,
    pub decision: Acceptance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopSession {
    pub session: SessionKey,
    pub operation: OperationId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CleanupState {
    Resolved,
    Pending { operation: OperationId },
    Unknown { operation: OperationId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileSession {
    pub scope: Option<AccountScope>,
    pub operation: OperationId,
    pub session: Option<SessionKey>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Reconciliation {
    PendingAllocation {
        session: SessionView,
        ticket: AllocationTicket,
    },
    Active {
        session: SessionView,
    },
    Terminal {
        session: SessionKey,
        reason: TerminalReason,
    },
    Unknown {
        operation: OperationId,
    },
    NotAllocated {
        operation: OperationId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorCode {
    InvalidRequest,
    UnsupportedFeature,
    ScopeChanged,
    AuthRequired,
    AuthenticationFailed,
    Cancelled,
    BusyBeforeDispatch,
    OutcomeUnknown,
    RateLimited,
    ServiceUnavailable,
    SessionNotFound,
    SessionNotReady,
    CleanupRequired,
    OwnershipRequired,
    SubscriptionRequired,
    InternalError,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderError {
    pub code: ProviderErrorCode,
    pub retry_after_ms: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProviderEffect {
    AuthChanged {
        #[serde(
            deserialize_with = "super::bounds::deserialize_public_revision",
            serialize_with = "super::bounds::serialize_public_revision"
        )]
        revision: u64,
    },
    CatalogInvalidated {
        scope: CatalogScope,
        revision: Text<256>,
    },
    SessionChanged {
        session: SessionView,
    },
    CleanupRequired {
        operation: OperationId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum SettingValue {
    Boolean(bool),
    Integer(i64),
    Number(FiniteNumber),
    Choice(Text<128>),
    Text(SettingText),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SettingControl {
    Boolean,
    Integer {
        min: i64,
        max: i64,
        step: u32,
    },
    Number {
        min: FiniteNumber,
        max: FiniteNumber,
        step: FiniteNumber,
    },
    Choice {
        choices: List<SettingChoice, 64>,
    },
    Text {
        #[serde(rename = "maximumBytes")]
        maximum_bytes: u16,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingChoice {
    pub value: Text<128>,
    pub label: Text<128>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDefinition {
    pub key: SettingKey,
    pub label: Text<128>,
    pub control: SettingControl,
    pub value: SettingValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsView {
    #[serde(
        deserialize_with = "super::bounds::deserialize_public_revision",
        serialize_with = "super::bounds::serialize_public_revision"
    )]
    pub revision: u64,
    pub settings: List<SettingDefinition, 64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsScope {
    pub account: Option<AccountScope>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetSetting {
    pub scope: SettingsScope,
    #[serde(
        deserialize_with = "super::bounds::deserialize_public_revision",
        serialize_with = "super::bounds::serialize_public_revision"
    )]
    pub expected_revision: u64,
    pub key: SettingKey,
    pub value: SettingValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamPreferences {
    pub video: crate::media::RequestedVideo,
    pub bitrate_kbps: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSession {
    pub scope: Option<AccountScope>,
    pub operation: OperationId,
    pub target: LaunchTarget,
    pub catalog_revision: Text<256>,
    #[serde(
        deserialize_with = "super::bounds::deserialize_public_revision",
        serialize_with = "super::bounds::serialize_public_revision"
    )]
    pub settings_revision: u64,
    pub preferences: StreamPreferences,
    pub offer: crate::media::NativeOffer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareSession {
    pub session: SessionKey,
    pub offer: crate::media::NativeOffer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverSessions {
    pub scope: Option<AccountScope>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimSession {
    pub scope: Option<AccountScope>,
    pub session: SessionKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHello {
    pub plugin_id: crate::PluginId,
    pub version: Text<64>,
    pub capabilities: List<Capability, 32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHelloReply {
    pub plugin_id: crate::PluginId,
    pub version: Text<64>,
    pub protocol_version: u32,
    pub capabilities: List<Capability, 32>,
    pub auth_kinds: List<AuthKind, 4>,
}
