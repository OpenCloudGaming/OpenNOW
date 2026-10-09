use crate::{
    CATALOG_CAPABILITY, CatalogPage, CatalogQuery, PROTOCOL_VERSION, PluginId, ValidationError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelloRequest {
    pub plugin_id: PluginId,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelloReply {
    pub plugin_id: PluginId,
    pub version: String,
    pub protocol_version: u32,
    pub capabilities: Vec<String>,
}

impl HelloReply {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol_version != PROTOCOL_VERSION
            || self.capabilities != [CATALOG_CAPABILITY]
            || self.version.len() > 64
            || semver::Version::parse(&self.version).is_err()
        {
            return Err(ValidationError("Plugin hello is incompatible"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestPayload {
    Hello(HelloRequest),
    CatalogPage(CatalogQuery),
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostRequest {
    pub v: u32,
    pub epoch: u64,
    pub id: String,
    pub payload: RequestPayload,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawHostMessage", into = "RawHostMessage")]
pub enum HostMessage {
    Request(HostRequest),
    Cancel { v: u32, epoch: u64, id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum RawHostMessage {
    Request {
        v: u32,
        epoch: u64,
        id: String,
        op: Operation,
        args: Value,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u64,
    },
    Cancel {
        v: u32,
        epoch: u64,
        id: String,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Operation {
    #[serde(rename = "plugin.hello")]
    Hello,
    #[serde(rename = "catalog.page")]
    CatalogPage,
    #[serde(rename = "plugin.shutdown")]
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownReply {}

impl TryFrom<RawHostMessage> for HostMessage {
    type Error = ValidationError;

    fn try_from(raw: RawHostMessage) -> Result<Self, Self::Error> {
        match raw {
            RawHostMessage::Cancel { v, epoch, id } => {
                validate_envelope(v, epoch, &id)?;
                Ok(Self::Cancel { v, epoch, id })
            }
            RawHostMessage::Request {
                v,
                epoch,
                id,
                op,
                args,
                timeout_ms,
            } => {
                validate_envelope(v, epoch, &id)?;
                if timeout_ms == 0 || timeout_ms > 10_000 {
                    return Err(ValidationError("Plugin request timeout is invalid"));
                }
                let payload = match op {
                    Operation::Hello => {
                        let hello: HelloRequest = serde_json::from_value(args)
                            .map_err(|_| ValidationError("Plugin hello request is invalid"))?;
                        if hello.capabilities != [CATALOG_CAPABILITY] {
                            return Err(ValidationError("Plugin capabilities are incompatible"));
                        }
                        RequestPayload::Hello(hello)
                    }
                    Operation::CatalogPage => RequestPayload::CatalogPage(
                        serde_json::from_value(args)
                            .map_err(|_| ValidationError("Catalog request is invalid"))?,
                    ),
                    Operation::Shutdown => {
                        serde_json::from_value::<ShutdownReply>(args)
                            .map_err(|_| ValidationError("Plugin shutdown request is invalid"))?;
                        RequestPayload::Shutdown
                    }
                };
                Ok(Self::Request(HostRequest {
                    v,
                    epoch,
                    id,
                    payload,
                    timeout_ms,
                }))
            }
        }
    }
}

impl From<HostMessage> for RawHostMessage {
    fn from(message: HostMessage) -> Self {
        match message {
            HostMessage::Cancel { v, epoch, id } => Self::Cancel { v, epoch, id },
            HostMessage::Request(request) => {
                let (op, args) = match request.payload {
                    RequestPayload::Hello(hello) => (
                        Operation::Hello,
                        serde_json::to_value(hello).expect("serializable hello"),
                    ),
                    RequestPayload::CatalogPage(query) => (
                        Operation::CatalogPage,
                        serde_json::to_value(query).expect("serializable query"),
                    ),
                    RequestPayload::Shutdown => (Operation::Shutdown, serde_json::json!({})),
                };
                Self::Request {
                    v: request.v,
                    epoch: request.epoch,
                    id: request.id,
                    op,
                    args,
                    timeout_ms: request.timeout_ms,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReplyPayload {
    Hello(HelloReply),
    CatalogPage(CatalogPage),
    Shutdown(ShutdownReply),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginFailureCode {
    Cancelled,
    InvalidRequest,
    UnsupportedCapability,
    InternalError,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginFailure {
    pub code: PluginFailureCode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPluginMessage", into = "RawPluginMessage")]
pub struct PluginMessage {
    pub v: u32,
    pub epoch: u64,
    pub id: String,
    pub outcome: Result<ReplyPayload, PluginFailure>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum ResponseTag {
    #[serde(rename = "response")]
    Response,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPluginMessage {
    v: u32,
    #[serde(rename = "type")]
    message_type: ResponseTag,
    epoch: u64,
    id: String,
    ok: bool,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    result: Option<ReplyPayload>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    error: Option<PluginFailure>,
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl TryFrom<RawPluginMessage> for PluginMessage {
    type Error = ValidationError;

    fn try_from(raw: RawPluginMessage) -> Result<Self, Self::Error> {
        validate_envelope(raw.v, raw.epoch, &raw.id)?;
        let outcome = match (raw.ok, raw.result, raw.error) {
            (true, Some(reply), None) => {
                if let ReplyPayload::Hello(hello) = &reply {
                    hello.validate()?;
                }
                Ok(reply)
            }
            (false, None, Some(error)) => Err(error),
            _ => {
                return Err(ValidationError(
                    "Plugin response must contain exactly one outcome",
                ));
            }
        };
        Ok(Self {
            v: raw.v,
            epoch: raw.epoch,
            id: raw.id,
            outcome,
        })
    }
}

impl From<PluginMessage> for RawPluginMessage {
    fn from(message: PluginMessage) -> Self {
        let (ok, result, error) = match message.outcome {
            Ok(result) => (true, Some(result), None),
            Err(error) => (false, None, Some(error)),
        };
        Self {
            v: message.v,
            message_type: ResponseTag::Response,
            epoch: message.epoch,
            id: message.id,
            ok,
            result,
            error,
        }
    }
}

fn validate_envelope(version: u32, epoch: u64, id: &str) -> Result<(), ValidationError> {
    if version != PROTOCOL_VERSION || epoch == 0 {
        return Err(ValidationError(
            "Plugin protocol version or epoch is invalid",
        ));
    }
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ValidationError("Plugin request ID is invalid"));
    }
    Ok(())
}
