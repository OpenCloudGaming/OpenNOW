use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    version: String,
    asset: String,
    size: u64,
    sha256: String,
    signature: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("opennow-update-manifest: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let asset = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: opennow-update-manifest <asset> <version> [output]".to_owned())?;
    let version = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "update version is required".to_owned())?
        .trim_start_matches('v')
        .to_owned();
    let output = arguments.next().map(PathBuf::from).unwrap_or_else(|| {
        let name = asset.file_name().unwrap_or_default().to_string_lossy();
        asset.with_file_name(format!("{name}.manifest.json"))
    });
    if arguments.next().is_some() || parse_version(&version).is_none() {
        return Err("update manifest arguments are invalid".to_owned());
    }
    let asset_name = asset
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| safe_asset_name(value))
        .ok_or_else(|| "update asset name is unsafe".to_owned())?
        .to_owned();
    let key = signing_key()?;
    let verifying_key = configured_verifying_key()?;
    if key.verifying_key() != verifying_key {
        return Err(
            "OPENNOW_UPDATE_ED25519_PUBLIC_KEY does not match the protected signing key".to_owned(),
        );
    }
    let (size, sha256) = hash_file(&asset)?;
    let mut manifest = Manifest {
        schema_version: 1,
        version,
        asset: asset_name,
        size,
        sha256,
        signature: String::new(),
    };
    let signature = key.sign(signature_payload(&manifest).as_bytes());
    verifying_key
        .verify_strict(signature_payload(&manifest).as_bytes(), &signature)
        .map_err(|_| "generated update signature did not verify".to_owned())?;
    manifest.signature = BASE64.encode(signature.to_bytes());
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    let temporary = output.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("could not create manifest: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("could not write manifest: {error}"))?;
    fs::rename(&temporary, &output)
        .map_err(|error| format!("could not finalize manifest: {error}"))?;
    println!("{}", output.display());
    Ok(())
}

fn signing_key() -> Result<SigningKey, String> {
    let encoded = env::var("OPENNOW_UPDATE_ED25519_PRIVATE_KEY")
        .map_err(|_| "OPENNOW_UPDATE_ED25519_PRIVATE_KEY is required".to_owned())?;
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| "update signing key is not valid base64".to_owned())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "update signing key must be a 32-byte Ed25519 seed".to_owned())?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn configured_verifying_key() -> Result<VerifyingKey, String> {
    let encoded = env::var("OPENNOW_UPDATE_ED25519_PUBLIC_KEY")
        .map_err(|_| "OPENNOW_UPDATE_ED25519_PUBLIC_KEY is required".to_owned())?;
    decode_verifying_key(&encoded)
}

fn decode_verifying_key(encoded: &str) -> Result<VerifyingKey, String> {
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| "update verifying key is not valid base64".to_owned())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "update verifying key must be 32 bytes".to_owned())?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "update verifying key is invalid".to_owned())
}

fn hash_file(path: &Path) -> Result<(u64, String), String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("could not read update asset: {error}"))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("could not hash update asset: {error}"))?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| "update asset size overflowed".to_owned())?;
        hasher.update(&buffer[..count]);
    }
    if size == 0 {
        return Err("update asset is empty".to_owned());
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn signature_payload(manifest: &Manifest) -> String {
    format!(
        "OpenNOW update manifest v1\nversion={}\nasset={}\nsize={}\nsha256={}\n",
        manifest.version, manifest.asset, manifest.size, manifest.sha256
    )
}

fn safe_asset_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && !value.contains(['/', '\\'])
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_key_decoder_rejects_malformed_and_accepts_the_signing_pair() {
        assert!(decode_verifying_key("not-base64").is_err());
        assert!(decode_verifying_key(&BASE64.encode([0_u8; 31])).is_err());
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let encoded = BASE64.encode(signing.verifying_key().as_bytes());
        assert_eq!(
            decode_verifying_key(&encoded).unwrap(),
            signing.verifying_key()
        );
    }

    #[test]
    fn canonical_manifest_signature_verifies_and_detects_tampering() {
        let signing = SigningKey::from_bytes(&[9_u8; 32]);
        let manifest = Manifest {
            schema_version: 1,
            version: "1.2.3".to_owned(),
            asset: "OpenNOW-Qt-1.2.3.deb".to_owned(),
            size: 42,
            sha256: "0".repeat(64),
            signature: String::new(),
        };
        let signature = signing.sign(signature_payload(&manifest).as_bytes());
        assert!(
            signing
                .verifying_key()
                .verify_strict(signature_payload(&manifest).as_bytes(), &signature)
                .is_ok()
        );
        let mut tampered = manifest;
        tampered.size += 1;
        assert!(
            signing
                .verifying_key()
                .verify_strict(signature_payload(&tampered).as_bytes(), &signature)
                .is_err()
        );
    }
}
