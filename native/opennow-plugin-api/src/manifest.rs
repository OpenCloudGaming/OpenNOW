use crate::{CATALOG_CAPABILITY, PROTOCOL_VERSION, PluginId, ValidationError, validate_text};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_PACKAGE_FILES: usize = 127;
pub const MAX_PACKAGE_PATH_BYTES: usize = 240;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPluginManifest", rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub protocol_version: u32,
    pub capabilities: Vec<String>,
    pub entrypoints: BTreeMap<String, String>,
    pub files: Vec<PackageFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPluginManifest {
    schema_version: u32,
    id: PluginId,
    name: String,
    version: String,
    publisher: String,
    description: String,
    protocol_version: u32,
    capabilities: Vec<String>,
    #[serde(deserialize_with = "unique_entrypoints")]
    entrypoints: BTreeMap<String, String>,
    files: Vec<PackageFile>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != 1 || self.protocol_version != PROTOCOL_VERSION {
            return Err(ValidationError("Plugin version is not supported"));
        }
        if self.id.is_builtin() {
            return Err(ValidationError("The built-in plugin ID is reserved"));
        }
        validate_text(&self.name, 128)?;
        validate_text(&self.publisher, 128)?;
        validate_text(&self.description, 1024)?;
        if self.version.len() > 64 || semver::Version::parse(&self.version).is_err() {
            return Err(ValidationError("Plugin version must be valid SemVer"));
        }
        if self.capabilities != [CATALOG_CAPABILITY] {
            return Err(ValidationError("Only catalog.v1 is supported"));
        }
        if self.files.is_empty() || self.files.len() > MAX_PACKAGE_FILES {
            return Err(ValidationError(
                "Plugin file inventory is empty or too large",
            ));
        }
        let mut paths = HashSet::new();
        for file in &self.files {
            validate_package_path(&file.path)?;
            if file.path.eq_ignore_ascii_case("manifest.json")
                || !paths.insert(file.path.to_ascii_lowercase())
            {
                return Err(ValidationError(
                    "Plugin file inventory contains a reserved or duplicate path",
                ));
            }
            if file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ValidationError(
                    "Plugin file hash must be lowercase SHA-256",
                ));
            }
        }
        if self.entrypoints.is_empty() || self.entrypoints.len() > 16 {
            return Err(ValidationError(
                "Plugin entrypoint inventory is empty or too large",
            ));
        }
        for (target, path) in &self.entrypoints {
            if target.len() > 128
                || target.split('-').count() < 3
                || !target.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
            {
                return Err(ValidationError("Plugin target triple is invalid"));
            }
            validate_package_path(path)?;
            if !self.files.iter().any(|file| &file.path == path) {
                return Err(ValidationError(
                    "Plugin entrypoint is not in the file inventory",
                ));
            }
        }
        Ok(())
    }
}

impl TryFrom<RawPluginManifest> for PluginManifest {
    type Error = ValidationError;

    fn try_from(raw: RawPluginManifest) -> Result<Self, Self::Error> {
        let manifest = Self {
            schema_version: raw.schema_version,
            id: raw.id,
            name: raw.name,
            version: raw.version,
            publisher: raw.publisher,
            description: raw.description,
            protocol_version: raw.protocol_version,
            capabilities: raw.capabilities,
            entrypoints: raw.entrypoints,
            files: raw.files,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

pub fn validate_package_path(path: &str) -> Result<(), ValidationError> {
    if path.is_empty()
        || !path.is_ascii()
        || path.len() > MAX_PACKAGE_PATH_BYTES
        || path.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
    {
        return Err(ValidationError("Plugin package path is invalid"));
    }
    for part in path.split('/') {
        let base = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(ValidationError("Plugin package path is reserved"));
        }
    }
    Ok(())
}

fn unique_entrypoints<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique target entrypoints")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut entries = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if entries.len() >= 16 || entries.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "Too many or duplicate target entrypoints",
                    ));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Entries)
}
