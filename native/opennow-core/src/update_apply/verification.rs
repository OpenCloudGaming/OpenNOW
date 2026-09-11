use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const MAXIMUM_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAXIMUM_UPDATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub version: String,
    pub asset: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

pub fn embedded_update_key() -> Result<VerifyingKey, String> {
    let encoded = option_env!("OPENNOW_UPDATE_ED25519_PUBLIC_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("This build has no pinned update signing key")?;
    decode_verifying_key(encoded)
}

pub fn decode_verifying_key(encoded: &str) -> Result<VerifyingKey, String> {
    let bytes: [u8; 32] = BASE64
        .decode(encoded)
        .map_err(|_| "Pinned update signing key is not valid base64")?
        .try_into()
        .map_err(|_| "Pinned update signing key has the wrong length")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "Pinned update signing key is invalid".to_owned())
}

pub fn signature_payload(manifest: &UpdateManifest) -> String {
    format!(
        "OpenNOW update manifest v1\nversion={}\nasset={}\nsize={}\nsha256={}\n",
        manifest.version.trim_start_matches('v'),
        manifest.asset,
        manifest.size,
        manifest.sha256.to_ascii_lowercase()
    )
}

pub fn verify_manifest(manifest: &UpdateManifest, key: &VerifyingKey) -> Result<(), String> {
    if manifest.schema_version != 1
        || semver::Version::parse(
            manifest
                .version
                .strip_prefix('v')
                .unwrap_or(&manifest.version),
        )
        .is_err()
        || !safe_asset_name(&manifest.asset)
        || manifest.size == 0
        || manifest.size > MAXIMUM_UPDATE_BYTES
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Signed update metadata fields are invalid".to_owned());
    }
    let bytes = BASE64
        .decode(&manifest.signature)
        .map_err(|_| "Update signature is not valid base64")?;
    let signature =
        Signature::from_slice(&bytes).map_err(|_| "Update signature has the wrong length")?;
    key.verify_strict(signature_payload(manifest).as_bytes(), &signature)
        .map_err(|_| "Update manifest signature is invalid".to_owned())
}

pub fn safe_asset_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub fn verify_signed_manifest(bytes: &[u8]) -> Result<UpdateManifest, String> {
    if bytes.len() as u64 > MAXIMUM_MANIFEST_BYTES {
        return Err("Update manifest exceeds its size limit".to_owned());
    }
    let manifest = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid update manifest: {error}"))?;
    verify_manifest(&manifest, &embedded_update_key()?)?;
    Ok(manifest)
}

pub fn verify_package(path: &Path, manifest: &UpdateManifest) -> Result<(), String> {
    if path.file_name().and_then(|name| name.to_str()) != Some(manifest.asset.as_str()) {
        return Err("Update package name does not match its signed manifest".to_owned());
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() != manifest.size {
        return Err(
            "Update package size or file type does not match its signed manifest".to_owned(),
        );
    }
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let (size, hash) = hash_reader(&mut file, MAXIMUM_UPDATE_BYTES)?;
    if size != manifest.size || !hash.eq_ignore_ascii_case(&manifest.sha256) {
        return Err("Update package SHA256 does not match its signed manifest".to_owned());
    }
    Ok(())
}

pub(super) fn hash_reader(reader: &mut impl Read, maximum: u64) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let length = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if length == 0 {
            break;
        }
        total = total
            .checked_add(length as u64)
            .ok_or("Update size overflow")?;
        if total > maximum {
            return Err("Update data exceeds its size limit".to_owned());
        }
        hasher.update(&buffer[..length]);
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}
