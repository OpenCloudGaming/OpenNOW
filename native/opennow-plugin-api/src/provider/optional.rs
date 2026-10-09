use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinRequest {
    pub account: AccountKey,
    pub pin: SecretString,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinStatus {
    pub enabled: bool,
    pub unlocked: bool,
    pub retry_after_ms: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FavoriteMutation {
    pub scope: AccountScope,
    pub game: GameId,
    pub favorite: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnershipMutation {
    pub scope: AccountScope,
    pub target: LaunchTarget,
    pub action: OwnershipAction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OwnershipAction {
    Add {
        #[serde(rename = "confirmedExistingLicense")]
        confirmed_existing_license: bool,
    },
    Remove,
    Select,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MutationState {
    Confirmed,
    Unconfirmed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDefinition {
    pub id: Text<128>,
    pub name: Text<128>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDefinitions {
    pub stores: List<CatalogDefinition, 128>,
    pub genres: List<CatalogDefinition, 256>,
    pub languages: List<CatalogDefinition, 512>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub label: Text<128>,
    pub features: List<Text<128>, 32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountConnection {
    pub id: ConnectionId,
    pub label: Text<128>,
    pub state: ConnectionState,
    pub can_sync: bool,
    pub can_unlink: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionState {
    Linked,
    NotLinked,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionRequest {
    pub scope: AccountScope,
    pub connection: ConnectionId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkPoll {
    pub scope: AccountScope,
    pub attempt: AttemptId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncObservation {
    pub scope: AccountScope,
    pub observation: ObservationId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SyncState {
    Pending { observation: ObservationId },
    Confirmed,
    Unconfirmed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: RegionId,
    pub name: Text<128>,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectRegion {
    pub scope: AccountScope,
    pub region: RegionId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageLocation {
    pub id: StorageId,
    pub name: Text<128>,
    pub can_reset: bool,
    pub used_bytes: Option<u64>,
    pub capacity_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetStorage {
    pub scope: AccountScope,
    pub storage: StorageId,
    pub confirmed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdEvent {
    Started,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportAd {
    pub session: SessionKey,
    pub ad: AdId,
    pub event: AdEvent,
}
