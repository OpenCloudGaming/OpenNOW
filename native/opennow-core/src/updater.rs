use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

const RELEASES_URL: &str =
    "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases?per_page=20";
const RELEASES_PAGE: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases";
const RELEASE_ASSET_PREFIX: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases/download/";
const MAXIMUM_MANIFEST_BYTES: u64 = 64 * 1024;
const MAXIMUM_UPDATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone)]
struct AvailableUpdate {
    version: String,
    asset: Asset,
    manifest_url: String,
}

#[derive(Clone)]
struct DownloadedUpdate {
    version: String,
    asset_name: String,
    path: PathBuf,
    size: u64,
    sha256: String,
}

#[derive(Clone)]
struct State {
    status: &'static str,
    available_version: Option<String>,
    release_url: Option<String>,
    notes: Option<String>,
    message: String,
    last_checked_at: Option<u128>,
    available: Option<AvailableUpdate>,
    downloaded: Option<DownloadedUpdate>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    schema_version: u32,
    version: String,
    asset: String,
    size: u64,
    sha256: String,
    signature: String,
}

pub struct UpdaterService {
    client: Client,
    staging_dir: PathBuf,
    state: Mutex<State>,
}

impl UpdaterService {
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|error| error.to_string())?;
        let staging_dir = data_dir.join("updates");
        fs::create_dir_all(&staging_dir)
            .map_err(|error| format!("Could not create the update staging directory: {error}"))?;
        Ok(Self {
            client,
            staging_dir,
            state: Mutex::new(State {
                status: "idle",
                available_version: None,
                release_url: None,
                notes: None,
                message: "Ready to check GitHub Releases".to_owned(),
                last_checked_at: None,
                available: None,
                downloaded: None,
            }),
        })
    }

    pub fn state(&self) -> Value {
        state_json(&self.state.lock().expect("updater state poisoned"))
    }

    pub fn check(&self, params: &Value) -> Result<Value, String> {
        let channel = params["channel"].as_str().unwrap_or("stable");
        if !matches!(channel, "stable" | "nightly") {
            return Err("Unsupported update channel".to_owned());
        }
        {
            let mut state = self.state.lock().expect("updater state poisoned");
            state.status = "checking";
            state.message = "Checking GitHub Releases…".to_owned();
        }
        let releases = self
            .client
            .get(RELEASES_URL)
            .header(USER_AGENT, "OpenNOW-Qt/0.5")
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .map_err(|error| friendly_network_error(&error.to_string()))
            .and_then(|response| {
                if !response.status().is_success() {
                    return Err(format!(
                        "GitHub Releases returned HTTP {}",
                        response.status().as_u16()
                    ));
                }
                response
                    .json::<Vec<Release>>()
                    .map_err(|_| "GitHub Releases returned invalid metadata".to_owned())
            });
        let releases = match releases {
            Ok(releases) => releases,
            Err(error) => {
                let mut state = self.state.lock().expect("updater state poisoned");
                state.status = "error";
                state.message = error.clone();
                return Err(error);
            }
        };
        let current = parse_version(crate::version::APPLICATION_VERSION)
            .ok_or_else(|| "Current application version is invalid".to_owned())?;
        let release = select_release(&releases, channel, current);
        let mut state = self.state.lock().expect("updater state poisoned");
        state.last_checked_at = Some(now_ms());
        state.downloaded = None;
        if let Some(release) = release {
            let version = release.tag_name.trim_start_matches('v').to_owned();
            let compatible = compatible_asset(&release.assets).cloned();
            let manifest_url = compatible.as_ref().and_then(|asset| {
                let expected = format!("{}.manifest.json", asset.name);
                release
                    .assets
                    .iter()
                    .find(|candidate| candidate.name == expected && trusted_asset_url(candidate))
                    .map(|candidate| candidate.browser_download_url.clone())
            });
            state.status = "available";
            state.available_version = Some(version.clone());
            state.release_url = trusted_release_url(&release.html_url)
                .then(|| release.html_url.clone())
                .or_else(|| Some(RELEASES_PAGE.to_owned()));
            state.notes = release.body.as_deref().map(|value| bounded(value, 20_000));
            state.available =
                compatible
                    .zip(manifest_url)
                    .map(|(asset, manifest_url)| AvailableUpdate {
                        version: version.clone(),
                        asset,
                        manifest_url,
                    });
            state.message = if state.available.is_none() {
                format!(
                    "OpenNOW {version} is available; this release has no signed Qt update package for this platform."
                )
            } else if embedded_update_key().is_err() {
                format!(
                    "OpenNOW {version} is available; this validation build has no pinned update signing key."
                )
            } else {
                format!("OpenNOW {version} is available with signed update metadata.")
            };
        } else {
            state.status = "not-available";
            state.available_version = None;
            state.release_url = Some(RELEASES_PAGE.to_owned());
            state.notes = None;
            state.available = None;
            state.message = "OpenNOW is up to date.".to_owned();
        }
        Ok(state_json(&state))
    }

    pub fn download(&self) -> Result<Value, String> {
        let available = {
            let mut state = self.state.lock().expect("updater state poisoned");
            let available = state
                .available
                .clone()
                .ok_or_else(|| "No signed update package is available".to_owned())?;
            embedded_update_key()?;
            state.status = "downloading";
            state.message = format!("Downloading OpenNOW {}…", available.version);
            available
        };
        match self.download_verified(&available) {
            Ok(downloaded) => {
                let mut state = self.state.lock().expect("updater state poisoned");
                state.status = "downloaded";
                state.message = format!(
                    "OpenNOW {} downloaded and verified. Ready to install.",
                    downloaded.version
                );
                state.downloaded = Some(downloaded);
                Ok(state_json(&state))
            }
            Err(error) => {
                let mut state = self.state.lock().expect("updater state poisoned");
                state.status = "available";
                state.message = format!("Update download failed verification: {error}");
                state.downloaded = None;
                Err(error)
            }
        }
    }

    pub fn install(&self, params: &Value) -> Result<Value, String> {
        if params["confirmed"].as_bool() != Some(true) {
            return Err("Update installation requires explicit confirmation".to_owned());
        }
        let downloaded = self
            .state
            .lock()
            .expect("updater state poisoned")
            .downloaded
            .clone()
            .ok_or_else(|| "No verified update has been downloaded".to_owned())?;
        verify_downloaded_file(&downloaded)?;
        let action = launch_verified_update(&downloaded.path)?;
        let mut state = self.state.lock().expect("updater state poisoned");
        state.status = "installing";
        state.message = format!("Verified update launched ({action}). OpenNOW will close.");
        Ok(state_json(&state))
    }

    pub fn highlights(&self) -> Value {
        let state = self.state.lock().expect("updater state poisoned");
        json!({
            "version": state.available_version,
            "title": state.available_version.as_ref().map(|version| format!("OpenNOW {version}")),
            "bodyMarkdown": state.notes,
            "releaseUrl": state.release_url
        })
    }

    fn download_verified(&self, available: &AvailableUpdate) -> Result<DownloadedUpdate, String> {
        let key = embedded_update_key()?;
        let manifest_response = self
            .client
            .get(&available.manifest_url)
            .header(USER_AGENT, "OpenNOW-Qt/0.5")
            .send()
            .map_err(|_| "Could not download signed update metadata".to_owned())?;
        ensure_asset_response(&manifest_response, MAXIMUM_MANIFEST_BYTES)?;
        let manifest_bytes = read_bounded(manifest_response, MAXIMUM_MANIFEST_BYTES)?;
        let manifest = serde_json::from_slice::<UpdateManifest>(&manifest_bytes)
            .map_err(|_| "Signed update metadata is invalid JSON".to_owned())?;
        verify_manifest(&manifest, &key)?;
        if manifest.version.trim_start_matches('v') != available.version
            || manifest.asset != available.asset.name
            || manifest.size != available.asset.size
            || manifest.size == 0
            || manifest.size > MAXIMUM_UPDATE_BYTES
        {
            return Err(
                "Signed update metadata does not match the selected release asset".to_owned(),
            );
        }
        if !safe_asset_name(&manifest.asset) || !trusted_asset_url(&available.asset) {
            return Err("Update asset name or URL is not trusted".to_owned());
        }

        let response = self
            .client
            .get(&available.asset.browser_download_url)
            .header(USER_AGENT, "OpenNOW-Qt/0.5")
            .send()
            .map_err(|_| "Could not download the update package".to_owned())?;
        ensure_asset_response(&response, manifest.size)?;
        let final_path = self.staging_dir.join(&manifest.asset);
        let partial_path = self.staging_dir.join(format!(".{}.part", manifest.asset));
        let _ = fs::remove_file(&partial_path);
        let result = write_verified_asset(response, &partial_path, &manifest);
        if let Err(error) = result {
            let _ = fs::remove_file(&partial_path);
            return Err(error);
        }
        if final_path.exists() {
            fs::remove_file(&final_path)
                .map_err(|error| format!("Could not replace the staged update: {error}"))?;
        }
        fs::rename(&partial_path, &final_path)
            .map_err(|error| format!("Could not finalize the staged update: {error}"))?;
        Ok(DownloadedUpdate {
            version: available.version.clone(),
            asset_name: manifest.asset,
            path: final_path,
            size: manifest.size,
            sha256: manifest.sha256.to_ascii_lowercase(),
        })
    }
}

fn state_json(state: &State) -> Value {
    let can_download =
        state.status == "available" && state.available.is_some() && embedded_update_key().is_ok();
    json!({
        "status": state.status,
        "currentVersion": crate::version::APPLICATION_VERSION,
        "availableVersion": state.available_version,
        "downloadedVersion": state.downloaded.as_ref().map(|value| value.version.clone()),
        "releaseUrl": state.release_url,
        "message": state.message,
        "lastCheckedAt": state.last_checked_at.map(|value| value.to_string()),
        "canCheck": !matches!(state.status, "checking" | "downloading" | "installing"),
        "canDownload": can_download,
        "canInstall": state.status == "downloaded" && state.downloaded.is_some(),
        "updateSource": "github-releases",
        "signaturePolicy": if embedded_update_key().is_ok() { "ed25519-pinned" } else { "unconfigured-fail-closed" }
    })
}

fn select_release<'a>(
    releases: &'a [Release],
    channel: &str,
    current: (u64, u64, u64),
) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|release| !release.draft && (channel == "nightly" || !release.prerelease))
        .filter_map(|release| parse_version(&release.tag_name).map(|version| (release, version)))
        .filter(|(_, version)| *version > current)
        .max_by_key(|(_, version)| *version)
        .map(|(release, _)| release)
}

fn compatible_asset(assets: &[Asset]) -> Option<&Asset> {
    let os_aliases: &[&str] = if cfg!(target_os = "windows") {
        &["win", "windows"]
    } else if cfg!(target_os = "macos") {
        &["mac", "macos", "darwin"]
    } else {
        &["linux"]
    };
    let arch_aliases: &[&str] = if cfg!(target_arch = "aarch64") {
        &["arm64", "aarch64"]
    } else {
        &["x64", "x86_64", "amd64"]
    };
    assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        let package = if cfg!(target_os = "windows") {
            name.ends_with(".msi") || name.ends_with(".exe")
        } else if cfg!(target_os = "macos") {
            name.ends_with(".dmg") || name.ends_with(".pkg")
        } else {
            name.ends_with(".appimage") || name.ends_with(".deb")
        };
        trusted_asset_url(asset)
            && safe_asset_name(&asset.name)
            && name.contains("qt")
            && os_aliases.iter().any(|alias| name.contains(alias))
            && arch_aliases.iter().any(|alias| name.contains(alias))
            && package
            && asset.size > 0
            && asset.size <= MAXIMUM_UPDATE_BYTES
    })
}

fn embedded_update_key() -> Result<VerifyingKey, String> {
    let encoded = option_env!("OPENNOW_UPDATE_ED25519_PUBLIC_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "This build has no pinned update signing key".to_owned())?;
    decode_verifying_key(encoded)
}

fn decode_verifying_key(encoded: &str) -> Result<VerifyingKey, String> {
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "Pinned update signing key is not valid base64".to_owned())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Pinned update signing key has the wrong length".to_owned())?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "Pinned update signing key is invalid".to_owned())
}

fn verify_manifest(manifest: &UpdateManifest, key: &VerifyingKey) -> Result<(), String> {
    if manifest.schema_version != 1
        || parse_version(&manifest.version).is_none()
        || !safe_asset_name(&manifest.asset)
        || manifest.size == 0
        || manifest.size > MAXIMUM_UPDATE_BYTES
        || manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err("Signed update metadata fields are invalid".to_owned());
    }
    let signature_bytes = BASE64
        .decode(&manifest.signature)
        .map_err(|_| "Update signature is not valid base64".to_owned())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "Update signature has the wrong length".to_owned())?;
    key.verify_strict(signature_payload(manifest).as_bytes(), &signature)
        .map_err(|_| "Update manifest signature is invalid".to_owned())
}

fn signature_payload(manifest: &UpdateManifest) -> String {
    format!(
        "OpenNOW update manifest v1\nversion={}\nasset={}\nsize={}\nsha256={}\n",
        manifest.version.trim_start_matches('v'),
        manifest.asset,
        manifest.size,
        manifest.sha256.to_ascii_lowercase()
    )
}

fn write_verified_asset(
    mut response: Response,
    path: &Path,
    manifest: &UpdateManifest,
) -> Result<(), String> {
    let mut output = fs::File::create(path)
        .map_err(|error| format!("Could not create the staged update: {error}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|_| "Update download ended unexpectedly".to_owned())?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| "Update size overflowed".to_owned())?;
        if total > manifest.size || total > MAXIMUM_UPDATE_BYTES {
            return Err("Update download exceeded its signed size".to_owned());
        }
        hasher.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(|error| format!("Could not write the staged update: {error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("Could not flush the staged update: {error}"))?;
    let digest = format!("{:x}", hasher.finalize());
    if total != manifest.size || !digest.eq_ignore_ascii_case(&manifest.sha256) {
        return Err("Update package hash or size did not match signed metadata".to_owned());
    }
    Ok(())
}

fn verify_downloaded_file(downloaded: &DownloadedUpdate) -> Result<(), String> {
    if !safe_asset_name(&downloaded.asset_name) || !downloaded.path.is_file() {
        return Err("The verified update package is no longer available".to_owned());
    }
    let metadata = fs::metadata(&downloaded.path)
        .map_err(|_| "The verified update package is unavailable".to_owned())?;
    if metadata.len() != downloaded.size {
        return Err("The staged update package changed after verification".to_owned());
    }
    let mut input = fs::File::open(&downloaded.path)
        .map_err(|_| "The staged update package could not be read".to_owned())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut input, &mut hasher)
        .map_err(|_| "The staged update package could not be verified".to_owned())?;
    if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&downloaded.sha256) {
        return Err("The staged update package changed after verification".to_owned());
    }
    Ok(())
}

fn launch_verified_update(path: &Path) -> Result<&'static str, String> {
    #[cfg(target_os = "windows")]
    {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let child = if extension.eq_ignore_ascii_case("msi") {
            Command::new("msiexec.exe")
                .arg("/i")
                .arg(path)
                .arg("/passive")
                .spawn()
        } else {
            Command::new(path).spawn()
        };
        child.map_err(|error| format!("Could not launch the verified installer: {error}"))?;
        Ok("installer")
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Could not open the verified update package: {error}"))?;
        Ok("package-opened")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("appimage"))
        {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
                format!("Could not mark the verified AppImage executable: {error}")
            })?;
            if let Some(current) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
                if current.is_absolute() && current.is_file() {
                    return replace_running_appimage(&current, path);
                }
            }
            Command::new(path)
                .spawn()
                .map_err(|error| format!("Could not launch the verified AppImage: {error}"))?;
            return Ok("appimage-launched");
        }
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Could not open the verified update package: {error}"))?;
        Ok("package-opened")
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn replace_running_appimage(current: &Path, staged: &Path) -> Result<&'static str, String> {
    use std::os::unix::fs::PermissionsExt as _;
    let parent = current
        .parent()
        .ok_or_else(|| "Running AppImage has no parent directory".to_owned())?;
    let name = current
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Running AppImage name is invalid".to_owned())?;
    let replacement = parent.join(format!(".{name}.new"));
    let backup = parent.join(format!(".{name}.previous"));
    fs::copy(staged, &replacement)
        .map_err(|error| format!("Could not stage the AppImage replacement: {error}"))?;
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("Could not set AppImage permissions: {error}"))?;
    let _ = fs::remove_file(&backup);
    fs::rename(current, &backup)
        .map_err(|error| format!("Could not preserve the previous AppImage: {error}"))?;
    if let Err(error) = fs::rename(&replacement, current) {
        let _ = fs::rename(&backup, current);
        return Err(format!("Could not install the AppImage update: {error}"));
    }
    if let Err(error) = Command::new(current).spawn() {
        let failed = parent.join(format!(".{name}.failed"));
        let _ = fs::rename(current, &failed);
        let _ = fs::rename(&backup, current);
        return Err(format!(
            "Updated AppImage could not restart; rolled back: {error}"
        ));
    }
    Ok("appimage-replaced")
}

fn ensure_asset_response(response: &Response, maximum: u64) -> Result<(), String> {
    if !response.status().is_success() {
        return Err(format!(
            "Update asset returned HTTP {}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err("Update asset exceeded its allowed size".to_owned());
    }
    Ok(())
}

fn read_bounded(mut response: Response, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read update metadata".to_owned())?;
    if bytes.len() as u64 > maximum {
        return Err("Update metadata exceeded the size limit".to_owned());
    }
    Ok(bytes)
}

fn trusted_asset_url(asset: &Asset) -> bool {
    asset.browser_download_url.starts_with(RELEASE_ASSET_PREFIX)
        && !asset.browser_download_url[RELEASE_ASSET_PREFIX.len()..].is_empty()
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
    let value = value.trim().trim_start_matches('v');
    let core = value.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

fn trusted_release_url(value: &str) -> bool {
    value
        .strip_prefix("https://github.com/OpenCloudGaming/OpenNOW/releases/")
        .is_some_and(|suffix| !suffix.is_empty())
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn friendly_network_error(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("connect") || lower.contains("dns") {
        "Unable to reach GitHub Releases right now.".to_owned()
    } else {
        "Update check failed.".to_owned()
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    fn release(version: &str, prerelease: bool) -> Release {
        Release {
            tag_name: version.to_owned(),
            html_url: format!("https://github.com/OpenCloudGaming/OpenNOW/releases/tag/{version}"),
            body: None,
            draft: false,
            prerelease,
            assets: vec![],
        }
    }

    #[test]
    fn versions_and_channels_are_selected_without_lexical_ordering() {
        let releases = vec![release("v0.9.0", false), release("v0.10.0-beta", true)];
        assert_eq!(
            select_release(&releases, "stable", (0, 8, 0))
                .unwrap()
                .tag_name,
            "v0.9.0"
        );
        assert_eq!(
            select_release(&releases, "nightly", (0, 8, 0))
                .unwrap()
                .tag_name,
            "v0.10.0-beta"
        );
        assert_eq!(parse_version("v10.2.3-beta.1"), Some((10, 2, 3)));
        assert!(select_release(&releases, "stable", (0, 9, 0)).is_none());
        assert!(select_release(&releases, "nightly", (0, 10, 0)).is_none());
    }

    #[test]
    fn update_urls_assets_and_names_are_strictly_scoped() {
        assert!(trusted_release_url(
            "https://github.com/OpenCloudGaming/OpenNOW/releases/tag/v1.0.0"
        ));
        assert!(!trusted_release_url("https://example.com/releases/tag/v1"));
        assert!(safe_asset_name("OpenNOW-Qt-linux-x64.AppImage"));
        assert!(!safe_asset_name("../OpenNOW.AppImage"));
        let malicious = Asset {
            name: "OpenNOW-Qt-linux-x64.AppImage".to_owned(),
            browser_download_url: "https://example.com/update".to_owned(),
            size: 1,
        };
        assert!(compatible_asset(&[malicious]).is_none());
    }

    #[test]
    fn signed_manifests_verify_canonical_fields_and_reject_tampering() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let mut manifest = UpdateManifest {
            schema_version: 1,
            version: "0.6.0".to_owned(),
            asset: "OpenNOW-Qt-linux-x64.AppImage".to_owned(),
            size: 123,
            sha256: "ab".repeat(32),
            signature: String::new(),
        };
        manifest.signature = BASE64.encode(
            signing
                .sign(signature_payload(&manifest).as_bytes())
                .to_bytes(),
        );
        let encoded_key = BASE64.encode(signing.verifying_key().as_bytes());
        let key = decode_verifying_key(&encoded_key).unwrap();
        verify_manifest(&manifest, &key).unwrap();
        manifest.size += 1;
        assert!(verify_manifest(&manifest, &key).is_err());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn failed_appimage_restart_restores_previous_binary() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "opennow-update-rollback-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let current = directory.join("OpenNOW.AppImage");
        let staged = directory.join("OpenNOW-new.AppImage");
        fs::write(&current, b"previous release").unwrap();
        fs::write(&staged, b"not an executable image").unwrap();
        fs::set_permissions(&current, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();

        let error = replace_running_appimage(&current, &staged).unwrap_err();
        assert!(error.contains("rolled back"));
        assert_eq!(fs::read(&current).unwrap(), b"previous release");
        assert_eq!(
            fs::read(directory.join(".OpenNOW.AppImage.failed")).unwrap(),
            b"not an executable image"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
