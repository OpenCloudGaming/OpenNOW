#![recursion_limit = "512"]

mod account_connections;
mod analytics;
mod artwork_cache;
mod bug_reports;
mod catalog_types;
mod cloudmatch;
mod community;
mod console_profiles;
mod credential_vault;
mod device_identity;
mod diagnostics;
mod discord;
mod frame_rate;
mod gfn;
mod language;
mod media;
mod network;
mod network_test;
mod persistent_storage;
mod proxy;
mod push_registry;
mod queue_servers;
mod reports;
mod requests;
mod server_vpc_cache;
mod settings;
mod store_cache;
mod store_catalog_page;
mod store_index;
mod store_requests;
mod streamer;
mod thanks;
mod updater;
mod version;

use fs2::FileExt;
use gfn::GfnService;
use opennow_core::update_apply;
use rand::RngCore;
use serde_json::{Map, Value, json};
use settings::{SettingsStore, resolve_data_dir};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use streamer::StreamerService;

const PROTOCOL_VERSION: i64 = 5;
const MAXIMUM_LINE_BYTES: usize = 1024 * 1024;
static PROFILE_LOCK: OnceLock<std::fs::File> = OnceLock::new();

struct AppCore {
    session_update_gate: Mutex<()>,
    artwork: artwork_cache::ArtworkCache,
    settings: Mutex<SettingsStore>,
    gfn: Arc<GfnService>,
    streamer: StreamerService,
    diagnostics: diagnostics::DiagnosticsService,
    media: media::MediaService,
    updater: updater::UpdaterService,
    push: Mutex<push_registry::PushRegistry>,
    community: community::CommunityService,
    thanks: thanks::ThanksService,
    discord: discord::DiscordService,
    reports: reports::ReportsClient,
    analytics: Arc<analytics::AnalyticsService>,
    pending_reports_retried: AtomicBool,
    bug_reports: bug_reports::BugReporter,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("opennow-core: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let data_dir = resolve_data_dir(argument_value("--data-dir").map(PathBuf::from));
    if env::args_os().any(|argument| argument == "--graphics-preferences") {
        let windows_gpu_device_id = SettingsStore::windows_gpu_device_id_read_only(Some(data_dir))
            .map_err(|error| error.to_string())?;
        let stdout = io::stdout();
        return write_json(
            &mut stdout.lock(),
            &json!({"version":1,"windowsGpuDeviceId":windows_gpu_device_id}),
        );
    }
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| format!("Could not initialize the data directory: {error}"))?;
    let profile_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join("core.lock"))
        .map_err(|error| format!("Could not open the data directory lock: {error}"))?;
    profile_lock.try_lock_exclusive().map_err(|error| {
        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            "The OpenNOW data directory is already in use".to_owned()
        } else {
            format!("Could not lock the data directory: {error}")
        }
    })?;
    PROFILE_LOCK.get_or_init(|| profile_lock);
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
    let reports_api = reports::api_base_from_env();
    let gfn = Arc::new(GfnService::new(data_dir.clone())?);
    let push = Mutex::new(push_registry::PushRegistry::new(
        Arc::clone(&gfn),
        output_tx.clone(),
        data_dir.clone(),
    ));
    let core = Arc::new(AppCore {
        session_update_gate: Mutex::new(()),
        artwork: artwork_cache::ArtworkCache::new(&data_dir, output_tx.clone()),
        settings: Mutex::new(
            SettingsStore::load(Some(data_dir.clone())).map_err(|error| error.to_string())?,
        ),
        gfn,
        streamer: StreamerService::new(),
        diagnostics: diagnostics::DiagnosticsService::new(&data_dir)
            .map_err(|error| format!("Could not initialize diagnostics: {error}"))?,
        media: media::MediaService::new()
            .map_err(|error| format!("Could not initialize media library: {error}"))?,
        updater: updater::UpdaterService::new(&data_dir)
            .map_err(|error| format!("Could not initialize updater: {error}"))?,
        push,
        community: community::CommunityService::new()
            .map_err(|error| format!("Could not initialize community services: {error}"))?,
        thanks: thanks::ThanksService::new()
            .map_err(|error| format!("Could not initialize acknowledgements: {error}"))?,
        discord: discord::DiscordService::new(),
        reports: reports::ReportsClient::new(&reports_api, &data_dir)
            .map_err(|error| format!("Could not initialize reporting services: {error}"))?,
        analytics: Arc::new(
            analytics::AnalyticsService::new(&reports_api, &data_dir)
                .map_err(|error| format!("Could not initialize analytics: {error}"))?,
        ),
        pending_reports_retried: AtomicBool::new(false),
        bug_reports: bug_reports::BugReporter::default(),
    });
    if !reporting_enabled(&core) {
        core.analytics.wipe();
        core.reports.clear_pending();
    }
    let analytics_finished = core.analytics.start()?;
    reconcile_push(&core);
    let requests = Arc::new(requests::Requests::default());
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
                requests.cancel(id);
            }
            continue;
        }
        if message["type"] == "ack" {
            if let Some(id) = message["id"].as_str() {
                requests.acknowledge(id);
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
        let Some(permit) = requests.admit(&id, &method) else {
            output_tx.send(json!({"type":"response", "id":id, "ok":false, "error":{"code":"busy", "message":"Core request limit reached"}}))
                .map_err(|error| error.to_string())?;
            continue;
        };

        let worker_core = Arc::clone(&core);
        let worker_output = output_tx.clone();
        thread::Builder::new().name(format!("opennow-rpc-{id}")).spawn(move || {
            let started = Instant::now();
            let result = requests::scope(permit.token.clone(), || {
                requests::check().map_err(|error| (error.code.to_owned(), error.message))?;
                dispatch(&method, &params, &worker_core, &worker_output)
            });
            let outcome = match &result {
                Ok(_) => "ok",
                Err((code, _)) => code.as_str(),
            };
            worker_core.diagnostics.record(
                "rpc",
                &method,
                format!("outcome={outcome} durationMs={}", started.elapsed().as_millis()),
            );
            observe_reporting(&worker_core, &worker_output, &method, &params, &result);
            let was_cancelled = permit.token.cancelled();
            if method == "session.create"
                && let Err((code, message)) = &result
                && code == "session_cleanup_pending"
            {
                let _ = worker_output.send(json!({"type":"event","name":"session.cleanup.pending",
                    "payload":{"code":code,"message":message}}));
            }
            if method == "session.create"
                && let Ok((value, _)) = &result
                && let Some(session_id) = value["session"]["sessionId"].as_str()
            {
                let delivered = !was_cancelled && worker_output.send(json!({"type":"response", "id":id, "ok":true, "result":value})).is_ok();
                let accepted = delivered && permit.token.await_acceptance(std::time::Duration::from_secs(10));
                let cleanup = worker_core.gfn.finish_session_create(session_id, accepted);
                worker_core.diagnostics.record("session", "allocation-handoff", format!(
                    "accepted={accepted} cleanup={}", cleanup.as_ref().map_or_else(|error| error.code, |()| "ok")
                ));
                if let Err(error) = cleanup {
                    let _ = worker_output.send(json!({"type":"event","name":"session.cleanup.pending","payload":{
                        "sessionId":session_id,"code":error.code,"message":"The cancelled cloud session could not be closed. End it before starting another game."
                    }}));
                }
                return;
            }
            if matches!(method.as_str(), "updater.check" | "updater.download" | "updater.install") {
                if let Err((_, message)) = &result {
                    worker_core.updater.request_failed(message);
                }
                let _ = worker_output.send(json!({"type":"event", "name":"updater.changed", "payload":worker_core.updater.state()}));
            }
            if matches!(
                method.as_str(),
                "auth.device.complete"
                    | "auth.logout"
                    | "auth.accounts.logoutAll"
                    | "auth.accounts.switch"
                    | "auth.accounts.remove"
                    | "settings.set"
            ) {
                reconcile_push(&worker_core);
            }
            if !was_cancelled {
                match result {
                    Ok((value, event)) => {
                        if let Some(("settings.changed", payload)) = &event {
                            let _ = worker_output.send(json!({"type":"event", "name":"settings.changed", "payload":payload}));
                        }
                        let _ = worker_output.send(json!({"type":"response", "id":id, "ok":true, "result":value}));
                        if let Some((name, payload)) = event && name != "settings.changed" {
                            let _ = worker_output.send(json!({"type":"event", "name":name, "payload":payload}));
                        }
                    }
                    Err((code, message)) => {
                        let _ = worker_output.send(json!({"type":"response", "id":id, "ok":false, "error":{"code":code, "message":message}}));
                    }
                }
            }
            drop(permit);
        }).map_err(|error| error.to_string())?;
    }
    core.analytics.shutdown(&analytics_finished);
    Ok(())
}

type DispatchResult = Result<(Value, Option<(&'static str, Value)>), (String, String)>;

fn update_session_idle(session: &Value, streamer: &Value) -> bool {
    session.get("session") == Some(&Value::Null)
        && matches!(
            streamer["streamer"]["status"].as_str(),
            Some("stopped" | "error")
        )
}

fn reconcile_push(core: &AppCore) {
    let _ = core
        .push
        .lock()
        .expect("push registry poisoned")
        .reconcile();
}

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

fn dispatch(
    method: &str,
    params: &Value,
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
) -> DispatchResult {
    let session_transition = matches!(
        method,
        "session.create" | "session.claim" | "session.poll" | "streamer.start" | "streamer.prepare"
    );
    let _session_update_guard = if session_transition || method == "updater.install" {
        Some(core.session_update_gate.try_lock().map_err(|_| {
            (
                "session_update_busy".to_owned(),
                "A session transition or update preparation is in progress".to_owned(),
            )
        })?)
    } else {
        None
    };
    if session_transition && core.updater.installation_pending() {
        return Err((
            "update_pending".to_owned(),
            "An update is waiting for OpenNOW to exit".to_owned(),
        ));
    }
    match method {
        "core.hello" => {
            if params["protocolVersion"].as_i64() != Some(PROTOCOL_VERSION) {
                return Err((
                    "incompatible_protocol".to_owned(),
                    "Shell and core protocol versions differ".to_owned(),
                ));
            }
            Ok((
                json!({"protocolVersion":PROTOCOL_VERSION, "coreVersion":version::APPLICATION_VERSION, "capabilities":["settings", "gfn.deviceAuth", "gfn.providers", "gfn.publicCatalog", "catalog.storePages.v1", "catalog.libraryPages.v1", "catalog.metadata.v1", "account.syncObservation.v1", "account.pushInvalidation.v1", "catalog.languages.v1", "queue.servers.v1", "catalog.storeLocal.v1", "gfn.accountLibrary", "gfn.regions", "gfn.subscription", "gfn.cloudmatch", "sessionProxy", "catalogArtworkCache.v1", "nativeStreamer.v7", "nativeStreamer.ownedNvstNegotiation", "nativeStreamer.dynamicSurface", "nativeStreamer.acceptanceEvidence", "liveAcceptance.v1", "osCredentialStore", "electronAccountMigration", "redactedDiagnostics", "mediaLibrary", "githubUpdateDiscovery", "discordRpc", "feedback", "bugReports", "automaticBugReports.v2", "social.capabilitySurface"]}),
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
            json!({"settings":core.settings.lock().expect("settings poisoned").all(),
                "keyboardLayouts":language::keyboard_choices()}),
            None,
        )),
        "settings.choices.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            Ok((
                json!({"colorQualities":streamer::StreamerService::color_quality_choices(
                &settings, &params["runtimeCapabilities"]),
                "codecs":streamer::StreamerService::codec_choices(
                &settings, &params["runtimeCapabilities"]),
                "frameRates":frame_rate::frame_rate_choices(
                &settings, &params["runtimeCapabilities"])}),
                None,
            ))
        }
        "settings.set" => {
            let key = params["key"].as_str().ok_or((
                "invalid_params".to_owned(),
                "settings.set requires a key".to_owned(),
            ))?;
            let value = params.get("value").cloned().ok_or((
                "invalid_params".to_owned(),
                "settings.set requires a value".to_owned(),
            ))?;
            if key == "region" {
                let provider = params["providerIdpId"].as_str().unwrap_or("");
                let event = core.gfn.with_region_provider(provider, || {
                    let mut settings = core.settings.lock().expect("settings poisoned");
                    let applied = settings.set_provider_region(provider, value).map_err(|message| gfn::ServiceError { code: "invalid_setting", message })?;
                    Ok(json!({"key":key,"value":applied,"changes":{
                        "regionProviderIdpId":provider,"providerRegions":settings.all()["providerRegions"]
                    }}))
                }).map_err(gfn_error)?;
                return Ok((event.clone(), Some(("settings.changed", event))));
            }
            let mut settings = core.settings.lock().expect("settings poisoned");
            let codec_before = settings.all()["codec"].clone();
            let fallback_before = settings.all()["fallbackCodec"].clone();
            let applied = settings
                .set(key, value)
                .map_err(|message| ("invalid_setting".to_owned(), message))?;
            let mut event = json!({"key":key, "value":applied});
            if key == "colorQuality" {
                // An explicitly saved codec the new color mode cannot use is
                // healed toward Auto in the same save; report the repair so the
                // shell follows it without re-reading all settings.
                let current = settings.all();
                let mut changes = Map::new();
                if current["codec"] != codec_before {
                    changes.insert("codec".to_owned(), current["codec"].clone());
                }
                if current["fallbackCodec"] != fallback_before {
                    changes.insert("fallbackCodec".to_owned(), current["fallbackCodec"].clone());
                }
                if !changes.is_empty() {
                    event["changes"] = Value::Object(changes);
                }
            }
            if key == "launchInConsoleMode" && applied == json!(false) {
                event["changes"] = json!({"switchToConsoleOnPad": false});
            } else if key == "themePack" {
                let values = settings.all();
                event["changes"] = json!({
                    "appTheme": values["appTheme"],
                    "themeAccentOverride": values["themeAccentOverride"]
                });
            } else if key == "appAccentColor" {
                event["changes"] = json!({"themeAccentOverride": true});
            }
            if key == "microphoneMode" && applied == json!("voice-activity") {
                event["changes"] = json!({"microphoneDeviceId": ""});
            }
            Ok((event.clone(), Some(("settings.changed", event))))
        }
        "settings.shortcuts.update" => {
            let bindings = core
                .settings
                .lock()
                .expect("settings poisoned")
                .set_shortcuts(&params["bindings"])
                .map_err(|message| ("invalid_setting".to_owned(), message))?;
            let (key, value) = bindings
                .iter()
                .next()
                .map(|(key, value)| (key.clone(), value.clone()))
                .expect("shortcut transaction applies at least one binding");
            let event = json!({"key":key, "value":value, "changes":bindings});
            Ok((
                json!({"bindings":bindings}),
                Some(("settings.changed", event)),
            ))
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
            Ok((value.clone(), Some(("auth.session.changed", value))))
        }
        "auth.accounts.logoutAll" => {
            let value = core.gfn.logout_all().map_err(gfn_error)?;
            Ok((value.clone(), Some(("auth.session.changed", value))))
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
            .map(|value| (value.clone(), Some(("auth.session.changed", value))))
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
        "catalog.game.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_game(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.launch.inspect" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_launch_inspect(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.launch.store.inspect" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .store_launch_inspect(params, None, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.favorites.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_favorites(&settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.favorites.add"
        | "catalog.favorites.remove"
        | "catalog.ownership.add"
        | "catalog.ownership.remove"
        | "catalog.ownership.select" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_mutate(method, params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.definitions.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_definitions(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.languages.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .catalog_languages(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "catalog.store.local" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .store_local_catalog(params, &settings)
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
        "catalog.store.presentation" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .store_presentation(params, &settings)
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
        "network.regions.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .regions(&settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "network.regions.ping" => network::ping_regions(params)
            .map(|value| (value, None))
            .map_err(|message| ("region_ping_failed".to_owned(), message)),
        "queue.servers.list" => queue_servers::list()
            .map(|value| (value, None))
            .map_err(gfn_error),
        "account.subscription.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .subscription(&settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.list" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .account_connections(&settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.sync" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .sync_account_connection(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.unlink" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .unlink_account_connection(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.link.start" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .start_account_link(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.link.poll" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .poll_account_link(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.sync.status" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .account_sync_status(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
        "account.connections.sync.cancel" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .account_sync_status(
                    &json!({"operationId":params["operationId"],"cancelObservation":true}),
                    &settings,
                )
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
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
            let settings = cloudmatch::allocation_settings(&settings);
            let settings = if !params["runtimeCapabilities"].is_null() {
                StreamerService::embedded_session_settings(
                    &settings,
                    &params["runtimeCapabilities"],
                )
                .map_err(streamer_error)?
            } else {
                core.streamer
                    .validate_codec(&settings)
                    .map_err(streamer_error)?;
                settings
            };
            core.gfn
                .create_session(params, &settings)
                .map(|value| (value.clone(), Some(("session.changed", value))))
                .map_err(gfn_error)
        }
        "session.poll" => core
            .gfn
            .poll_session(params)
            .map(|value| {
                (
                    value.clone(),
                    (params["recoveryMode"] != true).then_some(("session.changed", value)),
                )
            })
            .map_err(gfn_error),
        "session.stop" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .stop_session(params, &settings)
                .map(|value| (value.clone(), Some(("session.changed", value))))
                .map_err(gfn_error)
        }
        "session.active.get" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.gfn
                .reconcile_active_session(params, &settings)
                .map(|value| (value, None))
                .map_err(gfn_error)
        }
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
            if params["session"]["transportMode"] == "webrtc" {
                core.diagnostics.record(
                    "streamer",
                    "prepare_endpoints",
                    diagnostics::stream_endpoint_evidence(&params["session"]).to_string(),
                );
            }
            core.gfn
                .prepare_owned_stream(params, |owned| {
                    core.streamer
                        .prepare_embedded(owned, &settings)
                        .map_err(|error| gfn::ServiceError {
                            code: error.code,
                            message: error.message,
                        })
                })
                .inspect_err(|error| {
                    core.diagnostics.record(
                        "streamer",
                        "prepare_profile",
                        diagnostics::stream_profile_evidence(&params["session"]).to_string(),
                    );
                    core.diagnostics.record(
                        "streamer",
                        "prepare_rejected",
                        diagnostics::runtime_failure_reason(&error.message),
                    );
                })
                .map(|value| (value, None))
                .map_err(gfn_error)
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
                "streamer": core.streamer.acceptance_snapshot(),
                "nativeRuntime": diagnostics::native_runtime_evidence(&params["runtimeCapabilities"]),
                "shell": diagnostics::embedded_drop_evidence(params)
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
        "media.root.get" => core
            .media
            .root()
            .map(|value| (value, None))
            .map_err(|message| ("media_unavailable".to_owned(), message)),
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
        "updater.startup.ack" => {
            update_apply::acknowledge_startup_from_env(version::APPLICATION_VERSION)
                .map(|acknowledged| (json!({"acknowledged":acknowledged}), None))
                .map_err(|message| ("update_startup_ack_failed".to_owned(), message))
        }
        "updater.check" => {
            let value = core
                .updater
                .check(params)
                .map_err(|message| ("update_check_failed".to_owned(), message))?;
            Ok((value, None))
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
            // Historical notes remain readable without announcing an older
            // release as an available update or navigating away from About.
            let unseen = core.updater.state()["availableVersion"]
                .as_str()
                .is_some_and(|version| {
                    !version.is_empty()
                        && version != seen
                        && highlights["version"].as_str() == Some(version)
                });
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
            .map(|value| (value, None))
            .map_err(|message| ("update_download_failed".to_owned(), message)),
        "updater.install" => {
            let session = core.gfn.active_session().map_err(gfn_error)?;
            if !update_session_idle(&session, &core.streamer.status()) {
                return Err((
                    "update_session_active".to_owned(),
                    "End the active session before installing an update".to_owned(),
                ));
            }
            core.updater
                .install(params)
                .map(|value| (value, None))
                .map_err(|message| ("update_install_failed".to_owned(), message))
        }
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
        "feedback.submit" => {
            let install_id = ensure_install_id(core)?;
            let category = params["category"].as_str().unwrap_or("other");
            if !matches!(category, "bug" | "idea" | "other") {
                return Err((
                    "feedback_failed".to_owned(),
                    "Unsupported feedback category".to_owned(),
                ));
            }
            let message = reports::required_text(&params["message"], 8, 4_000, "Feedback")
                .map_err(|message| ("feedback_failed".to_owned(), message))?;
            let include = params["includeSystemInfo"].as_bool() == Some(true);
            let report = reports::Document {
                install_id: &install_id,
                account: &Value::Null,
                activity: if include {
                    core.bug_reports.activity()
                } else {
                    json!({"currentGame":null, "recentGames":[]})
                },
                app: report_app(core, include),
            }
            .manual(&format!("Feedback: {category}"), &message);
            match upload_report(core, &report, None) {
                Ok(accepted) => Ok((
                    json!({"submitted":true, "reportId":accepted.report_id,
                        "message":"Thanks — your feedback was sent."}),
                    None,
                )),
                Err(failure) if failure.retryable => Ok((
                    json!({"submitted":false, "queued":true,
                        "message":"Your feedback will be sent the next time OpenNOW starts."}),
                    None,
                )),
                Err(failure) => Err(("feedback_failed".to_owned(), failure.message)),
            }
        }
        "bug_report.submit" => {
            let install_id = ensure_install_id(core)?;
            let title = reports::required_text(&params["title"], 8, 120, "Bug report title")
                .map_err(|message| ("bug_report_failed".to_owned(), message))?;
            let description = reports::required_text(
                &params["description"],
                40,
                12_000,
                "Bug report description",
            )
            .map_err(|message| ("bug_report_failed".to_owned(), message))?;
            let log = if params["includeDiagnostics"].as_bool() == Some(true) {
                let export = core
                    .diagnostics
                    .export()
                    .map_err(|error| ("diagnostics_export_failed".to_owned(), error.to_string()))?;
                export["path"]
                    .as_str()
                    .and_then(|path| reports::gzip_log(Path::new(path)))
            } else {
                None
            };
            let report = reports::Document {
                install_id: &install_id,
                account: &core.gfn.bug_report_identity(),
                activity: core.bug_reports.activity(),
                app: report_app(core, true),
            }
            .manual(&title, &description);
            match upload_report(core, &report, log) {
                Ok(accepted) => Ok((
                    json!({"submitted":true, "reportId":accepted.report_id,
                        "issueStatus":accepted.issue_status}),
                    None,
                )),
                Err(failure) if failure.retryable => {
                    Ok((json!({"submitted":false, "queued":true}), None))
                }
                Err(failure) => Err(("bug_report_failed".to_owned(), failure.message)),
            }
        }
        "analytics.track" => {
            if let Some(surface) = analytics::ui_surface(params)
                .map_err(|message| ("invalid_params".to_owned(), message))?
            {
                core.analytics.set_ui_surface(surface);
            }
            if params["event"] == "app_opened" {
                return Ok((
                    match core.analytics.open_app(&params["runtimeCapabilities"]) {
                        Some(props) => track(core, "app_opened", Value::Object(props)),
                        None => json!({"accepted":false,"reason":"already_sent"}),
                    },
                    None,
                ));
            }
            let event = analytics::shell_event(params)
                .map_err(|message| ("invalid_params".to_owned(), message))?;
            Ok((track(core, event.name, Value::Object(event.props)), None))
        }
        "bug_report.incident" => {
            let incident = bug_reports::BugReporter::shell_incident(params)
                .map_err(|message| ("invalid_params".to_owned(), message))?;
            Ok((submit_automatic_report(core, output, incident), None))
        }
        _ => Err((
            "method_not_found".to_owned(),
            format!("Unknown core method: {method}"),
        )),
    }
}

fn observe_reporting(
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
    method: &str,
    params: &Value,
    result: &DispatchResult,
) {
    match (method, result) {
        ("session.create", _) => {
            let detail = core.bug_reports.observe_launch(params);
            core.diagnostics.record("activity", "game_launch", detail);
            track(
                core,
                "game_launch_requested",
                json!({"game_id":params["appId"], "game_title":params["title"],
                    "store":params["store"], "zone":params["zone"]}),
            );
        }
        ("session.stop", Ok(_)) => core.bug_reports.observe_stop(),
        ("streamer.prepare", _) => core
            .analytics
            .observe_runtime(&params["runtimeCapabilities"]),
        ("auth.device.complete", Ok(_)) => observe_sign_in(core, false),
        ("auth.session.get" | "auth.accounts.switch", Ok(_)) => observe_sign_in(core, true),
        ("auth.logout" | "auth.accounts.logoutAll", Ok(_)) => core.analytics.observe_sign_out(),
        ("settings.set", Ok((value, _))) if params["key"] == "automaticBugReports" => {
            observe_consent(core, &value["value"], &params["source"])
        }
        _ => {}
    }
    if let Err((code, message)) = result
        && let Some(incident) = bug_reports::BugReporter::rpc_incident(method, code, message)
    {
        if incident.kind == bug_reports::Kind::LibraryError {
            track(core, "library_load_failed", json!({"code":incident.code}));
        } else {
            let game_id = core.bug_reports.activity()["currentGame"]["appId"].clone();
            track(
                core,
                "session_error",
                json!({"stage":method.split('.').nth(1), "code":incident.code, "game_id":game_id}),
            );
        }
        submit_automatic_report(core, output, incident);
    }
}

fn reporting_enabled(core: &AppCore) -> bool {
    bug_reports::enabled(&core.settings.lock().expect("settings poisoned").all())
}

fn track(core: &AppCore, event: &str, props: Value) -> Value {
    if !reporting_enabled(core) {
        return json!({"accepted":false,"reason":"disabled"});
    }
    let install_id = match ensure_install_id(core) {
        Ok(install_id) => install_id,
        Err((_, message)) => return json!({"accepted":false,"reason":message}),
    };
    let identity = core.gfn.bug_report_identity();
    let account = if identity.is_null() {
        Value::Null
    } else {
        json!({"userId":identity["userId"], "providerIdpId":identity["providerIdpId"]})
    };
    let dropped = core.analytics.enqueue(
        &install_id,
        account,
        event,
        analytics::sanitize(event, &props),
    );
    if dropped % 100 == 1 {
        core.diagnostics
            .record("analytics", "queue-overflow", format!("dropped={dropped}"));
    }
    json!({"accepted":true})
}

fn observe_consent(core: &AppCore, value: &Value, source: &Value) {
    let Some(value) = value.as_str().filter(|value| *value != "unset") else {
        return;
    };
    if value == "disabled" {
        core.analytics.wipe();
        core.reports.clear_pending();
    }
    let Ok(install_id) = ensure_install_id(core) else {
        return;
    };
    core.analytics.enqueue(
        &install_id,
        Value::Null,
        "consent_changed",
        analytics::sanitize(
            "consent_changed",
            &json!({"value":value, "source":source.as_str().unwrap_or("settings")}),
        ),
    );
}

fn observe_sign_in(core: &Arc<AppCore>, restored: bool) {
    let identity = core.gfn.bug_report_identity();
    let Some(user_id) = identity["userId"].as_str() else {
        return;
    };
    if !core.analytics.observe_sign_in(user_id) {
        return;
    }
    track(
        core,
        "signed_in",
        json!({"provider_code":identity["providerCode"], "alliance_partner":identity["alliancePartner"],
            "membership_tier":identity["membershipTier"], "restored":restored}),
    );
    if !reporting_enabled(core) || core.pending_reports_retried.swap(true, Ordering::AcqRel) {
        return;
    }
    let worker_core = Arc::clone(core);
    let _ = thread::Builder::new()
        .name("opennow-report-retry".to_owned())
        .spawn(move || {
            for (report, accepted) in worker_core.reports.retry_pending(unix_time_millis()) {
                record_sent(&worker_core, &report, &accepted);
            }
        });
}

fn report_app(core: &AppCore, include_device: bool) -> Value {
    let device = if include_device {
        core.analytics.device()
    } else {
        json!({"gpu":null, "decoderBackend":null})
    };
    let backend = core.streamer.acceptance_snapshot()["mediaBackend"]
        .as_str()
        .filter(|backend| include_device && analytics::valid_code(backend))
        .map_or_else(|| device["decoderBackend"].clone(), Value::from);
    json!({
        "version": version::APPLICATION_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "gpu": device["gpu"],
        "decoderBackend": backend,
        "channel": version::update_channel(version::APPLICATION_VERSION)
    })
}

fn upload_report(
    core: &AppCore,
    report: &Value,
    log: Option<Vec<u8>>,
) -> Result<reports::Accepted, reports::Failure> {
    let result = core.reports.submit(report, log.as_deref());
    match &result {
        Ok(accepted) => record_sent(core, report, accepted),
        Err(failure) => {
            let spooled = failure.retryable
                && core
                    .reports
                    .spool(report, log.as_deref(), unix_time_millis())
                    .is_ok();
            core.diagnostics.record(
                "bug-report",
                "upload-failed",
                format!("retryable={} spooled={spooled}", failure.retryable),
            );
        }
    }
    result
}

fn record_sent(core: &AppCore, report: &Value, accepted: &reports::Accepted) {
    core.diagnostics.record(
        "bug-report",
        "sent",
        format!("reportId={}", accepted.report_id),
    );
    let automatic = report["automatic"] == true;
    let (kind, code) = if automatic {
        (
            report["trigger"]["kind"].clone(),
            report["trigger"]["code"].clone(),
        )
    } else {
        (json!("manual"), json!("manual"))
    };
    track(
        core,
        "bug_report_sent",
        json!({"report_id":accepted.report_id, "issue_id":accepted.issue_id,
            "kind":kind, "code":code, "automatic":automatic}),
    );
}

fn submit_automatic_report(
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
    incident: bug_reports::Incident,
) -> Value {
    if !reporting_enabled(core) {
        return json!({"accepted":false,"reason":"disabled"});
    }
    let account = core.gfn.bug_report_identity();
    if account.is_null() {
        return json!({"accepted":false,"reason":"signed_out"});
    }
    let Some(activity) = core.bug_reports.admit(&incident, Instant::now()) else {
        return json!({"accepted":false,"reason":"already_reported"});
    };
    let install_id = match ensure_install_id(core) {
        Ok(install_id) => install_id,
        Err((_, message)) => return json!({"accepted":false,"reason":message}),
    };
    core.diagnostics.record(
        "bug-report",
        "automatic",
        format!("kind={} code={}", incident.kind.as_str(), incident.code),
    );
    let report = reports::Document {
        install_id: &install_id,
        account: &account,
        activity: activity.clone(),
        app: report_app(core, true),
    }
    .automatic(incident.trigger());
    let mut payload = json!({"state":"sending", "kind":incident.kind.as_str(),
        "code":incident.code, "game":activity["currentGame"]["title"]});
    let _ =
        output.send(json!({"type":"event","name":"bug_report.changed","payload":payload.clone()}));
    let worker_core = Arc::clone(core);
    let worker_output = output.clone();
    let spawned = thread::Builder::new()
        .name("opennow-bug-report".to_owned())
        .spawn(move || {
            let core = worker_core;
            let runtime = json!({
                "schemaVersion": 1,
                "kind": "opennow.automatic-bug-report",
                "generatedAtMs": unix_time_millis().to_string(),
                "applicationVersion": version::APPLICATION_VERSION,
                "os": std::env::consts::OS,
                "cpuArchitecture": std::env::consts::ARCH,
                "trigger": incident.trigger(),
                "activity": activity,
                "streamer": core.streamer.acceptance_snapshot()
            });
            let log = core
                .diagnostics
                .export_with_runtime(Some(&runtime))
                .ok()
                .and_then(|export| {
                    let path = PathBuf::from(export["path"].as_str()?);
                    let log = reports::gzip_log(&path);
                    let _ = std::fs::remove_file(path);
                    log
                });
            match upload_report(&core, &report, log) {
                Ok(accepted) => {
                    payload["state"] = json!("sent");
                    payload["reportId"] = json!(accepted.report_id);
                    payload["issueStatus"] = json!(accepted.issue_status);
                }
                Err(failure) => {
                    payload["state"] = json!("failed");
                    payload["message"] = json!(failure.message);
                    payload["queued"] = json!(failure.retryable);
                }
            }
            core.diagnostics.record(
                "bug-report",
                "automatic-result",
                format!("state={}", payload["state"].as_str().unwrap_or_default()),
            );
            let _ = worker_output
                .send(json!({"type":"event","name":"bug_report.changed","payload":payload}));
        });
    if spawned.is_err() {
        return json!({"accepted":false,"reason":"Could not start the report upload"});
    }
    json!({"accepted":true})
}

fn ensure_install_id(core: &AppCore) -> Result<String, (String, String)> {
    let mut settings = core.settings.lock().expect("settings poisoned");
    let current = settings.all()["telemetryInstallId"]
        .as_str()
        .unwrap_or_default()
        .replace('-', "");
    let install_id = if reports::valid_install_id(&current) {
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
    fn updates_require_no_session_and_a_terminal_streamer() {
        for status in ["stopped", "error"] {
            assert!(update_session_idle(
                &json!({"session":null}),
                &json!({"streamer":{"status":status}})
            ));
        }
        for status in [
            "starting",
            "streaming",
            "recovering",
            "negotiating",
            "unknown",
        ] {
            assert!(!update_session_idle(
                &json!({"session":null}),
                &json!({"streamer":{"status":status}})
            ));
        }
        for session in [
            json!({}),
            json!({"session":{"phase":"queued"}}),
            json!({"session":{"phase":"ready"}}),
        ] {
            assert!(!update_session_idle(
                &session,
                &json!({"streamer":{"status":"stopped"}})
            ));
        }
    }

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
