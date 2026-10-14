use super::{AuthKind, Capability, List, PROVIDER_MANIFEST_VERSION, PROVIDER_PROTOCOL_VERSION};
use crate::{PackageFile, PluginId, PluginManifest, ValidationError, validate_package_path};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleEntrypoints {
    pub control: String,
    pub media: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawProviderManifest", rename_all = "camelCase")]
pub struct ProviderManifest {
    pub schema_version: u32,
    pub protocol_version: u32,
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub capabilities: List<Capability, 32>,
    pub auth_kinds: List<AuthKind, 4>,
    pub entrypoints: BTreeMap<String, RoleEntrypoints>,
    pub files: Vec<PackageFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawProviderManifest {
    schema_version: u32,
    protocol_version: u32,
    id: PluginId,
    name: String,
    version: String,
    publisher: String,
    description: String,
    capabilities: List<Capability, 32>,
    auth_kinds: List<AuthKind, 4>,
    #[serde(deserialize_with = "unique_targets")]
    entrypoints: BTreeMap<String, RoleEntrypoints>,
    files: Vec<PackageFile>,
}

impl ProviderManifest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != PROVIDER_MANIFEST_VERSION
            || self.protocol_version != PROVIDER_PROTOCOL_VERSION
        {
            return Err(ValidationError(
                "Provider manifest requires schema 2 and protocol 2",
            ));
        }
        let legacy_inventory = PluginManifest {
            schema_version: 1,
            protocol_version: 1,
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            publisher: self.publisher.clone(),
            description: self.description.clone(),
            capabilities: vec![crate::CATALOG_CAPABILITY.into()],
            entrypoints: self
                .entrypoints
                .iter()
                .map(|(target, roles)| (target.clone(), roles.control.clone()))
                .collect(),
            files: self.files.clone(),
        };
        legacy_inventory.validate()?;
        for roles in self.entrypoints.values() {
            validate_package_path(&roles.media)?;
            if !self.files.iter().any(|file| file.path == roles.media) {
                return Err(ValidationError(
                    "Provider media role is absent from the file inventory",
                ));
            }
        }
        let unique: BTreeSet<_> = self.capabilities.iter().copied().collect();
        if unique.len() != self.capabilities.len() {
            return Err(ValidationError("Provider capabilities must be unique"));
        }
        for required in [
            Capability::CatalogDetails,
            Capability::Launch,
            Capability::Sessions,
            Capability::MediaWorker,
        ] {
            if !unique.contains(&required) {
                return Err(ValidationError(
                    "Provider is missing a required playback capability",
                ));
            }
        }
        if !unique.contains(&Capability::PublicCatalog)
            && !unique.contains(&Capability::LibraryCatalog)
        {
            return Err(ValidationError("Provider must expose a usable catalog"));
        }
        if self.auth_kinds.is_empty() {
            return Err(ValidationError(
                "Provider must explicitly declare its authentication model",
            ));
        }
        for (index, kind) in self.auth_kinds.iter().enumerate() {
            if self.auth_kinds[..index].contains(kind) {
                return Err(ValidationError(
                    "Provider authentication models must be unique",
                ));
            }
            let capability = match kind {
                AuthKind::Anonymous => Capability::AuthAnonymous,
                AuthKind::DeviceCode => Capability::AuthDeviceCode,
                AuthKind::Browser => Capability::AuthBrowser,
                AuthKind::Pairing => Capability::AuthPairing,
            };
            if !unique.contains(&capability)
                || (*kind != AuthKind::Anonymous && !unique.contains(&Capability::Accounts))
            {
                return Err(ValidationError(
                    "Provider authentication capability is incomplete",
                ));
            }
        }
        for (kind, capability) in [
            (AuthKind::Anonymous, Capability::AuthAnonymous),
            (AuthKind::DeviceCode, Capability::AuthDeviceCode),
            (AuthKind::Browser, Capability::AuthBrowser),
            (AuthKind::Pairing, Capability::AuthPairing),
        ] {
            if unique.contains(&capability) != self.auth_kinds.contains(&kind) {
                return Err(ValidationError(
                    "Provider authentication declarations disagree",
                ));
            }
        }
        Ok(())
    }
}

impl TryFrom<RawProviderManifest> for ProviderManifest {
    type Error = ValidationError;

    fn try_from(raw: RawProviderManifest) -> Result<Self, Self::Error> {
        let manifest = Self {
            schema_version: raw.schema_version,
            protocol_version: raw.protocol_version,
            id: raw.id,
            name: raw.name,
            version: raw.version,
            publisher: raw.publisher,
            description: raw.description,
            capabilities: raw.capabilities,
            auth_kinds: raw.auth_kinds,
            entrypoints: raw.entrypoints,
            files: raw.files,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

fn unique_targets<'de, D>(deserializer: D) -> Result<BTreeMap<String, RoleEntrypoints>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Targets;
    impl<'de> serde::de::Visitor<'de> for Targets {
        type Value = BTreeMap<String, RoleEntrypoints>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique provider target roles")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut targets = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                if targets.len() >= 16 || targets.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "Too many or duplicate provider targets",
                    ));
                }
            }
            Ok(targets)
        }
    }
    deserializer.deserialize_map(Targets)
}
