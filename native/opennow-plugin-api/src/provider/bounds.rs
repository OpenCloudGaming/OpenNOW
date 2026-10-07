use crate::ValidationError;
use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::Deref;
use zeroize::Zeroize;

pub const MAX_PUBLIC_REVISION: u64 = (1_u64 << 53) - 1;

pub(crate) fn validate_public_revision(revision: u64) -> Result<(), ValidationError> {
    if revision > MAX_PUBLIC_REVISION {
        return Err(ValidationError(
            "Public revision exceeds the exact QML integer range",
        ));
    }
    Ok(())
}

pub(crate) fn deserialize_public_revision<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let revision = u64::deserialize(deserializer)?;
    validate_public_revision(revision).map_err(serde::de::Error::custom)?;
    Ok(revision)
}

pub(crate) fn serialize_public_revision<S: Serializer>(
    revision: &u64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    validate_public_revision(*revision).map_err(serde::ser::Error::custom)?;
    revision.serialize(serializer)
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Text<const MAX: usize>(String);

impl<const MAX: usize> Text<MAX> {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        crate::validate_text(&value, MAX)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> fmt::Debug for Text<MAX> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Text([redacted])")
    }
}

impl<const MAX: usize> Serialize for Text<MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for Text<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

macro_rules! identifiers {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Text<256>);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                Text::new(value).map(Self)
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
    )+};
}

identifiers!(
    AccountId,
    AuthorityId,
    GameId,
    VariantId,
    SessionId,
    OperationId,
    ReceiptId,
    AttemptId,
    ConnectionId,
    ObservationId,
    SettingKey,
    RegionId,
    StorageId,
    AdId
);

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OfferId(Text<128>);

impl OfferId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FiniteNumber(f64);

impl Eq for FiniteNumber {}

impl FiniteNumber {
    pub fn new(value: f64) -> Result<Self, ValidationError> {
        if !value.is_finite() || value.abs() > 1_000_000_000_000.0 {
            return Err(ValidationError(
                "Provider number must be finite and bounded",
            ));
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> f64 {
        self.0
    }
}

impl Serialize for FiniteNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FiniteNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct SettingText(String);

impl SettingText {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.len() > 1024 || value.chars().any(char::is_control) {
            return Err(ValidationError(
                "Setting text is invalid or exceeds its size limit",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SettingText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SettingText([redacted])")
    }
}

impl Serialize for SettingText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SettingText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct List<T, const MAX: usize>(Vec<T>);

impl<T, const MAX: usize> List<T, MAX> {
    pub fn new(values: Vec<T>) -> Result<Self, ValidationError> {
        if values.len() > MAX {
            return Err(ValidationError(
                "Provider collection exceeds its size limit",
            ));
        }
        Ok(Self(values))
    }

    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T, const MAX: usize> Default for List<T, MAX> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T, const MAX: usize> Deref for List<T, MAX> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>, const MAX: usize> Deserialize<'de> for List<T, MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Vec::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let mut value = value.into();
        if value.is_empty() || value.len() > 16 * 1024 || value.contains('\0') {
            value.zeroize();
            return Err(ValidationError(
                "Private provider text has an invalid length",
            ));
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([redacted])")
    }
}

impl Serialize for SecretString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(mut value: Vec<u8>) -> Result<Self, ValidationError> {
        if value.len() > 256 * 1024 {
            value.zeroize();
            return Err(ValidationError(
                "Private provider bootstrap exceeds its size limit",
            ));
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretBytes([redacted])")
    }
}

impl Serialize for SecretBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut encoded = base64::engine::general_purpose::STANDARD.encode(&self.0);
        let result = encoded.serialize(serializer);
        encoded.zeroize();
        result
    }
}

impl<'de> Deserialize<'de> for SecretBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut encoded = String::deserialize(deserializer)?;
        if encoded.len() > (256usize * 1024).div_ceil(3) * 4 {
            encoded.zeroize();
            return Err(serde::de::Error::custom(
                "Private provider bootstrap exceeds its size limit",
            ));
        }
        let decoded = base64::engine::general_purpose::STANDARD.decode(&encoded);
        encoded.zeroize();
        let decoded = decoded.map_err(|_| {
            serde::de::Error::custom("Private provider bootstrap is not valid base64")
        })?;
        Self::new(decoded).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PublicUrl(String);

impl fmt::Debug for PublicUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PublicUrl([redacted])")
    }
}

impl PublicUrl {
    pub fn new(value: String) -> Result<Self, ValidationError> {
        if value.len() > 4096 {
            return Err(ValidationError(
                "Public provider URL exceeds its size limit",
            ));
        }
        let url = url::Url::parse(&value)
            .map_err(|_| ValidationError("Public provider URL is invalid"))?;
        let loopback = url
            .host_str()
            .is_some_and(|host| host == "localhost" || host == "[::1]" || host == "127.0.0.1");
        if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ValidationError(
                "Public provider URL must be credential-free HTTPS or loopback HTTP",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PublicUrl {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PublicUrl> for String {
    fn from(value: PublicUrl) -> Self {
        value.0
    }
}
