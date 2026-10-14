use super::bounds::validate_public_revision;
use super::*;
use crate::ValidationError;
use crate::media::PreparedWorker;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

macro_rules! operations {
    ($( $variant:ident => ($wire:literal, $request:ty, $reply:ty, $capability:expr) ),+ $(,)?) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "method", content = "params", deny_unknown_fields)]
        pub enum ProviderRequest { $( #[serde(rename = $wire)] $variant($request), )+ }

        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "method", content = "result", deny_unknown_fields)]
        pub enum ProviderReply { $( #[serde(rename = $wire)] $variant($reply), )+ }

        impl ProviderRequest {
            pub fn method(&self) -> &'static str {
                match self { $(Self::$variant(_) => $wire,)+ }
            }

            pub fn required_capability(&self) -> Option<Capability> {
                match self { $(Self::$variant(_) => $capability,)+ }
            }

            pub fn matches_reply(&self, reply: &ProviderReply) -> bool {
                matches!((self, reply), $((Self::$variant(_), ProviderReply::$variant(_)))|+)
            }
        }
    };
}

operations! {
    Hello => ("provider.hello", ProviderHello, ProviderHelloReply, None),
    Shutdown => ("provider.shutdown", Empty, Empty, None),
    AuthAuthorities => ("auth.authorities", Empty, List<Authority, 128>, Some(Capability::Accounts)),
    AuthStatus => ("auth.status", Empty, AuthState, None),
    AuthBegin => ("auth.begin", BeginAuth, AuthState, None),
    AuthPoll => ("auth.poll", AuthAttempt, AuthState, None),
    AuthComplete => ("auth.complete", CompleteAuth, AuthState, None),
    AuthCancel => ("auth.cancel", AuthAttempt, Empty, None),
    AuthLogout => ("auth.logout", AccountKey, AuthState, Some(Capability::Accounts)),
    AccountsList => ("accounts.list", Empty, Accounts, Some(Capability::Accounts)),
    AccountsSelect => ("accounts.select", SelectAccount, AuthState, Some(Capability::Accounts)),
    AccountsRemove => ("accounts.remove", AccountKey, Accounts, Some(Capability::Accounts)),
    PinStatus => ("accounts.pin.status", AccountKey, PinStatus, Some(Capability::AccountPin)),
    PinSet => ("accounts.pin.set", PinRequest, PinStatus, Some(Capability::AccountPin)),
    PinVerify => ("accounts.pin.verify", PinRequest, PinStatus, Some(Capability::AccountPin)),
    PinClear => ("accounts.pin.clear", PinRequest, PinStatus, Some(Capability::AccountPin)),
    CatalogPublic => ("catalog.public", CatalogRequest, GamePage, Some(Capability::PublicCatalog)),
    CatalogLibrary => ("catalog.library", CatalogRequest, GamePage, Some(Capability::LibraryCatalog)),
    CatalogStore => ("catalog.store", CatalogRequest, GamePage, Some(Capability::StoreCatalog)),
    CatalogDetails => ("catalog.details", GameRequest, GameDetails, Some(Capability::CatalogDetails)),
    CatalogDefinitions => ("catalog.definitions", Empty, CatalogDefinitions, Some(Capability::CatalogDefinitions)),
    FavoritesList => ("catalog.favorites", CatalogRequest, GamePage, Some(Capability::Favorites)),
    FavoritesSet => ("catalog.favorites.set", FavoriteMutation, MutationState, Some(Capability::Favorites)),
    OwnershipSet => ("catalog.ownership.set", OwnershipMutation, MutationState, Some(Capability::Ownership)),
    LaunchInspect => ("launch.inspect", InspectLaunch, LaunchDecision, Some(Capability::Launch)),
    SettingsGet => ("settings.get", SettingsScope, SettingsView, Some(Capability::Settings)),
    SettingsSet => ("settings.set", SetSetting, SettingsView, Some(Capability::Settings)),
    SessionCreate => ("session.create", CreateSession, CreateReply, Some(Capability::Sessions)),
    SessionPoll => ("session.poll", SessionKey, SessionView, Some(Capability::Sessions)),
    SessionDiscover => ("session.discover", DiscoverSessions, List<SessionView, 32>, Some(Capability::Sessions)),
    SessionClaim => ("session.claim", ClaimSession, SessionView, Some(Capability::Sessions)),
    SessionReconcile => ("session.reconcile", ReconcileSession, Reconciliation, Some(Capability::Sessions)),
    SessionPrepare => ("session.prepare", PrepareSession, PreparedWorker, Some(Capability::Sessions)),
    SessionStop => ("session.stop", StopSession, CleanupState, Some(Capability::Sessions)),
    SessionResolveAllocation => ("session.resolve-allocation", ResolveAllocation, CleanupState, Some(Capability::Sessions)),
    SubscriptionGet => ("account.subscription", AccountScope, Subscription, Some(Capability::Subscription)),
    ConnectionsList => ("account.connections", AccountScope, List<AccountConnection, 32>, Some(Capability::Connections)),
    ConnectionLink => ("account.connection.link", ConnectionRequest, AuthChallenge, Some(Capability::Connections)),
    ConnectionLinkPoll => ("account.connection.link.poll", LinkPoll, AccountConnection, Some(Capability::Connections)),
    ConnectionUnlink => ("account.connection.unlink", ConnectionRequest, MutationState, Some(Capability::Connections)),
    ConnectionSync => ("account.connection.sync", ConnectionRequest, SyncState, Some(Capability::Connections)),
    ConnectionSyncObserve => ("account.connection.sync.observe", SyncObservation, SyncState, Some(Capability::Connections)),
    ConnectionSyncCancel => ("account.connection.sync.cancel", SyncObservation, Empty, Some(Capability::Connections)),
    RegionsList => ("locations.list", AccountScope, List<Region, 128>, Some(Capability::Locations)),
    RegionSelect => ("locations.select", SelectRegion, List<Region, 128>, Some(Capability::Locations)),
    StorageList => ("account.storage", AccountScope, List<StorageLocation, 128>, Some(Capability::Storage)),
    StorageReset => ("account.storage.reset", ResetStorage, MutationState, Some(Capability::Storage)),
    SessionAdReport => ("session.ad.report", ReportAd, SessionView, Some(Capability::SessionAds)),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version2;

impl Serialize for Version2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(PROVIDER_PROTOCOL_VERSION)
    }
}

impl<'de> Deserialize<'de> for Version2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if u32::deserialize(deserializer)? != PROVIDER_PROTOCOL_VERSION {
            return Err(serde::de::Error::custom(
                "Provider control protocol must be version 2",
            ));
        }
        Ok(Self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostRequestV2 {
    pub v: Version2,
    pub epoch: NonZeroU64,
    pub id: Text<64>,
    pub timeout_ms: u32,
    pub request: ProviderRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum HostMessageV2 {
    Request(Box<HostRequestV2>),
    Cancel {
        v: Version2,
        epoch: NonZeroU64,
        id: Text<64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
pub enum ProviderOutcome {
    Success { reply: Box<ProviderReply> },
    Failure { error: ProviderError },
}

impl ProviderOutcome {
    pub fn success(reply: ProviderReply) -> Self {
        Self::Success {
            reply: Box::new(reply),
        }
    }

    pub fn reply(&self) -> Option<&ProviderReply> {
        match self {
            Self::Success { reply } => Some(reply.as_ref()),
            Self::Failure { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderResponseV2 {
    pub v: Version2,
    pub epoch: NonZeroU64,
    pub id: Text<64>,
    pub outcome: ProviderOutcome,
    pub effects: List<ProviderEffect, 32>,
    pub allocation: Option<AllocationTicket>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PluginMessageV2 {
    Response(ProviderResponseV2),
}

impl HostMessageV2 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ValidationError> {
        if bytes.len() > crate::MAX_FRAME_BYTES {
            return Err(ValidationError("Provider control frame is too large"));
        }
        let message: Self = serde_json::from_slice(bytes)
            .map_err(|_| ValidationError("Provider request is malformed"))?;
        if let Self::Request(request) = &message {
            request.validate()?;
        }
        Ok(message)
    }
}

impl HostRequestV2 {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=120_000).contains(&self.timeout_ms) {
            return Err(ValidationError("Provider deadline is out of bounds"));
        }
        self.request.validate()
    }
}

impl PluginMessageV2 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ValidationError> {
        if bytes.len() > crate::MAX_FRAME_BYTES {
            return Err(ValidationError("Provider control frame is too large"));
        }
        let message: Self = serde_json::from_slice(bytes)
            .map_err(|_| ValidationError("Provider response is malformed"))?;
        let Self::Response(response) = &message;
        response.validate()?;
        Ok(message)
    }
}

impl ProviderRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let scope = match self {
            Self::CatalogPublic(request)
            | Self::CatalogLibrary(request)
            | Self::CatalogStore(request)
            | Self::FavoritesList(request) => request.scope.account_scope(),
            Self::CatalogDetails(request) => request.scope.account_scope(),
            Self::FavoritesSet(request) => Some(&request.scope),
            Self::OwnershipSet(request) => Some(&request.scope),
            Self::LaunchInspect(request) => request.scope.as_ref(),
            Self::SettingsGet(request) => request.account.as_ref(),
            Self::SettingsSet(request) => {
                validate_public_revision(request.expected_revision)?;
                request.scope.account.as_ref()
            }
            Self::SessionCreate(request) => {
                validate_public_revision(request.settings_revision)?;
                request.scope.as_ref()
            }
            Self::SessionDiscover(request) => request.scope.as_ref(),
            Self::SessionClaim(request) => request.scope.as_ref(),
            Self::SessionReconcile(request) => request.scope.as_ref(),
            Self::SubscriptionGet(scope)
            | Self::ConnectionsList(scope)
            | Self::RegionsList(scope)
            | Self::StorageList(scope) => Some(scope),
            Self::ConnectionLink(request)
            | Self::ConnectionUnlink(request)
            | Self::ConnectionSync(request) => Some(&request.scope),
            Self::ConnectionLinkPoll(request) => Some(&request.scope),
            Self::ConnectionSyncObserve(request) | Self::ConnectionSyncCancel(request) => {
                Some(&request.scope)
            }
            Self::RegionSelect(request) => Some(&request.scope),
            Self::StorageReset(request) => Some(&request.scope),
            Self::Hello(_)
            | Self::Shutdown(_)
            | Self::AuthAuthorities(_)
            | Self::AuthStatus(_)
            | Self::AuthBegin(_)
            | Self::AuthPoll(_)
            | Self::AuthComplete(_)
            | Self::AuthCancel(_)
            | Self::AuthLogout(_)
            | Self::AccountsList(_)
            | Self::AccountsSelect(_)
            | Self::AccountsRemove(_)
            | Self::PinStatus(_)
            | Self::PinSet(_)
            | Self::PinVerify(_)
            | Self::PinClear(_)
            | Self::CatalogDefinitions(_)
            | Self::SessionPoll(_)
            | Self::SessionPrepare(_)
            | Self::SessionStop(_)
            | Self::SessionResolveAllocation(_)
            | Self::SessionAdReport(_) => None,
        };
        if let Some(scope) = scope {
            validate_public_revision(scope.revision)?;
        }
        match self {
            Self::SessionCreate(create) => {
                create.offer.validate()?;
                create.preferences.video.validate()?;
                if !(1..=200_000).contains(&create.preferences.bitrate_kbps) {
                    return Err(ValidationError("Requested bitrate exceeds its bounds"));
                }
            }
            Self::SessionPrepare(prepare) => prepare.offer.validate()?,
            Self::CatalogPublic(request)
            | Self::CatalogLibrary(request)
            | Self::CatalogStore(request)
            | Self::FavoritesList(request) => request.query.validate()?,
            Self::OwnershipSet(OwnershipMutation {
                action:
                    OwnershipAction::Add {
                        confirmed_existing_license: false,
                    },
                ..
            }) => {
                return Err(ValidationError(
                    "Ownership mutation requires explicit confirmation",
                ));
            }
            Self::StorageReset(ResetStorage {
                confirmed: false, ..
            }) => {
                return Err(ValidationError(
                    "Storage reset requires explicit confirmation",
                ));
            }
            Self::Hello(hello) if semver::Version::parse(hello.version.as_str()).is_err() => {
                return Err(ValidationError("Provider hello version is invalid"));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn permits(&self, capabilities: &[Capability]) -> bool {
        if self
            .required_capability()
            .is_some_and(|capability| !capabilities.contains(&capability))
        {
            return false;
        }
        match self {
            Self::AuthBegin(begin) => capabilities.contains(&match begin.kind {
                AuthKind::Anonymous => Capability::AuthAnonymous,
                AuthKind::DeviceCode => Capability::AuthDeviceCode,
                AuthKind::Browser => Capability::AuthBrowser,
                AuthKind::Pairing => Capability::AuthPairing,
            }),
            Self::AuthPoll(_) | Self::AuthComplete(_) | Self::AuthCancel(_) => {
                capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        Capability::AuthDeviceCode
                            | Capability::AuthBrowser
                            | Capability::AuthPairing
                    )
                })
            }
            _ => true,
        }
    }
}

impl ProviderResponseV2 {
    pub fn validate(&self) -> Result<(), ValidationError> {
        for effect in self.effects.iter() {
            match effect {
                ProviderEffect::AuthChanged { revision } => validate_public_revision(*revision)?,
                ProviderEffect::CatalogInvalidated { scope, .. } => scope.validate_revision()?,
                ProviderEffect::SessionChanged { .. } | ProviderEffect::CleanupRequired { .. } => {}
            }
        }
        if let ProviderOutcome::Failure { error } = &self.outcome {
            if error.retry_after_ms.is_some_and(|delay| delay > 3_600_000) {
                return Err(ValidationError("Provider retry delay is too large"));
            }
        }
        if let ProviderOutcome::Success { reply } = &self.outcome {
            reply.validate()?;
        }
        Ok(())
    }

    pub fn validate_for(&self, request: &HostRequestV2) -> Result<(), ValidationError> {
        self.validate()?;
        if self.epoch != request.epoch || self.id != request.id {
            return Err(ValidationError(
                "Provider response does not match the request incarnation",
            ));
        }
        if let ProviderOutcome::Success { reply } = &self.outcome {
            if !request.request.matches_reply(reply) {
                return Err(ValidationError(
                    "Provider response method does not match its request",
                ));
            }
            match (&request.request, reply.as_ref()) {
                (ProviderRequest::Hello(expected), ProviderReply::Hello(actual)) => {
                    if expected.plugin_id != actual.plugin_id
                        || expected.version != actual.version
                        || actual
                            .capabilities
                            .iter()
                            .any(|capability| !expected.capabilities.contains(capability))
                    {
                        return Err(ValidationError(
                            "Provider greeting differs from the installed identity or capabilities",
                        ));
                    }
                }
                (ProviderRequest::CatalogPublic(query), ProviderReply::CatalogPublic(page))
                | (ProviderRequest::CatalogLibrary(query), ProviderReply::CatalogLibrary(page))
                | (ProviderRequest::CatalogStore(query), ProviderReply::CatalogStore(page))
                | (ProviderRequest::FavoritesList(query), ProviderReply::FavoritesList(page)) => {
                    if page.scope != query.scope
                        || page.items.len() > usize::from(query.query.limit)
                    {
                        return Err(ValidationError(
                            "Catalog reply differs from the requested scope or page limit",
                        ));
                    }
                }
                (
                    ProviderRequest::CatalogDetails(query),
                    ProviderReply::CatalogDetails(details),
                ) => {
                    if details.scope != query.scope || details.game.id != query.game {
                        return Err(ValidationError(
                            "Game details differ from the requested scope or game",
                        ));
                    }
                }
                (
                    ProviderRequest::LaunchInspect(query),
                    ProviderReply::LaunchInspect(LaunchDecision::Ready { target, .. }),
                ) => {
                    if target != &query.target {
                        return Err(ValidationError("Launch decision names a different target"));
                    }
                }
                (ProviderRequest::SessionPoll(key), ProviderReply::SessionPoll(session)) => {
                    if key != &session.key {
                        return Err(ValidationError(
                            "Session poll changed its owner or remote identity",
                        ));
                    }
                }
                (ProviderRequest::SessionCreate(create), ProviderReply::SessionCreate(reply)) => {
                    if reply.session.target != create.target {
                        return Err(ValidationError(
                            "Allocated session names a different launch target",
                        ));
                    }
                }
                (
                    ProviderRequest::SessionStop(stop),
                    ProviderReply::SessionStop(
                        CleanupState::Pending { operation } | CleanupState::Unknown { operation },
                    ),
                ) => {
                    if operation != &stop.operation {
                        return Err(ValidationError(
                            "Cleanup changed the original stop operation",
                        ));
                    }
                }
                (
                    ProviderRequest::SessionResolveAllocation(resolve),
                    ProviderReply::SessionResolveAllocation(
                        CleanupState::Pending { operation } | CleanupState::Unknown { operation },
                    ),
                ) => {
                    if operation != &resolve.operation {
                        return Err(ValidationError(
                            "Cleanup changed the original allocation operation",
                        ));
                    }
                }
                (ProviderRequest::SessionClaim(claim), ProviderReply::SessionClaim(session)) => {
                    if claim.session != session.key {
                        return Err(ValidationError(
                            "Session claim changed its owner or remote identity",
                        ));
                    }
                }
                (
                    ProviderRequest::SessionDiscover(discovery),
                    ProviderReply::SessionDiscover(sessions),
                ) => {
                    let account = discovery.scope.as_ref().map(|scope| &scope.account);
                    if sessions
                        .iter()
                        .any(|session| session.key.account.as_ref() != account)
                    {
                        return Err(ValidationError(
                            "Session discovery returned another account's seat",
                        ));
                    }
                }
                (
                    ProviderRequest::SessionReconcile(query),
                    ProviderReply::SessionReconcile(result),
                ) => {
                    let actual = match result {
                        Reconciliation::PendingAllocation { session, ticket } => {
                            if ticket.operation != query.operation || ticket.session != session.key
                            {
                                return Err(ValidationError(
                                    "Pending allocation ticket differs from the original operation or session",
                                ));
                            }
                            Some(&session.key)
                        }
                        Reconciliation::Active { session } => Some(&session.key),
                        Reconciliation::Terminal { session, .. } => Some(session),
                        Reconciliation::Unknown { operation } => {
                            if operation != &query.operation {
                                return Err(ValidationError(
                                    "Reconciliation changed the allocation operation",
                                ));
                            }
                            None
                        }
                        Reconciliation::NotAllocated { operation } => {
                            if operation != &query.operation || query.session.is_some() {
                                return Err(ValidationError(
                                    "No-allocation proof does not match an unresolved operation without a session",
                                ));
                            }
                            None
                        }
                    };
                    if let Some(actual) = actual {
                        if actual.account.as_ref()
                            != query.scope.as_ref().map(|scope| &scope.account)
                            || query
                                .session
                                .as_ref()
                                .is_some_and(|expected| expected != actual)
                        {
                            return Err(ValidationError(
                                "Reconciliation changed the original seat owner",
                            ));
                        }
                    }
                }
                (ProviderRequest::SessionPrepare(query), ProviderReply::SessionPrepare(plan)) => {
                    plan.accepted.validate_against(&query.offer, 0)?;
                }
                _ => {}
            }
        }
        match (&request.request, &self.allocation) {
            (ProviderRequest::SessionCreate(create), Some(ticket)) => {
                if ticket.operation != create.operation
                    || ticket.session.account
                        != create.scope.as_ref().map(|scope| scope.account.clone())
                {
                    return Err(ValidationError(
                        "Allocation receipt does not match the original operation owner",
                    ));
                }
                if let Some(ProviderReply::SessionCreate(reply)) = self.outcome.reply() {
                    if reply.session.key != ticket.session {
                        return Err(ValidationError(
                            "Allocation receipt names a different session",
                        ));
                    }
                }
            }
            (ProviderRequest::SessionCreate(_), None)
                if matches!(self.outcome, ProviderOutcome::Success { .. }) =>
            {
                return Err(ValidationError(
                    "Created session is missing its allocation receipt",
                ));
            }
            (_, Some(_)) => return Err(ValidationError("Unexpected allocation receipt")),
            _ => {}
        }
        Ok(())
    }

    pub fn validate_for_at(
        &self,
        request: &HostRequestV2,
        now_ms: u64,
    ) -> Result<(), ValidationError> {
        self.validate_for(request)?;
        if let (ProviderRequest::SessionPrepare(query), Some(ProviderReply::SessionPrepare(plan))) =
            (&request.request, self.outcome.reply())
        {
            plan.accepted.validate_against(&query.offer, now_ms)?;
        }
        Ok(())
    }
}

impl ProviderReply {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Hello(hello) => {
                if hello.protocol_version != PROVIDER_PROTOCOL_VERSION
                    || semver::Version::parse(hello.version.as_str()).is_err()
                {
                    return Err(ValidationError("Provider greeting version is invalid"));
                }
                let unique: std::collections::BTreeSet<_> =
                    hello.capabilities.iter().copied().collect();
                if unique.len() != hello.capabilities.len() || hello.auth_kinds.is_empty() {
                    return Err(ValidationError(
                        "Provider greeting capabilities are invalid",
                    ));
                }
            }
            Self::AuthStatus(state)
            | Self::AuthBegin(state)
            | Self::AuthPoll(state)
            | Self::AuthComplete(state)
            | Self::AuthLogout(state)
            | Self::AccountsSelect(state) => {
                if let AuthState::Pending { challenge } = state {
                    challenge.validate()?;
                }
                if let AuthState::SignedIn { revision, .. } = state {
                    validate_public_revision(*revision)?;
                }
            }
            Self::AccountsList(accounts) | Self::AccountsRemove(accounts) => {
                validate_public_revision(accounts.revision)?
            }
            Self::ConnectionLink(challenge) => challenge.validate()?,
            Self::CatalogDetails(details) => {
                details.scope.validate_revision()?;
                let mut variants = std::collections::BTreeSet::new();
                if details
                    .variants
                    .iter()
                    .any(|variant| !variants.insert(&variant.id))
                {
                    return Err(ValidationError("Game details contain duplicate variants"));
                }
            }
            Self::SessionPrepare(plan) => {
                plan.accepted.video.validate()?;
                if let Some(audio) = &plan.accepted.audio {
                    audio.validate()?;
                }
                if plan.accepted.runtime_epoch == 0 || plan.accepted.input.gamepad_slots > 4 {
                    return Err(ValidationError("Prepared media binding is invalid"));
                }
            }
            Self::CatalogPublic(page)
            | Self::CatalogLibrary(page)
            | Self::CatalogStore(page)
            | Self::FavoritesList(page) => {
                page.scope.validate_revision()?;
                let mut ids = std::collections::BTreeSet::new();
                if page.items.iter().any(|game| !ids.insert(&game.id)) {
                    return Err(ValidationError("Provider page contains duplicate game IDs"));
                }
            }
            Self::SettingsGet(settings) | Self::SettingsSet(settings) => settings.validate()?,
            _ => {}
        }
        Ok(())
    }
}

impl AuthChallenge {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let (expires_at_ms, poll_after_ms) = match self {
            Self::DeviceCode {
                expires_at_ms,
                poll_after_ms,
                ..
            }
            | Self::Browser {
                expires_at_ms,
                poll_after_ms,
                ..
            }
            | Self::Pairing {
                expires_at_ms,
                poll_after_ms,
                ..
            } => (*expires_at_ms, *poll_after_ms),
        };
        if expires_at_ms == 0 || !(250..=3_600_000).contains(&poll_after_ms) {
            return Err(ValidationError(
                "Authentication challenge has invalid timing",
            ));
        }
        if let Self::Browser { authorization, .. } = self {
            let url = url::Url::parse(authorization.expose_secret())
                .map_err(|_| ValidationError("Private auth navigation URL is invalid"))?;
            let loopback = url
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
            if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(ValidationError(
                    "Private auth navigation URL has an unsupported scheme or credentials",
                ));
            }
        }
        Ok(())
    }
}

impl SettingsView {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_public_revision(self.revision)?;
        let mut keys = std::collections::BTreeSet::new();
        for setting in self.settings.iter() {
            if !keys.insert(&setting.key) {
                return Err(ValidationError("Provider settings contain duplicate keys"));
            }
            match (&setting.control, &setting.value) {
                (SettingControl::Boolean, SettingValue::Boolean(_)) => {}
                (SettingControl::Number { min, max, step }, SettingValue::Number(value))
                    if min.get() <= max.get()
                        && step.get() > 0.0
                        && value.get() >= min.get()
                        && value.get() <= max.get() => {}
                (SettingControl::Integer { min, max, step }, SettingValue::Integer(value))
                    if min <= max
                        && *step > 0
                        && value >= min
                        && value <= max
                        && (i128::from(*value) - i128::from(*min)) % i128::from(*step) == 0 => {}
                (SettingControl::Choice { choices }, SettingValue::Choice(value))
                    if !choices.is_empty() =>
                {
                    let mut unique = std::collections::BTreeSet::new();
                    if choices.iter().any(|choice| !unique.insert(&choice.value))
                        || !choices.iter().any(|choice| &choice.value == value)
                    {
                        return Err(ValidationError("Provider choice setting is invalid"));
                    }
                }
                (SettingControl::Text { maximum_bytes }, SettingValue::Text(value))
                    if (1..=1024).contains(maximum_bytes)
                        && value.as_str().len() <= usize::from(*maximum_bytes) => {}
                _ => {
                    return Err(ValidationError(
                        "Provider setting value does not match its control",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl CatalogScope {
    fn account_scope(&self) -> Option<&AccountScope> {
        match self {
            Self::Public => None,
            Self::Account { scope } => Some(scope),
        }
    }

    fn validate_revision(&self) -> Result<(), ValidationError> {
        if let Some(scope) = self.account_scope() {
            validate_public_revision(scope.revision)?;
        }
        Ok(())
    }
}
