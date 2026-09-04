#![recursion_limit = "512"]

mod account_connections;
mod artwork_cache;
mod cloudmatch;
mod community;
mod console_profiles;
mod credential_vault;
mod diagnostics;
mod discord;
mod gfn;
mod media;
mod network;
mod persistent_storage;
mod proxy;
mod settings;
mod streamer;
mod telemetry;
mod thanks;
mod updater;
mod version;

use gfn::GfnService;
use rand::RngCore;
use serde_json::{Value, json};
use settings::{SettingsStore, resolve_data_dir};
use std::collections::HashSet;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use streamer::StreamerService;

const PROTOCOL_VERSION: i64 = 1;
const MAXIMUM_LINE_BYTES: usize = 1024 * 1024;
const MAXIMUM_CONCURRENT_REQUESTS: usize = 8;

struct AppCore {
    artwork: artwork_cache::ArtworkCache,
    settings: Mutex<SettingsStore>,
    gfn: GfnService,
    streamer: StreamerService,
    diagnostics: diagnostics::DiagnosticsService,
    media: media::MediaService,
    updater: updater::UpdaterService,
    community: community::CommunityService,
    thanks: thanks::ThanksService,
    discord: discord::DiscordService,
    telemetry: telemetry::TelemetryService,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("opennow-core: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let data_dir = resolve_data_dir(argument_value("--data-dir").map(PathBuf::from));
    let (output_tx, output_rx) = mpsc::channel::<Value>();
    thread::Builder::new()
        .name("opennow-core-writer".to_owned())
        .spawn(move || {
            let stdout = io::stdout();
            let mut output = stdout.lock();
            for value in output_rx {
                if let Err(error) = write_json(&mut output, &value) {
                    eprintln!("opennow-core: output failed: {error}");
                    break;
                }
            }
        })
        .map_err(|error| error.to_string())?;
    let core = Arc::new(AppCore {
        artwork: artwork_cache::ArtworkCache::new(&data_dir, output_tx.clone()),
        settings: Mutex::new(
            SettingsStore::load(Some(data_dir.clone())).map_err(|error| error.to_string())?,
        ),
        gfn: GfnService::new(data_dir.clone())?,
        streamer: StreamerService::new(),
        diagnostics: diagnostics::DiagnosticsService::new(&data_dir)
            .map_err(|error| format!("Could not initialize diagnostics: {error}"))?,
        media: media::MediaService::new()
            .map_err(|error| format!("Could not initialize media library: {error}"))?,
        updater: updater::UpdaterService::new(&data_dir)
            .map_err(|error| format!("Could not initialize updater: {error}"))?,
        community: community::CommunityService::new()
            .map_err(|error| format!("Could not initialize community services: {error}"))?,
        thanks: thanks::ThanksService::new()
            .map_err(|error| format!("Could not initialize acknowledgements: {error}"))?,
        discord: discord::DiscordService::new(),
        telemetry: telemetry::TelemetryService::new()
            .map_err(|error| format!("Could not initialize reporting services: {error}"))?,
    });
    let cancelled = Arc::new(Mutex::new(HashSet::<String>::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.len() > MAXIMUM_LINE_BYTES {
            return Err("protocol line exceeds the size limit".to_owned());
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => return Err("malformed JSON protocol message".to_owned()),
        };
        if message["type"] == "cancel" {
            if let Some(id) = message["id"].as_str() {
                cancelled
                    .lock()
                    .expect("cancellation state poisoned")
                    .insert(id.to_owned());
            }
            continue;
        }
        if message["type"] != "request" {
            return Err("unknown protocol message".to_owned());
        }
        let id = message["id"].as_str().unwrap_or_default().to_owned();
        let method = message["method"].as_str().unwrap_or_default().to_owned();
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        if id.is_empty() || method.is_empty() {
            output_tx.send(json!({"type":"response", "id":id, "ok":false, "error":{"code":"invalid_request", "message":"Request requires string id and method"}}))
                .map_err(|error| error.to_string())?;
            continue;
        }
        if active.fetch_add(1, Ordering::AcqRel) >= MAXIMUM_CONCURRENT_REQUESTS {
            active.fetch_sub(1, Ordering::AcqRel);
            output_tx.send(json!({"type":"response", "id":id, "ok":false, "error":{"code":"busy", "message":"Core request limit reached"}}))
                .map_err(|error| error.to_string())?;
            continue;
        }

        let worker_core = Arc::clone(&core);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_active = Arc::clone(&active);
        let worker_output = output_tx.clone();
        thread::Builder::new().name(format!("opennow-rpc-{id}")).spawn(move || {
            let started = Instant::now();
            let result = dispatch(&method, &params, &worker_core);
            let outcome = match &result {
                Ok(_) => "ok",
                Err((code, _)) => code.as_str(),
            };
            worker_core.diagnostics.record(
                "rpc",
                &method,
                format!("outcome={outcome} durationMs={}", started.elapsed().as_millis()),
            );
            let was_cancelled = worker_cancelled.lock().expect("cancellation state poisoned").remove(&id);
            if !was_cancelled {
                match result {
                    Ok((value, event)) => {
                        let _ = worker_output.send(json!({"type":"response", "id":id, "ok":true, "result":value}));
                        if let Some((name, payload)) = event {
                            let _ = worker_output.send(json!({"type":"event", "name":name, "payload":payload}));
                        }
                    }
                    Err((code, message)) => {
                        let _ = worker_output.send(json!({"type":"response", "id":id, "ok":false, "error":{"code":code, "message":message}}));
                    }
                }
            }
            worker_active.fetch_sub(1, Ordering::AcqRel);
        }).map_err(|error| error.to_string())?;
    }
    Ok(())
}

type DispatchResult = Result<(Value, Option<(&'static str, Value)>), (String, String)>;

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn acceptance_window_system(params: &Value) -> Result<String, (String, String)> {
    let value = params["windowSystem"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let valid = match std::env::consts::OS {
        "linux" => matches!(value.as_str(), "xcb" | "wayland"),
        "windows" => value == "windows",
        "macos" => value == "cocoa",
        _ => false,
    };
    if !valid {
        return Err((
            "acceptance_platform_invalid".to_owned(),
            "Live acceptance requires the native X11, Wayland, Win32, or AppKit platform"
                .to_owned(),
        ));
    }
    Ok(value)
}

fn acceptance_shell_evidence(params: &Value) -> Result<Value, (String, String)> {
    let source = params["shell"].as_object().ok_or_else(|| {
        (
            "acceptance_shell_invalid".to_owned(),
            "Live acceptance requires bounded shell recovery and guide evidence".to_owned(),
        )
    })?;
    let counter = |key: &str| {
        source
            .get(key)
            .and_then(Value::as_u64)
            .filter(|value| *value <= 100)
            .ok_or_else(|| {
                (
                    "acceptance_shell_invalid".to_owned(),
                    format!("Live acceptance shell field {key} is missing or out of range"),
                )
            })
    };
    let streamer_recovery_count = counter("streamerRecoveryCount")?;
    let session_recovery_count = counter("sessionRecoveryCount")?;
    let pages = source
        .get("guidePagesVisited")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            (
                "acceptance_shell_invalid".to_owned(),
                "Live acceptance requires the visited guide page list".to_owned(),
            )
        })?;
    let allowed = [
        "guide-session",
        "guide-controls",
        "guide-media",
        "guide-shortcuts",
    ];
    let mut visited = pages
        .iter()
        .filter_map(Value::as_str)
        .filter(|page| allowed.contains(page))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    visited.sort();
    visited.dedup();
    Ok(json!({
        "streamerRecoveryCount": streamer_recovery_count,
        "sessionRecoveryCount": session_recovery_count,
        "guidePagesVisited": visited,
        "allGuidePagesVisited": visited.len() == allowed.len()
    }))
}

fn dispatch(method: &str, params: &Value, core: &AppCore) -> DispatchResult {
    match method {
        "core.hello" => {
            if params["protocolVersion"].as_i64() != Some(PROTOCOL_VERSION) {
                return Err((
                    "incompatible_protocol".to_owned(),
                    "Shell and core protocol versions differ".to_owned(),
                ));
            }
            Ok((
                json!({"protocolVersion":PROTOCOL_VERSION, "coreVersion":version::APPLICATION_VERSION, "capabilities":["settings", "gfn.deviceAuth", "gfn.providers", "gfn.publicCatalog", "gfn.accountLibrary", "gfn.regions", "gfn.subscription", "gfn.cloudmatch", "sessionProxy", "catalogArtworkCache.v1", "nativeStreamer.v5", "nativeStreamer.ownedNvstNegotiation", "nativeStreamer.dynamicSurface", "nativeStreamer.acceptanceEvidence", "liveAcceptance.v1", "osCredentialStore", "electronAccountMigration", "redactedDiagnostics", "mediaLibrary", "githubUpdateDiscovery", "discordRpc", "optInTelemetry", "feedback", "bugReports", "social.capabilitySurface"]}),
                None,
            ))
        }
        "app.status" => Ok((
            json!({"status":"ready", "version":version::APPLICATION_VERSION}),
            None,
        )),
        "social.capabilities.get" => Ok((
            json!({
                "friendsAvailable":false,
                "presenceAvailable":false,
                "invitesAvailable":false,
                "localControllerJoin":true,
                "reason":"NVIDIA does not expose the GeForce NOW friends, presence, or invitation service to third-party clients. OpenNOW will not display invented contacts or claim invitations were sent."
            }),
            None,
        )),
        "settings.get" => Ok((
            json!({"settings":core.settings.lock().expect("settings poisoned").all()}),
            None,
        )),
        "settings.set" => {
            let key = params["key"].as_str().ok_or((
                "invalid_params".to_owned(),
                "settings.set requires a key".to_owned(),
            ))?;
            let value = params.get("value").cloned().ok_or((
                "invalid_params".to_owned(),
                "settings.set requires a value".to_owned(),
            ))?;
            let applied = core
                .settings
                .lock()
                .expect("settings poisoned")
                .set(key, value)
                .map_err(|message| ("invalid_setting".to_owned(), message))?;
            let event = json!({"key":key, "value":applied});
            Ok((event.clone(), Some(("settings.changed", event))))
        }
        "settings.reset" => {
            let values = core
                .settings
                .lock()
                .expect("settings poisoned")
                .reset()
                .map_err(|message| ("settings_write_failed".to_owned(), message))?;
            Ok((
                json!({"settings":values}),
                Some(("settings.reset", json!({}))),
            ))
        }
        "auth.providers.list" => core
            .gfn
            .providers()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.device.start" => core
            .gfn
            .start_device_login(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.device.poll" => core
            .gfn
            .poll_device_login(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.device.complete" => core
            .gfn
            .complete_device_login(params)
            .map(|value| (value.clone(), Some(("auth.session.changed", value))))
            .map_err(gfn_error),
        "auth.device.cancel" => core
            .gfn
            .cancel_device_login(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.session.get" => core
            .gfn
            .session()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.logout" => {
            let value = core.gfn.logout().map_err(gfn_error)?;
            Ok((
                value.clone(),
                Some(("auth.session.changed", json!({"session":value["session"]}))),
            ))
        }
        "auth.accounts.logoutAll" => {
            let value = core.gfn.logout_all().map_err(gfn_error)?;
            Ok((
                value,
                Some(("auth.session.changed", json!({"session":null}))),
            ))
        }
        "auth.accounts.list" => core
            .gfn
            .saved_accounts()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.accounts.switch" => core
            .gfn
            .switch_account(params)
            .map(|value| (value.clone(), Some(("auth.session.changed", value))))
            .map_err(gfn_error),
        "auth.accounts.remove" => core
            .gfn
            .remove_account(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.pin.status" => core
            .gfn
            .pin_status(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.pin.set" => core
            .gfn
            .set_pin(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.pin.clear" => core
            .gfn
            .clear_pin(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "auth.pin.verify" => core
            .gfn
            .verify_pin(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "catalog.public.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .public_catalog(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.library.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .library_catalog(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.store.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .store_catalog(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "artwork.resolve" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.artwork
                .resolve(params, &settings)
                .map(|value| (value, None))
                .map_err(|message| ("invalid_params".to_owned(), message))
        }
        "network.regions.list" => core
            .gfn
            .regions()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "network.regions.ping" => network::ping_regions(params)
            .map(|value| (value, None))
            .map_err(|message| ("region_ping_failed".to_owned(), message)),
        "account.subscription.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .subscription(&settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.list" => core
            .gfn
            .account_connections()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.connections.sync" => core
            .gfn
            .sync_account_connection(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.connections.unlink" => core
            .gfn
            .unlink_account_connection(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.connections.link.start" => core
            .gfn
            .start_account_link(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.connections.link.poll" => core
            .gfn
            .poll_account_link(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.storage.locations" => core
            .gfn
            .persistent_storage_locations(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.storage.reset" => core
            .gfn
            .reset_persistent_storage(params)
            .map(|value| (value, None))
            .map_err(gfn_error),
        "session.create" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.streamer
                .validate_codec(&settings)
                .map_err(streamer_error)?;
            core.gfn
                .create_session(params, &settings)
                .map(|value| (value.clone(), Some(("session.changed", value))))
                .map_err(gfn_error)
        }
        "session.poll" => core
            .gfn
            .poll_session(params)
            .map(|value| (value.clone(), Some(("session.changed", value))))
            .map_err(gfn_error),
        "session.stop" => core
            .gfn
            .stop_session(params)
            .map(|value| (value.clone(), Some(("session.changed", value))))
            .map_err(gfn_error),
        "session.active.get" => core
            .gfn
            .active_session()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "session.remote.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .remote_sessions(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "session.claim" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .claim_session(params, &settings)
                .map(|value| (value.clone(), Some(("session.changed", value))))
                .map_err(gfn_error)
        }
        "session.ad.report" => core
            .gfn
            .report_session_ad(params)
            .map(|value| (value.clone(), Some(("session.changed", value))))
            .map_err(gfn_error),
        "streamer.detect" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.streamer
                .detect(&settings)
                .map(|value| (value, None))
                .map_err(streamer_error)
        }
        "streamer.start" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.streamer
                .start(params, &settings)
                .map(|value| (value.clone(), Some(("streamer.changed", value))))
                .map_err(streamer_error)
        }
        "streamer.prepare" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.streamer
                .prepare_embedded(params, &settings)
                .map(|value| (value, None))
                .map_err(streamer_error)
        }
        "streamer.status.get" => Ok((core.streamer.status(), None)),
        "streamer.stop" => core
            .streamer
            .stop(params["reason"].as_str().unwrap_or("session stopped"))
            .map(|value| (value.clone(), Some(("streamer.changed", value))))
            .map_err(streamer_error),
        "streamer.input.pause" => core
            .streamer
            .set_input_paused(params["paused"].as_bool().unwrap_or(true))
            .map(|value| (value, None))
            .map_err(streamer_error),
        "streamer.control" => core
            .streamer
            .control(params["action"].as_str().unwrap_or_default())
            .map(|value| (value, None))
            .map_err(streamer_error),
        "streamer.recording.start" => {
            let validated = core
                .media
                .validate_recording_target(params)
                .map_err(|message| ("media_recording_target_invalid".to_owned(), message))?;
            core.streamer
                .recording(&validated, true)
                .map(|value| (value, None))
                .map_err(streamer_error)
        }
        "streamer.recording.stop" => core
            .streamer
            .recording(params, false)
            .map(|value| (value.clone(), Some(("media.changed", value))))
            .map_err(streamer_error),
        "streamer.surface.update" => core
            .streamer
            .update_surface(params)
            .map(|value| (value, None))
            .map_err(streamer_error),
        "diagnostics.snapshot" => Ok((core.diagnostics.snapshot(), None)),
        "diagnostics.export" => {
            let runtime = json!({
                "schemaVersion": 1,
                "kind": "opennow.acceptance",
                "generatedAtMs": unix_time_millis().to_string(),
                "applicationVersion": version::APPLICATION_VERSION,
                "os": std::env::consts::OS,
                "cpuArchitecture": std::env::consts::ARCH,
                "streamer": core.streamer.acceptance_snapshot()
            });
            core.diagnostics
                .export_with_runtime(Some(&runtime))
                .map(|value| (value, None))
                .map_err(|error| ("diagnostics_export_failed".to_owned(), error.to_string()))
        }
        "acceptance.export" => {
            let window_system = acceptance_window_system(params)?;
            let streamer = core.streamer.acceptance_snapshot();
            let session_started_at_ms = streamer["sessionStartedAtMs"]
                .as_str()
                .and_then(|value| value.parse::<u128>().ok());
            let media = core
                .media
                .acceptance_evidence(session_started_at_ms)
                .map_err(|message| ("acceptance_media_failed".to_owned(), message))?;
            let shell = acceptance_shell_evidence(params)?;
            let checks = json!({
                "streamingTenMinutes": streamer["status"] == "streaming"
                    && streamer["sessionUptimeMs"].as_u64().unwrap_or_default() >= 600_000,
                "firstFramePresented": streamer["firstFrameLatencyMs"].is_number()
                    && streamer["mediaBackend"].is_string(),
                "nvstTransportActive": streamer["transport"] == "nvst",
                "nativeInputReady": streamer["inputReady"].as_bool().unwrap_or(false),
                "inputOwnershipExercised": streamer["inputPauseCount"].as_u64().unwrap_or_default() > 0
                    && streamer["inputResumeCount"].as_u64().unwrap_or_default() > 0,
                "allGuidePagesVisited": shell["allGuidePagesVisited"].as_bool().unwrap_or(false),
                "surfaceReconfigured": streamer["surfaceUpdateCount"].as_u64().unwrap_or_default() > 0,
                "fullscreenControlExercised": streamer["fullscreenToggleCount"].as_u64().unwrap_or_default() > 0,
                "statsControlExercised": streamer["statsToggleCount"].as_u64().unwrap_or_default() > 0,
                "recordingRoundTrip": streamer["recordingStartCount"].as_u64().unwrap_or_default() > 0
                    && streamer["recordingStopCount"].as_u64().unwrap_or_default() > 0,
                "mediaArtifactsComplete": media["complete"].as_bool().unwrap_or(false),
                "networkRecoveryExercised": shell["sessionRecoveryCount"].as_u64().unwrap_or_default() > 0,
                "streamerRecoveryExercised": shell["streamerRecoveryCount"].as_u64().unwrap_or_default() > 0,
                "noTerminalMediaError": streamer["errorCode"].is_null()
                    && streamer["decoderErrorCount"].as_u64().unwrap_or_default() == 0
                    && streamer["outputErrorCount"].as_u64().unwrap_or_default() == 0,
                "deviceRecoveryBalanced": streamer["deviceLossCount"].as_u64().unwrap_or_default()
                    <= streamer["deviceRecoveryCount"].as_u64().unwrap_or_default()
            });
            let observed_pass = checks
                .as_object()
                .is_some_and(|values| values.values().all(|value| value.as_bool() == Some(true)));
            let manifest = json!({
                "schemaVersion": 1,
                "kind": "opennow.live-acceptance",
                "generatedAtMs": unix_time_millis().to_string(),
                "applicationVersion": version::APPLICATION_VERSION,
                "platform": {
                    "os": std::env::consts::OS,
                    "cpuArchitecture": std::env::consts::ARCH,
                    "windowSystem": window_system
                },
                "stream": streamer,
                "shell": shell,
                "media": media,
                "checks": checks,
                "observedPass": observed_pass,
                "scope": "machine-observed-live-runtime"
            });
            core.diagnostics
                .export_acceptance(&manifest)
                .map(|value| (value, None))
                .map_err(|error| ("acceptance_export_failed".to_owned(), error.to_string()))
        }
        "media.root.get" => Ok((core.media.root(), None)),
        "media.recording.target" => core
            .media
            .recording_target(params)
            .map(|value| (value, None))
            .map_err(|message| ("media_recording_target_failed".to_owned(), message)),
        "media.list" => core
            .media
            .list(params)
            .map(|value| (value, None))
            .map_err(|message| ("media_list_failed".to_owned(), message)),
        "media.delete" => core
            .media
            .delete(params)
            .map(|value| (value.clone(), Some(("media.changed", value))))
            .map_err(|message| ("media_delete_failed".to_owned(), message)),
        "cache.delete" => Ok((core.gfn.clear_cache(), Some(("cache.changed", json!({}))))),
        "queue.status.get" => core
            .community
            .queue()
            .map(|value| (value, None))
            .map_err(|message| ("queue_fetch_failed".to_owned(), message)),
        "queue.serverMapping.get" => core
            .community
            .server_mapping()
            .map(|value| (value, None))
            .map_err(|message| ("server_mapping_fetch_failed".to_owned(), message)),
        "thanks.data.get" => Ok((core.thanks.data(), None)),
        "communityProxy.provision" => core
            .community
            .provision_proxy(core.gfn.device_id())
            .map(|value| (value, None))
            .map_err(|message| ("community_proxy_failed".to_owned(), message)),
        "updater.state.get" => Ok((core.updater.state(), None)),
        "updater.check" => {
            let value = core
                .updater
                .check(params)
                .map_err(|message| ("update_check_failed".to_owned(), message))?;
            Ok((value.clone(), Some(("updater.changed", value))))
        }
        "updater.highlights.get" => {
            let highlights = core.updater.highlights();
            let seen = core
                .settings
                .lock()
                .expect("settings poisoned")
                .all()["lastSeenReleaseHighlightsVersion"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let unseen = highlights["version"]
                .as_str()
                .is_some_and(|version| !version.is_empty() && version != seen);
            Ok((
                highlights.clone(),
                unseen.then_some(("updater.highlights.show", highlights)),
            ))
        }
        "updater.highlights.ack" => {
            let highlights = core.updater.highlights();
            let version = params["version"]
                .as_str()
                .or_else(|| highlights["version"].as_str())
                .unwrap_or(version::APPLICATION_VERSION)
                .trim_start_matches('v')
                .to_owned();
            if version.is_empty() || version.len() > 128 {
                return Err((
                    "invalid_version".to_owned(),
                    "Release version is invalid".to_owned(),
                ));
            }
            core.settings
                .lock()
                .expect("settings poisoned")
                .set("lastSeenReleaseHighlightsVersion", json!(version.clone()))
                .map_err(|message| ("settings_write_failed".to_owned(), message))?;
            Ok((json!({"acknowledged":true,"version":version}), None))
        }
        "updater.download" => core
            .updater
            .download()
            .map(|value| (value.clone(), Some(("updater.changed", value))))
            .map_err(|message| ("update_download_failed".to_owned(), message)),
        "updater.install" => core
            .updater
            .install(params)
            .map(|value| (value.clone(), Some(("updater.changed", value))))
            .map_err(|message| ("update_install_failed".to_owned(), message)),
        "discord.activity.sync" => core
            .discord
            .sync(params)
            .map(|value| (value, None))
            .map_err(|message| ("discord_rpc_failed".to_owned(), message)),
        "discord.activity.clear" => core
            .discord
            .clear()
            .map(|value| (value, None))
            .map_err(|message| ("discord_rpc_failed".to_owned(), message)),
        "telemetry.sync" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            let consent = settings["errorReportingConsent"]
                .as_str()
                .unwrap_or("unset");
            if consent != "granted" {
                return Ok((json!({"enabled":false,"sent":false}), None));
            }
            let install_id = ensure_install_id(core)?;
            core.telemetry
                .sync(consent, &install_id)
                .map(|value| (value, None))
                .map_err(|message| ("telemetry_failed".to_owned(), message))
        }
        "feedback.submit" => {
            let install_id = ensure_install_id(core)?;
            core.telemetry
                .feedback(&install_id, params)
                .map(|value| (value, None))
                .map_err(|message| ("feedback_failed".to_owned(), message))
        }
        "bug_report.submit" => {
            let install_id = ensure_install_id(core)?;
            let diagnostic =
                if params["includeDiagnostics"].as_bool() == Some(true) {
                    Some(core.diagnostics.export().map_err(|error| {
                        ("diagnostics_export_failed".to_owned(), error.to_string())
                    })?)
                } else {
                    None
                };
            let diagnostic_path = diagnostic
                .as_ref()
                .and_then(|value| value["path"].as_str())
                .map(PathBuf::from);
            core.telemetry
                .bug_report(&install_id, params, diagnostic_path.as_deref())
                .map(|value| (value, None))
                .map_err(|message| ("bug_report_failed".to_owned(), message))
        }
        _ => Err((
            "method_not_found".to_owned(),
            format!("Unknown core method: {method}"),
        )),
    }
}

fn ensure_install_id(core: &AppCore) -> Result<String, (String, String)> {
    let mut settings = core.settings.lock().expect("settings poisoned");
    let current = settings.all()["telemetryInstallId"]
        .as_str()
        .unwrap_or_default()
        .replace('-', "");
    let install_id = if telemetry::valid_install_id(&current) {
        current
    } else {
        let mut bytes = [0_u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        bytes.iter().map(|value| format!("{value:02x}")).collect()
    };
    settings
        .set("telemetryInstallId", json!(install_id.clone()))
        .map_err(|message| ("settings_write_failed".to_owned(), message))?;
    Ok(install_id)
}

fn gfn_error(error: gfn::ServiceError) -> (String, String) {
    (error.code.to_owned(), error.message)
}

fn streamer_error(error: streamer::StreamerError) -> (String, String) {
    (error.code.to_owned(), error.message)
}

fn write_json(output: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *output, value).map_err(|error| error.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|error| error.to_string())
}

fn argument_value(name: &str) -> Option<String> {
    let arguments: Vec<String> = env::args().collect();
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

#[cfg(test)]
mod acceptance_tests {
    use super::*;

    #[test]
    fn shell_acceptance_evidence_is_bounded_normalized_and_complete() {
        let evidence = acceptance_shell_evidence(&json!({"shell":{
            "streamerRecoveryCount":1,
            "sessionRecoveryCount":2,
            "guidePagesVisited":["guide-shortcuts","unknown","guide-session",
                                 "guide-controls","guide-media","guide-session"]
        }}))
        .unwrap();
        assert_eq!(evidence["allGuidePagesVisited"], true);
        assert_eq!(evidence["guidePagesVisited"].as_array().unwrap().len(), 4);
        assert!(
            acceptance_shell_evidence(&json!({"shell":{
                "streamerRecoveryCount":101,
                "sessionRecoveryCount":0,
                "guidePagesVisited":[]
            }}))
            .is_err()
        );
    }

    #[test]
    fn acceptance_platform_rejects_headless_and_accepts_only_the_native_plugin() {
        assert!(acceptance_window_system(&json!({"windowSystem":"offscreen"})).is_err());
        let expected = match std::env::consts::OS {
            "linux" => "wayland",
            "windows" => "windows",
            "macos" => "cocoa",
            _ => return,
        };
        assert_eq!(
            acceptance_window_system(&json!({"windowSystem":expected})).unwrap(),
            expected
        );
    }
}
