#![recursion_limit = "512"]

mod analytics;
mod artwork_cache;
mod bug_reports;
mod diagnostics;
mod discord;
mod frame_rate;
mod language;
mod media;
mod network;
mod playback_endpoints;
mod plugins;
mod proxy;
mod reports;
mod requests;
mod service_error;
mod settings;
mod sources;
mod streamer;
mod thanks;
mod updater;
mod version;

use fs2::FileExt;
use opennow_core::update_apply;
use rand::RngCore;
use serde_json::{Map, Value, json};
use settings::{SettingsStore, resolve_data_dir};
use sources::SourceHost;
use sources::contract::{Completion, ReportingEffect, SessionOccupancy};
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
    settings: Arc<Mutex<SettingsStore>>,
    sources: Arc<SourceHost>,
    streamer: Arc<StreamerService>,
    diagnostics: Arc<diagnostics::DiagnosticsService>,
    media: media::MediaService,
    updater: updater::UpdaterService,
    thanks: thanks::ThanksService,
    discord: discord::DiscordService,
    reports: reports::ReportsClient,
    analytics: Arc<analytics::AnalyticsService>,
    pending_reports_retried: AtomicBool,
    bug_reports: bug_reports::BugReporter,
}

struct SourceShutdown(Arc<SourceHost>);

impl Drop for SourceShutdown {
    fn drop(&mut self) {
        self.0.shutdown();
    }
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
    let settings = Arc::new(Mutex::new(
        SettingsStore::load(Some(data_dir.clone())).map_err(|error| error.to_string())?,
    ));
    let streamer = Arc::new(StreamerService::new());
    let diagnostics = Arc::new(
        diagnostics::DiagnosticsService::new(&data_dir)
            .map_err(|error| format!("Could not initialize diagnostics: {error}"))?,
    );
    let builtin = Arc::new(sources::gfn::GfnModule::open(
        data_dir.clone(),
        output_tx.clone(),
        Arc::clone(&settings),
        Arc::clone(&streamer),
        Arc::clone(&diagnostics),
    )?);
    let plugins = Arc::new(
        plugins::PluginManager::open(&data_dir, output_tx.clone())
            .map_err(|error| error.to_string())?,
    );
    let sources = Arc::new(
        SourceHost::new(
            builtin,
            plugins,
            &data_dir,
            output_tx.clone(),
            Arc::clone(&settings),
        )
        .map_err(|error| error.to_string())?,
    );
    let initializing_sources = SourceShutdown(Arc::clone(&sources));
    let core = Arc::new(AppCore {
        session_update_gate: Mutex::new(()),
        artwork: artwork_cache::ArtworkCache::new(&data_dir, output_tx.clone()),
        settings,
        sources,
        streamer,
        diagnostics,
        media: media::MediaService::new()
            .map_err(|error| format!("Could not initialize media library: {error}"))?,
        updater: updater::UpdaterService::new(&data_dir)
            .map_err(|error| format!("Could not initialize updater: {error}"))?,
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
    let _source_shutdown = initializing_sources;
    if !reporting_enabled(&core) {
        core.analytics.wipe();
        core.reports.clear_pending();
    }
    let analytics_finished = core.analytics.start()?;
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
            let completion = requests::scope(permit.token.clone(), || {
                if let Err(error) = requests::check() {
                    return Completion::from(Err((error.code.to_owned(), error.message)));
                }
                dispatch(&method, &params, &worker_core, &worker_output)
            });
            let outcome = match &completion.result {
                Ok(_) => "ok",
                Err((code, _)) => code.as_str(),
            };
            worker_core.diagnostics.record("rpc", &method, format!("outcome={outcome} durationMs={}", started.elapsed().as_millis()));
            for effect in &completion.reporting { observe_module_reporting(&worker_core, &worker_output, effect); }
            observe_host_reporting(&worker_core, &worker_output, &method, &params, &completion.result);
            let result = &completion.result;
            if matches!(method.as_str(), "updater.check" | "updater.download" | "updater.install") {
                if let Err((_, message)) = &result {
                    worker_core.updater.request_failed(message);
                }
                let _ = worker_output.send(json!({"type":"event", "name":"updater.changed", "payload":worker_core.updater.state()}));
            }
            deliver_completion(completion, &id, &permit.token, &worker_output, &worker_core.diagnostics);
            drop(permit);
        }).map_err(|error| error.to_string())?;
    }
    core.sources.shutdown();
    core.analytics.shutdown(&analytics_finished);
    Ok(())
}

type DispatchResult = Result<(Value, Option<(&'static str, Value)>), (String, String)>;

fn deliver_completion(
    completion: Completion,
    id: &str,
    cancellation: &requests::Cancellation,
    output: &mpsc::Sender<Value>,
    diagnostics: &diagnostics::DiagnosticsService,
) {
    for (name, payload) in completion.required_events {
        let _ = output.send(json!({"type":"event","name":name,"payload":payload}));
    }
    let mut delivered = false;
    if !cancellation.cancelled() {
        match completion.result {
            Ok((value, event)) => {
                if let Some(("settings.changed", payload)) = &event {
                    let _ = output
                        .send(json!({"type":"event","name":"settings.changed","payload":payload}));
                }
                delivered = output
                    .send(json!({"type":"response","id":id,"ok":true,"result":value}))
                    .is_ok();
                if let Some((name, payload)) = event
                    && name != "settings.changed"
                {
                    let _ = output.send(json!({"type":"event","name":name,"payload":payload}));
                }
            }
            Err((code, message)) => {
                let _ = output.send(json!({"type":"response","id":id,"ok":false,"error":{"code":code,"message":message}}));
            }
        }
    }
    if let Some(receipt) = completion.receipt {
        let accepted =
            delivered && cancellation.await_acceptance(std::time::Duration::from_secs(10));
        let outcome = requests::scope(requests::Cancellation::default(), || {
            receipt.settle(accepted)
        });
        diagnostics.record(
            "session",
            "allocation-handoff",
            format!(
                "accepted={accepted} cleanup={}",
                outcome
                    .result
                    .as_ref()
                    .map_or_else(|error| error.code.as_str(), |()| "ok")
            ),
        );
        for (name, payload) in outcome.required_events {
            let _ = output.send(json!({"type":"event","name":name,"payload":payload}));
        }
    }
}

fn core_capabilities(core: &AppCore) -> Vec<&'static str> {
    let mut capabilities = vec![
        "plugins.v1",
        "sources.catalog.v1",
        "sources.v2",
        "settings",
        "catalogArtworkCache.v1",
        "nativeStreamer.v8",
        "nativeStreamer.ownedNvstNegotiation",
        "nativeStreamer.dynamicSurface",
        "nativeStreamer.acceptanceEvidence",
        "liveAcceptance.v1",
        "redactedDiagnostics",
        "mediaLibrary",
        "githubUpdateDiscovery",
        "discordRpc",
        "feedback",
        "bugReports",
        "automaticBugReports.v2",
    ];
    capabilities.extend_from_slice(core.sources.core_capabilities());
    capabilities
}

fn update_session_idle(occupancy: SessionOccupancy, streamer: &Value) -> bool {
    occupancy == SessionOccupancy::Idle
        && matches!(
            streamer["streamer"]["status"].as_str(),
            Some("stopped" | "error")
        )
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
) -> Completion {
    with_session_update_gate(
        method,
        &core.session_update_gate,
        || core.updater.installation_pending(),
        || {
            if let Some(completion) =
                core.sources
                    .dispatch_private(method, params, &requests::current())
            {
                return completion;
            }
            if method.starts_with("sources.") && method != "sources.catalog.page" {
                return core
                    .sources
                    .dispatch_sources(method, params, &requests::current());
            }

            if let Some(completion) =
                core.sources
                    .dispatch_builtin(method, params, &requests::current())
            {
                return completion;
            }
            let result = dispatch_host(method, params, core, output);
            if method == "settings.set" {
                core.sources.settings_changed();
            }
            Completion::from(result)
        },
    )
}

fn with_session_update_gate(
    method: &str,
    gate: &Mutex<()>,
    update_pending: impl FnOnce() -> bool,
    invoke: impl FnOnce() -> Completion,
) -> Completion {
    let session_transition = matches!(
        method,
        "session.create"
            | "session.claim"
            | "session.poll"
            | "session.active.get"
            | "streamer.start"
            | "streamer.prepare"
            | "sources.session.create"
            | "sources.session.claim"
            | "sources.session.reconcile"
            | "sources.session.poll"
            | "streamer.source.prepare"
    );
    let _session_update_guard = if session_transition || method == "updater.install" {
        match gate.try_lock() {
            Ok(guard) => Some(guard),
            Err(_) => {
                return Completion::from(Err((
                    "session_update_busy".into(),
                    "A session transition or update preparation is in progress".into(),
                )));
            }
        }
    } else {
        None
    };
    if session_transition && update_pending() {
        return Completion::from(Err((
            "update_pending".into(),
            "An update is waiting for OpenNOW to exit".into(),
        )));
    }
    invoke()
}

fn dispatch_host(
    method: &str,
    params: &Value,
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
) -> DispatchResult {
    match method {
        "plugins.list"
        | "plugins.install.inspect"
        | "plugins.install.commit"
        | "plugins.install.cancel"
        | "plugins.setEnabled"
        | "plugins.uninstall" => core
            .sources
            .dispatch_plugins(method, params, &requests::current())
            .map(|value| (value, None))
            .map_err(Into::into),
        "sources.catalog.page" => core
            .sources
            .catalog_page(params, &requests::current())
            .and_then(|page| {
                serde_json::to_value(page).map_err(|_| {
                    sources::contract::SourceError::new(
                        "catalog_invalid",
                        "Could not encode catalog page",
                    )
                })
            })
            .map(|value| (value, None))
            .map_err(Into::into),

        "core.hello" => {
            if params["protocolVersion"].as_i64() != Some(PROTOCOL_VERSION) {
                return Err((
                    "incompatible_protocol".to_owned(),
                    "Shell and core protocol versions differ".to_owned(),
                ));
            }
            Ok((
                json!({"protocolVersion":PROTOCOL_VERSION, "coreVersion":version::APPLICATION_VERSION, "capabilities":core_capabilities(core)}),
                None,
            ))
        }
        "app.status" => Ok((
            json!({"status":"ready", "version":version::APPLICATION_VERSION}),
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
            } else if key == "webrtcCompatibilityMode" {
                event["changes"] = json!({"allianceWebrtcCompatibility": settings.all()["allianceWebrtcCompatibility"]});
            } else if key == "allianceWebrtcCompatibility" {
                event["changes"] =
                    json!({"webrtcCompatibilityMode": settings.all()["webrtcCompatibilityMode"]});
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
        "artwork.resolve" => {
            let settings = core.settings.lock().expect("settings poisoned").all();
            core.artwork
                .resolve(params, &settings)
                .map(|value| (value, None))
                .map_err(|message| ("invalid_params".to_owned(), message))
        }
        "network.regions.ping" => network::ping_regions(params)
            .map(|value| (value, None))
            .map_err(|message| ("region_ping_failed".to_owned(), message)),
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
        "thanks.data.get" => Ok((core.thanks.data(), None)),
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
            if !update_session_idle(core.sources.session_occupancy(), &core.streamer.status()) {
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
                account: &core.sources.reporting_identity(),
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

fn observe_host_reporting(
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
    method: &str,
    params: &Value,
    result: &DispatchResult,
) {
    if method == "settings.set"
        && params["key"] == "automaticBugReports"
        && let Ok((value, _)) = result
    {
        observe_consent(core, &value["value"], &params["source"]);
    }
    if method == "streamer.start"
        && let Err((code, message)) = result
    {
        observe_module_reporting(
            core,
            output,
            &ReportingEffect::RpcFailure {
                method: method.into(),
                code: code.clone(),
                message: message.clone(),
            },
        );
    }
}

fn observe_module_reporting(
    core: &Arc<AppCore>,
    output: &mpsc::Sender<Value>,
    effect: &ReportingEffect,
) {
    match effect {
        ReportingEffect::LaunchRequested { params } => {
            let detail = core.bug_reports.observe_launch(params);
            core.diagnostics.record("activity", "game_launch", detail);
            track(
                core,
                "game_launch_requested",
                json!({"game_id":params["appId"], "game_title":params["title"], "store":params["store"], "zone":params["zone"]}),
            );
        }
        ReportingEffect::SessionStopped => core.bug_reports.observe_stop(),
        ReportingEffect::RuntimeObserved { capabilities } => {
            core.analytics.observe_runtime(capabilities)
        }
        ReportingEffect::SignedIn { restored } => observe_sign_in(core, *restored),
        ReportingEffect::SignedOut => core.analytics.observe_sign_out(),
        ReportingEffect::RpcFailure {
            method,
            code,
            message,
        } => {
            if let Some(incident) = bug_reports::BugReporter::rpc_incident(method, code, message) {
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
    }
}

fn reporting_enabled(core: &AppCore) -> bool {
    bug_reports::enabled(&core.settings.lock().expect("settings poisoned").all())
}

fn track(core: &AppCore, event: &str, props: Value) -> Value {
    if !core.sources.allows_legacy_reporting() {
        return json!({"accepted":false,"reason":"provider_privacy"});
    }
    if !reporting_enabled(core) {
        return json!({"accepted":false,"reason":"disabled"});
    }
    let install_id = match ensure_install_id(core) {
        Ok(install_id) => install_id,
        Err((_, message)) => return json!({"accepted":false,"reason":message}),
    };
    let identity = core.sources.reporting_identity();
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
    let identity = core.sources.reporting_identity();
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
    let account = core.sources.reporting_identity();
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

    struct DeliveryReceipt(Arc<Mutex<Vec<bool>>>);

    #[test]
    fn host_transition_gate_rejects_before_module_action_receipt_or_reporting() {
        let gate = Mutex::new(());
        let held = gate.lock().unwrap();
        let blocked = with_session_update_gate(
            "session.create",
            &gate,
            || panic!("update state must be checked under the acquired gate"),
            || panic!("blocked request must not reach a module"),
        );
        assert_eq!(blocked.result.unwrap_err().0, "session_update_busy");
        assert!(blocked.receipt.is_none());
        assert!(blocked.reporting.is_empty());
        assert!(blocked.required_events.is_empty());
        drop(held);
        let pending = with_session_update_gate(
            "session.create",
            &gate,
            || {
                assert!(gate.try_lock().is_err());
                true
            },
            || panic!("pending update must prevent provider allocation"),
        );
        assert_eq!(pending.result.unwrap_err().0, "update_pending");
        assert!(pending.receipt.is_none());
        assert!(pending.reporting.is_empty());
        let admitted = with_session_update_gate(
            "session.create",
            &gate,
            || false,
            || {
                assert!(gate.try_lock().is_err());
                Completion::from(Ok((json!({"invoked":true}), None)))
            },
        );
        assert_eq!(admitted.result.unwrap().0["invoked"], true);
        assert!(gate.try_lock().is_ok());
    }

    impl sources::contract::AllocationReceipt for DeliveryReceipt {
        fn settle(self: Box<Self>, accepted: bool) -> sources::contract::ReceiptOutcome {
            assert!(requests::check().is_ok());
            self.0.lock().unwrap().push(accepted);
            sources::contract::ReceiptOutcome {
                result: Ok(()),
                required_events: vec![("receipt.settled", json!({"accepted":accepted}))],
            }
        }
    }

    #[test]
    fn module_completion_preserves_settings_before_response_and_normal_events_after() {
        let directory = tempfile::tempdir().unwrap();
        let diagnostics = diagnostics::DiagnosticsService::new(directory.path()).unwrap();
        for (name, before) in [("settings.changed", true), ("auth.session.changed", false)] {
            let (output, received) = mpsc::channel();
            let value = json!({"key":"fixture","value":true});
            deliver_completion(
                Completion::from(Ok((value.clone(), Some((name, value))))),
                "fixture",
                &requests::Cancellation::default(),
                &output,
                &diagnostics,
            );
            let first = received.try_recv().unwrap();
            let second = received.try_recv().unwrap();
            assert_eq!(first["type"], if before { "event" } else { "response" });
            assert_eq!(second["type"], if before { "response" } else { "event" });
            assert!(received.try_recv().is_err());
        }
    }

    #[test]
    fn cancelled_module_completion_keeps_required_events_and_settles_uncancelled() {
        let directory = tempfile::tempdir().unwrap();
        let diagnostics = diagnostics::DiagnosticsService::new(directory.path()).unwrap();
        let requests = Arc::new(requests::Requests::default());
        let permit = requests.admit("create", "session.create").unwrap();
        requests.cancel("create");
        let outcomes = Arc::new(Mutex::new(Vec::new()));
        let (output, received) = mpsc::channel();
        let mut completion = Completion::from(Err(("fixture_failure".into(), "fixture".into())));
        completion
            .required_events
            .push(("session.cleanup.pending", json!({"code":"fixture"})));
        completion.receipt = Some(Box::new(DeliveryReceipt(Arc::clone(&outcomes))));
        requests::scope(permit.token.clone(), || {
            deliver_completion(completion, "create", &permit.token, &output, &diagnostics);
        });
        assert_eq!(*outcomes.lock().unwrap(), vec![false]);
        assert_eq!(
            received.try_recv().unwrap()["name"],
            "session.cleanup.pending"
        );
        assert_eq!(received.try_recv().unwrap()["name"], "receipt.settled");
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn module_receipt_accepts_only_delivered_acknowledged_allocation() {
        let directory = tempfile::tempdir().unwrap();
        let diagnostics = diagnostics::DiagnosticsService::new(directory.path()).unwrap();
        for delivered in [true, false] {
            let requests = Arc::new(requests::Requests::default());
            let permit = requests.admit("create", "session.create").unwrap();
            requests.acknowledge("create");
            let outcomes = Arc::new(Mutex::new(Vec::new()));
            let (output, received) = mpsc::channel();
            let received = delivered.then_some(received);
            let mut completion =
                Completion::from(Ok((json!({"session":{"sessionId":"seat"}}), None)));
            completion.receipt = Some(Box::new(DeliveryReceipt(Arc::clone(&outcomes))));
            deliver_completion(completion, "create", &permit.token, &output, &diagnostics);
            assert_eq!(*outcomes.lock().unwrap(), vec![delivered]);
            if let Some(received) = received {
                assert_eq!(received.try_recv().unwrap()["type"], "response");
                assert_eq!(received.try_recv().unwrap()["name"], "receipt.settled");
                assert!(received.try_recv().is_err());
            }
        }
    }

    #[test]
    fn updates_require_no_session_and_a_terminal_streamer() {
        for status in ["stopped", "error"] {
            assert!(update_session_idle(
                SessionOccupancy::Idle,
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
                SessionOccupancy::Idle,
                &json!({"streamer":{"status":status}})
            ));
        }
        for occupancy in [SessionOccupancy::InUse, SessionOccupancy::Unknown] {
            for status in ["stopped", "error", "streaming"] {
                assert!(!update_session_idle(
                    occupancy,
                    &json!({"streamer":{"status":status}})
                ));
            }
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
