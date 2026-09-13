use crate::update_apply;
use crate::update_apply::verification::{
    MAXIMUM_MANIFEST_BYTES, MAXIMUM_UPDATE_BYTES, UpdateManifest, embedded_update_key,
    safe_asset_name, verify_manifest,
};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const RELEASES_URL: &str =
    "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases?per_page=20";
const RELEASES_PAGE: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases";
const RELEASE_ASSET_PREFIX: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases/download/";

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
    notes_version: Option<String>,
    notes: Option<String>,
    message: String,
    last_checked_at: Option<u128>,
    available: Option<AvailableUpdate>,
    downloaded: Option<DownloadedUpdate>,
    transaction: Option<update_apply::PreparedUpdate>,
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

pub struct UpdaterService {
    client: Client,
    staging_dir: PathBuf,
    data_dir: PathBuf,
    operation: Mutex<()>,
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
        let (transaction, status, message) =
            if let Some(message) = update_apply::external_update_message() {
                (None, "unsupported", message.to_owned())
            } else {
                match read_prepared_update(&staging_dir) {
                    Ok(transaction) => (
                        transaction,
                        "idle",
                        "Ready to check GitHub Releases".to_owned(),
                    ),
                    Err(error) => (
                        None,
                        "failed",
                        format!("Could not recover update status: {error}"),
                    ),
                }
            };
        Ok(Self {
            client,
            staging_dir,
            data_dir: data_dir.to_path_buf(),
            operation: Mutex::new(()),
            state: Mutex::new(State {
                status,
                available_version: None,
                release_url: None,
                notes_version: None,
                notes: None,
                message,
                last_checked_at: None,
                available: None,
                downloaded: None,
                transaction,
            }),
        })
    }

    pub fn state(&self) -> Value {
        let mut state = self.state.lock().expect("updater state poisoned");
        reconcile_transaction(&mut state, &self.staging_dir);
        state_json(&state)
    }

    pub fn installation_pending(&self) -> bool {
        self.state();
        matches!(
            self.state.lock().expect("updater state poisoned").status,
            "preparing"
                | "awaiting-exit"
                | "applying"
                | "restarting"
                | "installing"
                | "managed-pending"
        )
    }

    pub fn request_failed(&self, message: &str) {
        let mut state = self.state.lock().expect("updater state poisoned");
        if !matches!(
            state.status,
            "unsupported"
                | "checking"
                | "downloading"
                | "preparing"
                | "awaiting-exit"
                | "applying"
                | "restarting"
                | "installing"
        ) {
            state.message = message.to_owned();
        }
    }

    pub fn check(&self, params: &Value) -> Result<Value, String> {
        if update_apply::external_update_message().is_some() {
            return Ok(self.state());
        }
        let _operation = self.begin_operation()?;
        let channel = params["channel"]
            .as_str()
            .unwrap_or_else(|| crate::version::update_channel(crate::version::APPLICATION_VERSION));
        if !matches!(channel, "stable" | "nightly") {
            return Err("Unsupported update channel".to_owned());
        }
        {
            let mut state = self.state.lock().expect("updater state poisoned");
            state.status = "checking";
            state.transaction = None;
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
        // Reading release notes is independent of installing a newer version.
        // A development build can be ahead of every published release.
        update_highlights(&mut state, select_latest_release(&releases, channel));
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
            state.available = None;
            state.message = "OpenNOW is up to date.".to_owned();
        }
        restore_downloaded_status(&mut state);
        Ok(state_json(&state))
    }

    pub fn download(&self) -> Result<Value, String> {
        let _operation = self.begin_operation()?;
        let available = {
            let mut state = self.state.lock().expect("updater state poisoned");
            let available = state
                .available
                .clone()
                .ok_or_else(|| "No signed update package is available".to_owned())?;
            embedded_update_key()?;
            state.status = "downloading";
            state.transaction = None;
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
                Err(error)
            }
        }
    }

    pub fn install(&self, params: &Value) -> Result<Value, String> {
        let _operation = self.begin_operation()?;
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
        {
            let mut state = self.state.lock().expect("updater state poisoned");
            state.status = "preparing";
            state.message =
                "Verifying and preparing a complete replacement before shutdown.".to_owned();
            state.transaction = None;
        }
        let result = (|| {
            verify_downloaded_file(&downloaded)?;
            let application = std::env::var_os("OPENNOW_APP_EXECUTABLE")
                .map(PathBuf::from)
                .ok_or("The Qt application executable identity is unavailable")?;
            let application_pid = std::env::var("OPENNOW_APP_PID")
                .map_err(|_| "The Qt application process identity is unavailable")?
                .parse::<u32>()
                .map_err(|_| "The Qt application process identity is invalid")?;
            let kind = update_apply::detect_install_kind(&application, &downloaded.path)?;
            let prepared = update_apply::prepare_update(update_apply::PrepareRequest {
                package: downloaded.path.clone(),
                expected_version: downloaded.version.clone(),
                application_executable: application,
                application_pid,
                core_pid: std::process::id(),
                kind,
                data_dir: self.data_dir.clone(),
            })?;
            save_prepared_update(&self.staging_dir, &prepared)?;
            self.state
                .lock()
                .expect("updater state poisoned")
                .transaction = Some(prepared.clone());
            update_apply::launch_prepared_update(&prepared)?;
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            let mut state = self.state.lock().expect("updater state poisoned");
            state.status = "failed";
            state.message = format!("Update preparation failed; OpenNOW remains open: {error}");
            state.transaction = None;
            return Err(error);
        }
        Ok(self.state())
    }

    pub fn highlights(&self) -> Value {
        let state = self.state.lock().expect("updater state poisoned");
        json!({
            "version": state.notes_version,
            "title": state.notes_version.as_ref().map(|version| format!("OpenNOW {version}")),
            "bodyMarkdown": state.notes,
            "releaseUrl": state.release_url
        })
    }

    fn begin_operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        if let Some(message) = update_apply::external_update_message() {
            return Err(message.to_owned());
        }
        let operation = self
            .operation
            .try_lock()
            .map_err(|_| "An update operation is already in progress".to_owned())?;
        if self.installation_pending() {
            return Err("Update installation is already in progress".to_owned());
        }
        if self.state.lock().expect("updater state poisoned").status == "reboot-required" {
            return Err("Restart your system before starting another update operation.".to_owned());
        }
        Ok(operation)
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
        if !manifest_matches_release(&manifest, available) {
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
        fs::write(
            self.staging_dir
                .join(format!("{}.manifest.json", manifest.asset)),
            manifest_bytes,
        )
        .map_err(|error| format!("Could not preserve signed update metadata: {error}"))?;
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
    let can_download = matches!(state.status, "available" | "error" | "downloaded")
        && state.available.is_some()
        && embedded_update_key().is_ok()
        && state.available.as_ref().is_some_and(|available| {
            state
                .downloaded
                .as_ref()
                .is_none_or(|downloaded| downloaded.asset_name != available.asset.name)
        });
    json!({
        "status": state.status,
        "currentVersion": crate::version::APPLICATION_VERSION,
        "availableVersion": state.available_version,
        "downloadedVersion": state.downloaded.as_ref().map(|value| value.version.clone()),
        "releaseUrl": state.release_url,
        "message": state.message,
        "lastCheckedAt": state.last_checked_at.map(|value| value.to_string()),
        "canCheck": !matches!(state.status, "unsupported" | "checking" | "downloading" | "preparing" | "awaiting-exit" | "applying" | "restarting" | "installing" | "managed-pending" | "reboot-required"),
        "canDownload": can_download,
        "canInstall": matches!(state.status, "downloaded" | "error" | "available" | "not-available" | "failed" | "rolled-back") && state.downloaded.is_some(),
        "exitRequired": state.status == "awaiting-exit",
        "installVersion": state.transaction.as_ref().map(|transaction| &transaction.version),
        "updateSource": if state.status == "unsupported" { "flatpak" } else { "github-releases" },
        "signaturePolicy": if embedded_update_key().is_ok() { "ed25519-pinned" } else { "unconfigured-fail-closed" }
    })
}

fn read_prepared_update(directory: &Path) -> Result<Option<update_apply::PreparedUpdate>, String> {
    let path = directory.join("active-apply.json");
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read update transaction: {error}")),
    };
    let bytes = read_bounded(file, MAXIMUM_MANIFEST_BYTES)?;
    let prepared: update_apply::PreparedUpdate =
        serde_json::from_slice(&bytes).map_err(|_| "Persisted update transaction is malformed")?;
    if !prepared.plan_path.is_absolute()
        || !prepared.outcome_path.is_absolute()
        || prepared
            .plan_path
            .file_name()
            .is_none_or(|name| name != "plan.json")
        || prepared
            .outcome_path
            .file_name()
            .is_none_or(|name| name != "outcome.json")
        || prepared.plan_path.parent() != prepared.outcome_path.parent()
        || parse_version(&prepared.version).is_none()
    {
        return Err("Persisted update transaction paths or version are invalid".to_owned());
    }
    Ok(Some(prepared))
}

fn save_prepared_update(
    directory: &Path,
    prepared: &update_apply::PreparedUpdate,
) -> Result<(), String> {
    let temporary = directory.join(format!(".active-apply-{:016x}.part", rand::random::<u64>()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(prepared).map_err(|error| error.to_string())?)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        let destination = directory.join("active-apply.json");
        match fs::remove_file(&destination) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
        fs::rename(&temporary, destination).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn reconcile_transaction(state: &mut State, staging_dir: &Path) {
    let Some(transaction) = &state.transaction else {
        return;
    };
    let outcome = update_apply::read_outcome(&transaction.outcome_path);
    let outcome = match outcome {
        Ok(Some(outcome)) if outcome.version == transaction.version => outcome,
        Ok(None) | Ok(Some(_)) => {
            state.status = "failed";
            state.message =
                "The update helper outcome is missing or does not match the prepared version."
                    .to_owned();
            return;
        }
        Err(error) => {
            state.status = "failed";
            state.message = error;
            return;
        }
    };
    use update_apply::OutcomeStatus;
    if matches!(
        outcome.status,
        OutcomeStatus::ManagedPending | OutcomeStatus::RebootRequired
    ) {
        match update_apply::recover_managed_update(
            transaction,
            &outcome,
            crate::version::APPLICATION_VERSION,
        ) {
            Ok(update_apply::ManagedRecovery::Pending(message)) => {
                state.status = "managed-pending";
                state.message = message;
                return;
            }
            Ok(update_apply::ManagedRecovery::RebootRequired(message)) => {
                state.status = "reboot-required";
                state.message = message;
                return;
            }
            Ok(update_apply::ManagedRecovery::Finished(result)) => {
                state.status = if result.status == OutcomeStatus::Completed {
                    "succeeded"
                } else {
                    "failed"
                };
                state.message = result.message;
            }
            Err(error) => {
                state.status = "failed";
                state.message = format!("Could not recover native update completion: {error}");
            }
        }
        state.transaction = None;
        match fs::remove_file(staging_dir.join("active-apply.json")) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => state.message.push_str(&format!(
                "; could not remove the completed update transaction: {error}"
            )),
        }
        return;
    }
    if matches!(
        outcome.status,
        OutcomeStatus::WaitingForExit
            | OutcomeStatus::BackingUp
            | OutcomeStatus::Installing
            | OutcomeStatus::AwaitingStartup
    ) || (outcome.status == OutcomeStatus::Prepared && state.status != "preparing")
    {
        match update_apply::helper_is_running(transaction) {
            Ok(true) => (),
            Ok(false) => {
                state.status = "failed";
                state.message = "The update helper stopped before completing the transaction. OpenNOW will not close automatically.".to_owned();
                return;
            }
            Err(error) => {
                state.status = "failed";
                state.message = format!("Could not confirm update helper ownership: {error}");
                return;
            }
        }
    }
    state.status = match outcome.status {
        OutcomeStatus::Prepared => "preparing",
        OutcomeStatus::WaitingForExit => "awaiting-exit",
        OutcomeStatus::BackingUp | OutcomeStatus::Installing => "applying",
        OutcomeStatus::AwaitingStartup => "restarting",
        OutcomeStatus::Completed => "succeeded",
        OutcomeStatus::RolledBack => "rolled-back",
        OutcomeStatus::Failed => "failed",
        OutcomeStatus::ManagedPending => "managed-pending",
        OutcomeStatus::RebootRequired => "reboot-required",
    };
    state.message = outcome.message;
}

fn restore_downloaded_status(state: &mut State) {
    if let Some(downloaded) = &state.downloaded {
        state.status = "downloaded";
        state.message = format!(
            "OpenNOW {} remains downloaded and verified. Ready to install.",
            downloaded.version
        );
    }
}

fn select_release<'a>(
    releases: &'a [Release],
    channel: &str,
    current: Version,
) -> Option<&'a Release> {
    select_latest_release(releases, channel).filter(|release| {
        parse_version(&release.tag_name)
            .is_some_and(|version| version.cmp_precedence(&current).is_gt())
    })
}

fn select_latest_release<'a>(releases: &'a [Release], channel: &str) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|release| !release.draft && (channel == "nightly" || !release.prerelease))
        .filter_map(|release| parse_version(&release.tag_name).map(|version| (release, version)))
        .filter(|(_, version)| channel == "nightly" || version.pre.is_empty())
        .max_by(|(_, left), (_, right)| left.cmp_precedence(right))
        .map(|(release, _)| release)
}

fn update_highlights(state: &mut State, release: Option<&Release>) {
    state.notes_version =
        release.map(|release| release.tag_name.trim_start_matches('v').to_owned());
    state.release_url = Some(
        release
            .filter(|release| trusted_release_url(&release.html_url))
            .map_or_else(
                || RELEASES_PAGE.to_owned(),
                |release| release.html_url.clone(),
            ),
    );
    state.notes = Some(match release {
        Some(release) => release
            .body
            .as_deref()
            .map(str::trim)
            .filter(|body| !body.is_empty())
            .map(|body| bounded(body, 20_000))
            .unwrap_or_else(|| "No release notes were published for this release.".to_owned()),
        None => "No published releases were found for this update channel.".to_owned(),
    });
}

fn compatible_asset(assets: &[Asset]) -> Option<&Asset> {
    let extension = update_apply::compatible_package_extension().ok()?;
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
        let package = name
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| suffix == extension);
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

fn manifest_matches_release(manifest: &UpdateManifest, available: &AvailableUpdate) -> bool {
    parse_version(&manifest.version)
        .is_some_and(|version| Some(version) == parse_version(&available.version))
        && manifest.asset == available.asset.name
        && manifest.size == available.asset.size
        && manifest.size > 0
        && manifest.size <= MAXIMUM_UPDATE_BYTES
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
    let manifest = fs::File::open(
        downloaded
            .path
            .with_file_name(format!("{}.manifest.json", downloaded.asset_name)),
    )
    .map_err(|_| "The staged update manifest could not be read".to_owned())?;
    let manifest = update_apply::verification::verify_signed_manifest(&read_bounded(
        manifest,
        MAXIMUM_MANIFEST_BYTES,
    )?)?;
    if manifest.version.trim_start_matches('v') != downloaded.version
        || manifest.asset != downloaded.asset_name
        || manifest.size != downloaded.size
        || !manifest.sha256.eq_ignore_ascii_case(&downloaded.sha256)
    {
        return Err("The staged update package changed after verification".to_owned());
    }
    update_apply::verification::verify_package(&downloaded.path, &manifest)
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

fn read_bounded(mut response: impl Read, maximum: u64) -> Result<Vec<u8>, String> {
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

fn parse_version(value: &str) -> Option<Version> {
    Version::parse(value.strip_prefix('v').unwrap_or(value)).ok()
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
    use crate::update_apply::verification::{decode_verifying_key, signature_payload};
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use ed25519_dalek::{Signer as _, SigningKey};

    #[cfg(target_os = "linux")]
    #[test]
    fn flatpak_updates_are_external_even_with_cached_native_updates() {
        if std::env::var_os("OPENNOW_FLATPAK_UPDATER_TEST").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "updater::tests::flatpak_updates_are_external_even_with_cached_native_updates",
                    "--nocapture",
                ])
                .env("OPENNOW_FLATPAK_UPDATER_TEST", "1")
                .env("FLATPAK_ID", "io.github.opencloudgaming.OpenNOW")
                .env("APPIMAGE", "/host/OpenNOW.AppImage")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let updates = directory.path().join("updates");
        fs::create_dir(&updates).unwrap();
        let persisted = updates.join("active-apply.json");
        fs::write(&persisted, b"partial host update transaction").unwrap();
        let updater = UpdaterService::new(directory.path()).unwrap();
        let message = update_apply::external_update_message().unwrap();
        assert!(message.contains("flatpak update io.github.opencloudgaming.OpenNOW"));
        assert!(!updater.installation_pending());
        for state in [updater.state(), updater.check(&json!({})).unwrap()] {
            assert_eq!(state["status"], "unsupported");
            assert_eq!(state["updateSource"], "flatpak");
            assert_eq!(state["message"], message);
            for capability in ["canCheck", "canDownload", "canInstall", "exitRequired"] {
                assert_eq!(state[capability], false);
            }
            assert!(state["installVersion"].is_null());
        }
        updater.request_failed("A generic failure must not replace the update instruction");
        assert_eq!(updater.state()["message"], message);
        assert_eq!(updater.download().unwrap_err(), message);
        assert_eq!(
            updater.install(&json!({"confirmed": true})).unwrap_err(),
            message
        );
        assert_eq!(
            fs::read(&persisted).unwrap(),
            b"partial host update transaction"
        );
        assert_eq!(fs::read_dir(&updates).unwrap().count(), 1);
        assert_eq!(
            update_apply::compatible_package_extension().unwrap_err(),
            message
        );
        for (name, kind) in [
            ("OpenNOW.AppImage", update_apply::InstallKind::AppImage),
            ("OpenNOW.deb", update_apply::InstallKind::DebianPackage),
        ] {
            let package = directory.path().join(name);
            let application = directory.path().join("OpenNOW");
            assert_eq!(
                update_apply::detect_install_kind(&application, &package).unwrap_err(),
                message
            );
            assert_eq!(
                update_apply::prepare_update(update_apply::PrepareRequest {
                    package,
                    expected_version: "1.2.3".to_owned(),
                    application_executable: application,
                    application_pid: std::process::id(),
                    core_pid: std::process::id(),
                    kind,
                    data_dir: directory.path().to_path_buf(),
                })
                .unwrap_err(),
                message
            );
        }
        let prepared = update_apply::PreparedUpdate {
            plan_path: directory.path().join("plan.json"),
            outcome_path: directory.path().join("outcome.json"),
            version: "1.2.3".to_owned(),
        };
        assert_eq!(
            update_apply::launch_prepared_update(&prepared).unwrap_err(),
            message
        );
        assert_eq!(
            update_apply::run_helper(&prepared.plan_path).unwrap_err(),
            message
        );
        assert!(!directory.path().join("apply.lock").exists());
    }

    fn persisted_managed_fixture(
        directory: &Path,
        status: update_apply::OutcomeStatus,
        installer: Option<update_apply::ProcessIdentity>,
    ) -> update_apply::PreparedUpdate {
        let updates = directory.join("updates");
        fs::create_dir_all(&updates).unwrap();
        let prepared = update_apply::PreparedUpdate {
            plan_path: directory.join("plan.json"),
            outcome_path: directory.join("outcome.json"),
            version: "1.1.0".to_owned(),
        };
        save_prepared_update(&updates, &prepared).unwrap();
        fs::write(&prepared.plan_path, serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "version": prepared.version,
            "kind": if status == update_apply::OutcomeStatus::RebootRequired || cfg!(windows) { "windowsMsi" } else { "debianPackage" },
            "package": directory.join("update-package"),
            "target": directory.join("installed"),
            "applicationExecutable": directory.join("installed/OpenNOW"),
            "dataDir": directory.join("data"),
            "processes": [],
            "nonce": "a".repeat(64),
            "managedIdentity": {
                "package": if cfg!(windows) { "{00000000-0000-0000-0000-000000000000}" } else { "opennow-recovery-test-missing" },
                "version": "1.1.0", "architecture": "amd64", "installed_product": "opennow-recovery-test-missing"
            }
        })).unwrap()).unwrap();
        fs::write(
            &prepared.outcome_path,
            serde_json::to_vec(&update_apply::UpdateOutcome {
                schema_version: 1,
                version: prepared.version.clone(),
                status,
                message: "Native installation completion is pending".to_owned(),
                installed_version: None,
                restarted_process: installer,
            })
            .unwrap(),
        )
        .unwrap();
        prepared
    }

    #[test]
    fn a_live_native_installer_keeps_persisted_updates_blocking() {
        for status in [
            update_apply::OutcomeStatus::ManagedPending,
            update_apply::OutcomeStatus::RebootRequired,
        ] {
            let directory = tempfile::tempdir().unwrap();
            persisted_managed_fixture(
                directory.path(),
                status,
                Some(update_apply::ProcessIdentity::capture(std::process::id()).unwrap()),
            );
            let updater = UpdaterService::new(directory.path()).unwrap();
            assert_eq!(updater.state()["status"], "managed-pending");
            assert_eq!(updater.state()["canCheck"], false);
            assert!(updater.installation_pending());
            assert!(directory.path().join("updates/active-apply.json").exists());
        }
    }

    #[test]
    fn a_live_helper_keeps_native_recovery_blocking() {
        use fs2::FileExt;
        let directory = tempfile::tempdir().unwrap();
        let mut installer = update_apply::ProcessIdentity::capture(std::process::id()).unwrap();
        installer.started = installer.started.wrapping_add(1);
        persisted_managed_fixture(
            directory.path(),
            update_apply::OutcomeStatus::ManagedPending,
            Some(installer),
        );
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(directory.path().join("apply.lock"))
            .unwrap();
        lock.try_lock_exclusive().unwrap();
        let updater = UpdaterService::new(directory.path()).unwrap();
        assert!(updater.installation_pending());
        assert_eq!(updater.state()["canCheck"], false);
        drop(lock);
        assert!(!updater.installation_pending());
        assert_eq!(updater.state()["canCheck"], true);
    }

    #[test]
    fn finished_native_installer_verification_failure_unblocks_and_clears_persistence() {
        let directory = tempfile::tempdir().unwrap();
        let mut installer = update_apply::ProcessIdentity::capture(std::process::id()).unwrap();
        installer.started = installer.started.wrapping_add(1);
        let prepared = persisted_managed_fixture(
            directory.path(),
            update_apply::OutcomeStatus::ManagedPending,
            Some(installer),
        );
        let updater = UpdaterService::new(directory.path()).unwrap();
        assert_eq!(updater.state()["status"], "failed");
        assert_eq!(updater.state()["canCheck"], true);
        assert!(!updater.installation_pending());
        assert!(updater.begin_operation().is_ok());
        assert!(!directory.path().join("updates/active-apply.json").exists());
        assert_eq!(
            update_apply::read_outcome(&prepared.outcome_path)
                .unwrap()
                .unwrap()
                .status,
            update_apply::OutcomeStatus::Failed
        );
        let restarted = UpdaterService::new(directory.path()).unwrap();
        assert!(!restarted.installation_pending());
        assert_eq!(restarted.state()["canCheck"], true);
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn missing_native_installer_identity_requires_a_proven_reboot_before_recovery() {
        let directory = tempfile::tempdir().unwrap();
        persisted_managed_fixture(
            directory.path(),
            update_apply::OutcomeStatus::ManagedPending,
            None,
        );
        let updater = UpdaterService::new(directory.path()).unwrap();
        assert!(updater.installation_pending());
        assert_eq!(updater.state()["canCheck"], false);
        assert!(
            updater.state()["message"]
                .as_str()
                .unwrap()
                .contains("Restart your system")
        );
        assert!(directory.path().join("updates/active-apply.json").exists());
        assert!(directory.path().join("install-boot.json").exists());
        fs::write(
            directory.path().join("install-boot.json"),
            br#"{"schemaVersion":1,"bootId":"previous-boot"}"#,
        )
        .unwrap();
        let restarted = UpdaterService::new(directory.path()).unwrap();
        assert!(!restarted.installation_pending());
        assert_eq!(restarted.state()["status"], "failed");
        assert_eq!(restarted.state()["canCheck"], true);
        assert!(!directory.path().join("updates/active-apply.json").exists());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn reboot_warning_allows_sessions_but_blocks_update_operations_until_boot_change() {
        let directory = tempfile::tempdir().unwrap();
        persisted_managed_fixture(
            directory.path(),
            update_apply::OutcomeStatus::RebootRequired,
            None,
        );
        let updater = UpdaterService::new(directory.path()).unwrap();
        assert_eq!(updater.state()["status"], "reboot-required");
        assert_eq!(updater.state()["canCheck"], false);
        assert!(!updater.installation_pending());
        assert!(
            updater
                .begin_operation()
                .unwrap_err()
                .contains("Restart your system")
        );
        assert!(directory.path().join("updates/active-apply.json").exists());
        fs::write(
            directory.path().join("install-boot.json"),
            br#"{"schemaVersion":1,"bootId":"previous-boot"}"#,
        )
        .unwrap();
        let restarted = UpdaterService::new(directory.path()).unwrap();
        assert_eq!(restarted.state()["status"], "failed");
        assert!(!restarted.installation_pending());
        assert_eq!(restarted.state()["canCheck"], true);
        assert!(!directory.path().join("updates/active-apply.json").exists());
    }

    #[test]
    fn persisted_apply_outcomes_require_a_live_helper_before_quit() {
        let directory = tempfile::tempdir().unwrap();
        let updates = directory.path().join("updates");
        fs::create_dir(&updates).unwrap();
        let prepared = update_apply::PreparedUpdate {
            plan_path: directory.path().join("plan.json"),
            outcome_path: directory.path().join("outcome.json"),
            version: "1.1.0".to_owned(),
        };
        save_prepared_update(&updates, &prepared).unwrap();
        assert_eq!(
            read_prepared_update(&updates).unwrap().unwrap().version,
            "1.1.0"
        );
        for status in [
            update_apply::OutcomeStatus::Prepared,
            update_apply::OutcomeStatus::WaitingForExit,
            update_apply::OutcomeStatus::BackingUp,
            update_apply::OutcomeStatus::Installing,
            update_apply::OutcomeStatus::AwaitingStartup,
        ] {
            fs::write(
                &prepared.outcome_path,
                serde_json::to_vec(&update_apply::UpdateOutcome {
                    schema_version: 1,
                    version: prepared.version.clone(),
                    status,
                    message: "A stale helper snapshot".to_owned(),
                    installed_version: None,
                    restarted_process: None,
                })
                .unwrap(),
            )
            .unwrap();
            let updater = UpdaterService::new(directory.path()).unwrap();
            assert_eq!(updater.state()["status"], "failed");
            assert_eq!(updater.state()["exitRequired"], false);
            assert!(!updater.installation_pending());
        }
    }

    #[test]
    fn corrupt_update_status_does_not_prevent_core_startup() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("updates")).unwrap();
        fs::write(
            directory.path().join("updates/active-apply.json"),
            b"partial JSON",
        )
        .unwrap();
        let updater = UpdaterService::new(directory.path()).unwrap();
        assert_eq!(updater.state()["status"], "failed");
        assert_eq!(updater.state()["exitRequired"], false);
        assert_eq!(updater.state()["canCheck"], true);
    }

    #[test]
    fn checks_and_errors_preserve_verified_install_capability() {
        let mut state = notes_state();
        state.downloaded = Some(DownloadedUpdate {
            version: "1.1.0".to_owned(),
            asset_name: "OpenNOW-Qt-linux-x64.AppImage".to_owned(),
            path: PathBuf::from("/not-read-by-state"),
            size: 123,
            sha256: "ab".repeat(32),
        });
        for status in ["available", "not-available", "error"] {
            state.status = status;
            assert_eq!(state_json(&state)["canInstall"], true);
            restore_downloaded_status(&mut state);
            assert_eq!(state.status, "downloaded");
            assert_eq!(state_json(&state)["downloadedVersion"], "1.1.0");
        }
        for status in ["checking", "downloading", "installing"] {
            state.status = status;
            assert_eq!(state_json(&state)["canInstall"], false);
        }
    }

    #[test]
    fn install_requires_consent_before_accessing_packages() {
        let path =
            std::env::temp_dir().join(format!("opennow-update-consent-{}", rand::random::<u64>()));
        let updater = UpdaterService::new(&path).unwrap();
        for params in [
            json!({}),
            json!({"confirmed":false}),
            json!({"confirmed":"true"}),
        ] {
            assert_eq!(
                updater.install(&params).unwrap_err(),
                "Update installation requires explicit confirmation"
            );
        }
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn overlapping_update_operations_are_rejected_without_mutating_state() {
        let path =
            std::env::temp_dir().join(format!("opennow-update-busy-{}", rand::random::<u64>()));
        let updater = UpdaterService::new(&path).unwrap();
        let operation = updater.begin_operation().unwrap();
        let before = updater.state();
        for result in [
            updater.check(&json!({"channel":"invalid"})),
            updater.download(),
            updater.install(&json!({"confirmed":true})),
        ] {
            assert_eq!(
                result.unwrap_err(),
                "An update operation is already in progress"
            );
            assert_eq!(updater.state(), before);
        }
        drop(operation);
        assert!(updater.begin_operation().is_ok());
        updater.state.lock().unwrap().status = "installing";
        for result in [
            updater.check(&json!({"channel":"invalid"})),
            updater.download(),
            updater.install(&json!({"confirmed":true})),
        ] {
            assert_eq!(
                result.unwrap_err(),
                "Update installation is already in progress"
            );
        }
        std::fs::remove_dir_all(path).unwrap();
    }

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
            select_release(&releases, "stable", Version::new(0, 8, 0))
                .unwrap()
                .tag_name,
            "v0.9.0"
        );
        assert_eq!(
            select_release(&releases, "nightly", Version::new(0, 8, 0))
                .unwrap()
                .tag_name,
            "v0.10.0-beta"
        );
        assert_eq!(
            parse_version("v10.2.3-beta.1"),
            Some(Version::parse("10.2.3-beta.1").unwrap())
        );
        assert!(select_release(&releases, "stable", Version::new(0, 9, 0)).is_none());
        assert!(select_release(&releases, "nightly", Version::new(0, 10, 0)).is_none());
    }

    fn notes_state() -> State {
        State {
            status: "not-available",
            available_version: None,
            release_url: None,
            notes_version: None,
            notes: None,
            message: String::new(),
            last_checked_at: None,
            available: None,
            downloaded: None,
            transaction: None,
        }
    }

    #[test]
    fn nightly_runs_and_attempts_are_ordered_numerically() {
        let releases = vec![
            release("v1.0.0-nightly.10.2", true),
            release("v1.0.0-nightly.9.10", true),
            release("v1.0.0-nightly.10.10", true),
        ];
        for current in ["1.0.0-nightly.9.10", "1.0.0-nightly.10.2"] {
            assert_eq!(
                select_release(&releases, "nightly", parse_version(current).unwrap())
                    .unwrap()
                    .tag_name,
                "v1.0.0-nightly.10.10"
            );
        }
        for current in ["1.0.0-nightly.10.10", "1.0.0-nightly.11.1", "1.0.0"] {
            assert!(
                select_release(&releases, "nightly", parse_version(current).unwrap()).is_none()
            );
        }
    }

    #[test]
    fn stable_promotes_the_same_base_nightly_on_both_channels() {
        let releases = vec![
            release("v1.0.0", false),
            release("v1.0.0-nightly.999.1", true),
        ];
        for channel in ["stable", "nightly"] {
            assert_eq!(
                select_release(
                    &releases,
                    channel,
                    parse_version("1.0.0-nightly.999.1").unwrap()
                )
                .unwrap()
                .tag_name,
                "v1.0.0"
            );
            assert!(select_release(&releases, channel, Version::new(1, 0, 0)).is_none());
        }
        let mislabeled = [release("v1.1.0-nightly.1.1", false)];
        assert!(select_latest_release(&mislabeled, "stable").is_none());
    }

    #[test]
    fn build_metadata_does_not_offer_an_update() {
        let releases = [release("v1.0.0-nightly.10.1+z", true)];
        assert!(
            select_release(
                &releases,
                "nightly",
                parse_version("1.0.0-nightly.10.1+a").unwrap()
            )
            .is_none()
        );
    }

    #[test]
    fn invalid_semver_suffixes_and_prefixes_are_rejected() {
        for version in [
            "1.0.0-nightly.01.1",
            "1.0.0-nightly..1",
            "1.0.0-",
            "1.0.0+",
            "01.0.0",
            "vv1.0.0",
            "1.0.0\n",
        ] {
            assert!(parse_version(version).is_none(), "{version:?}");
        }
    }

    #[test]
    fn manifest_identity_preserves_prerelease_and_build_metadata() {
        let available = AvailableUpdate {
            version: "1.0.0-nightly.10.2+build".to_owned(),
            asset: Asset {
                name: "OpenNOW-Qt-1.0.0-nightly.10.2-Linux-x64.deb".to_owned(),
                browser_download_url: String::new(),
                size: 123,
            },
            manifest_url: String::new(),
        };
        let mut manifest = UpdateManifest {
            schema_version: 1,
            version: format!("v{}", available.version),
            asset: available.asset.name.clone(),
            size: available.asset.size,
            sha256: "ab".repeat(32),
            signature: String::new(),
        };
        assert!(manifest_matches_release(&manifest, &available));
        for version in [
            "1.0.0-nightly.10.1+build",
            "1.0.0-nightly.10.2+other",
            "1.0.0-nightly.10.2",
            "1.0.0",
            "invalid",
        ] {
            manifest.version = version.to_owned();
            assert!(
                !manifest_matches_release(&manifest, &available),
                "{version}"
            );
        }
    }

    #[test]
    fn nightly_manifests_require_valid_signatures() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let mut manifest = UpdateManifest {
            schema_version: 1,
            version: "1.0.0-nightly.10.2".to_owned(),
            asset: "OpenNOW-Qt-1.0.0-nightly.10.2-Linux-x64.deb".to_owned(),
            size: 123,
            sha256: "ab".repeat(32),
            signature: String::new(),
        };
        let key = signing.verifying_key();
        assert!(verify_manifest(&manifest, &key).is_err());
        manifest.signature = BASE64.encode(
            signing
                .sign(signature_payload(&manifest).as_bytes())
                .to_bytes(),
        );
        verify_manifest(&manifest, &key).unwrap();
        let original_version = manifest.version.clone();
        for version in ["1.0.0-nightly.10.3", "1.0.0"] {
            manifest.version = version.to_owned();
            assert!(verify_manifest(&manifest, &key).is_err());
        }
        manifest.version = original_version;
        assert!(
            verify_manifest(
                &manifest,
                &SigningKey::from_bytes(&[8_u8; 32]).verifying_key()
            )
            .is_err()
        );
    }

    #[test]
    fn builds_without_a_pinned_key_cannot_download_updates() {
        if option_env!("OPENNOW_UPDATE_ED25519_PUBLIC_KEY")
            .is_some_and(|value| !value.trim().is_empty())
        {
            return;
        }
        let path =
            std::env::temp_dir().join(format!("opennow-update-no-key-{}", rand::random::<u64>()));
        let updater = UpdaterService::new(&path).unwrap();
        updater.state.lock().unwrap().available = Some(AvailableUpdate {
            version: "1.0.0-nightly.10.2".to_owned(),
            asset: Asset {
                name: "OpenNOW-Qt-1.0.0-nightly.10.2-Linux-x64.deb".to_owned(),
                browser_download_url: String::new(),
                size: 123,
            },
            manifest_url: String::new(),
        });
        updater.state.lock().unwrap().status = "available";
        assert_eq!(updater.state()["canDownload"], false);
        assert_eq!(
            updater.state()["signaturePolicy"],
            "unconfigured-fail-closed"
        );
        assert_eq!(
            updater.download().unwrap_err(),
            "This build has no pinned update signing key"
        );
        assert!(updater.state()["downloadedVersion"].is_null());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nightly_debian_package_matches_the_platform() {
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        let name = format!("OpenNOW-Qt-1.0.0-nightly.10.2-Linux-{arch}.deb");
        let assets = [Asset {
            browser_download_url: format!("{RELEASE_ASSET_PREFIX}v1.0.0-nightly.10.2/{name}"),
            name,
            size: 123,
        }];
        assert!(compatible_asset(&assets).is_some());
    }

    #[test]
    fn current_and_ahead_builds_keep_published_notes_without_offering_a_downgrade() {
        let mut published = release("v0.5.4", false);
        published.body = Some("# Changes\n\n- **Fixed** release notes".to_owned());
        let releases = vec![published];
        for current in [Version::new(0, 5, 4), Version::new(1, 0, 0)] {
            assert!(select_release(&releases, "stable", current).is_none());
            let mut state = notes_state();
            update_highlights(&mut state, select_latest_release(&releases, "stable"));
            assert_eq!(state.notes_version.as_deref(), Some("0.5.4"));
            assert_eq!(state.notes, releases[0].body);
            assert_eq!(
                state.release_url.as_deref(),
                Some(releases[0].html_url.as_str())
            );
            assert!(state_json(&state)["availableVersion"].is_null());
            assert_eq!(state_json(&state)["canDownload"], false);
        }
    }

    #[test]
    fn notes_follow_channel_and_ignore_drafts_and_invalid_versions() {
        let mut draft = release("v99.0.0", false);
        draft.draft = true;
        let releases = vec![
            release("v1.0.0", false),
            release("v1.1.0-beta", true),
            draft,
            release("not-a-version", false),
        ];
        let mut state = notes_state();
        update_highlights(&mut state, select_latest_release(&releases, "nightly"));
        assert_eq!(state.notes_version.as_deref(), Some("1.1.0-beta"));
        update_highlights(&mut state, select_latest_release(&releases, "stable"));
        assert_eq!(state.notes_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            select_release(&releases, "stable", Version::new(0, 9, 0))
                .unwrap()
                .tag_name,
            "v1.0.0"
        );
    }

    #[test]
    fn missing_bodies_and_empty_channels_replace_stale_notes_with_feedback() {
        let mut state = notes_state();
        for body in [None, Some(" \n\t".to_owned())] {
            let mut published = release("v1.0.0", false);
            published.body = body;
            update_highlights(&mut state, Some(&published));
            assert_eq!(
                state.notes.as_deref(),
                Some("No release notes were published for this release.")
            );
        }
        update_highlights(&mut state, None);
        assert!(state.notes_version.is_none());
        assert_eq!(
            state.notes.as_deref(),
            Some("No published releases were found for this update channel.")
        );
        assert_eq!(state.release_url.as_deref(), Some(RELEASES_PAGE));
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
}
