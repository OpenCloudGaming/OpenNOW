#![doc = include_str!("../README.md")]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub mod manifest;
pub mod media;
pub mod provider;
pub mod wire;

pub use manifest::*;
pub use wire::*;

pub const PROTOCOL_VERSION: u32 = 1;
pub const BUILTIN_GFN_ID: &str = "org.opennow.geforce-now";
pub const EXAMPLE_PLUGIN_ID: &str = "org.opennow.example.catalog";
pub const CATALOG_CAPABILITY: &str = "catalog.v1";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MAX_CURSOR_BYTES: usize = 4096;
pub const MAX_PAGE_ITEMS: usize = 100;
pub const MAX_ITEM_TEXT_BYTES: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError(pub &'static str);

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ValidationError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PluginId(String);

impl PluginId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.len() > 128 || value.split('.').count() < 3 {
            return Err(ValidationError(
                "Plugin ID must be a bounded reverse-domain identifier",
            ));
        }
        for segment in value.split('.') {
            if segment.is_empty()
                || segment.len() > 63
                || !segment.as_bytes()[0].is_ascii_lowercase()
                || !segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(ValidationError("Plugin ID contains an invalid segment"));
            }
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_builtin(&self) -> bool {
        self.0 == BUILTIN_GFN_ID
    }
}

impl TryFrom<String> for PluginId {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PluginId> for String {
    fn from(value: PluginId) -> Self {
        value.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCatalogQuery")]
pub struct CatalogQuery {
    pub query: String,
    pub cursor: Option<String>,
    pub limit: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalogQuery {
    #[serde(default)]
    query: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default = "default_limit")]
    limit: u16,
}

fn default_limit() -> u16 {
    20
}

impl Default for CatalogQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            cursor: None,
            limit: default_limit(),
        }
    }
}

impl CatalogQuery {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.query.len() > MAX_QUERY_BYTES || self.query.chars().any(char::is_control) {
            return Err(ValidationError(
                "Catalog query is invalid or exceeds 512 bytes",
            ));
        }
        if !(1..=MAX_PAGE_ITEMS as u16).contains(&self.limit) {
            return Err(ValidationError("Catalog limit must be between 1 and 100"));
        }
        validate_cursor(self.cursor.as_deref())
    }
}

impl TryFrom<RawCatalogQuery> for CatalogQuery {
    type Error = ValidationError;

    fn try_from(value: RawCatalogQuery) -> Result<Self, Self::Error> {
        let query = Self {
            query: value.query,
            cursor: value.cursor,
            limit: value.limit,
        };
        query.validate()?;
        Ok(query)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogItem {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Coverage {
    Partial,
    Complete,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCatalogPage", rename_all = "camelCase")]
pub struct CatalogPage {
    pub items: Vec<CatalogItem>,
    pub next_cursor: Option<String>,
    pub coverage: Coverage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawCatalogPage {
    items: Vec<CatalogItem>,
    next_cursor: Option<String>,
    coverage: Coverage,
}

impl CatalogPage {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.items.len() > MAX_PAGE_ITEMS {
            return Err(ValidationError("Catalog page exceeds 100 items"));
        }
        let mut ids = HashSet::new();
        for item in &self.items {
            validate_text(&item.id, MAX_ITEM_TEXT_BYTES)?;
            validate_text(&item.title, MAX_ITEM_TEXT_BYTES)?;
            if !ids.insert(&item.id) {
                return Err(ValidationError("Catalog page contains duplicate IDs"));
            }
        }
        validate_cursor(self.next_cursor.as_deref())
    }
}

impl TryFrom<RawCatalogPage> for CatalogPage {
    type Error = ValidationError;

    fn try_from(value: RawCatalogPage) -> Result<Self, Self::Error> {
        let page = Self {
            items: value.items,
            next_cursor: value.next_cursor,
            coverage: value.coverage,
        };
        page.validate()?;
        Ok(page)
    }
}

fn validate_cursor(cursor: Option<&str>) -> Result<(), ValidationError> {
    if cursor.is_some_and(|value| {
        value.is_empty() || value.len() > MAX_CURSOR_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(ValidationError(
            "Catalog cursor is invalid or exceeds 4096 bytes",
        ));
    }
    Ok(())
}

pub fn validate_text(value: &str, maximum: usize) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ValidationError(
            "Display text is empty, invalid, or too long",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceItemRef {
    pub source_id: PluginId,
    pub local_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCatalogItem {
    pub id: SourceItemRef,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCatalogPage {
    pub source_id: PluginId,
    pub generation: u64,
    pub items: Vec<SourceCatalogItem>,
    pub next_cursor: Option<String>,
    pub coverage: Coverage,
}

impl SourceCatalogPage {
    pub fn bind(source_id: PluginId, generation: u64, page: CatalogPage) -> Self {
        Self {
            items: page
                .items
                .into_iter()
                .map(|item| SourceCatalogItem {
                    id: SourceItemRef {
                        source_id: source_id.clone(),
                        local_id: item.id,
                    },
                    title: item.title,
                })
                .collect(),
            source_id,
            generation,
            next_cursor: page.next_cursor,
            coverage: page.coverage,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    Disabled,
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PluginTrust {
    #[serde(rename = "builtin")]
    Builtin,
    #[serde(rename = "unsigned-native")]
    UnsignedNative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDescriptor {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub builtin: bool,
    pub required: bool,
    pub enabled: bool,
    pub state: PluginState,
    pub capabilities: Vec<String>,
    pub trust: PluginTrust,
    pub last_error: Option<PluginError>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSnapshot {
    pub generation: u64,
    pub plugins: Vec<PluginDescriptor>,
}
