use crate::proxy::normalize_proxy_url;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const NATIVE_TRANSPORT: &str = "nvst";
const CONSOLE_POLICY_VERSION: &str = "qtConsoleModePolicyVersion";
const WINDOWS_GPU_DEVICE_ID: &str = "windowsGpuDeviceId";
const MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES: usize = 1024;
const MAXIMUM_BOOTSTRAP_SETTINGS_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadPolicy {
    ReadWrite,
    ReadOnly,
}

pub struct SettingsStore {
    path: PathBuf,
    values: Map<String, Value>,
    // Preserve fields owned by an older/newer Electron build so trying the Qt
    // client and then rolling back never destroys settings it does not know.
    // Unknown values are not exposed over the shell/core contract.
    passthrough: Map<String, Value>,
}

impl SettingsStore {
    pub fn load(data_dir: Option<PathBuf>) -> io::Result<Self> {
        Self::load_with_policy(data_dir, LoadPolicy::ReadWrite)
    }

    pub(super) fn windows_gpu_device_id_read_only(data_dir: Option<PathBuf>) -> io::Result<String> {
        let store = Self::load_with_policy(data_dir, LoadPolicy::ReadOnly)?;
        Ok(store.values[WINDOWS_GPU_DEVICE_ID]
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    fn load_with_policy(data_dir: Option<PathBuf>, policy: LoadPolicy) -> io::Result<Self> {
        let path = data_dir
            .unwrap_or_else(default_data_dir)
            .join("settings.json");
        let defaults = defaults();
        let mut values = defaults.clone();
        let mut passthrough = Map::new();
        let mut migrate_onboarding = false;
        if path.exists() {
            match read_persisted_settings(&path, policy) {
                Some(persisted) => {
                    migrate_onboarding = policy == LoadPolicy::ReadWrite
                        && !persisted.contains_key("onboardingCompleted");
                    if migrate_onboarding {
                        values.insert("onboardingCompleted".to_owned(), json!(true));
                    }
                    for (key, value) in persisted {
                        if defaults.contains_key(&key) {
                            let value = if key == "gameCollections" {
                                match normalize_game_collections(value) {
                                    Ok(value) => value,
                                    Err(_) if policy == LoadPolicy::ReadOnly => {
                                        defaults["gameCollections"].clone()
                                    }
                                    Err(error) => {
                                        return Err(io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            error,
                                        ));
                                    }
                                }
                            } else if policy == LoadPolicy::ReadWrite && key == "mouseAcceleration"
                            {
                                value.as_bool().map_or(value.clone(), |enabled| {
                                    Value::Number((if enabled { 100 } else { 1 }).into())
                                })
                            } else {
                                value
                            };
                            values.insert(key, value);
                        } else if key != "nativeHdrSupported" {
                            if policy == LoadPolicy::ReadWrite
                                && key == "sessionTimeRemainingDisplay"
                                && matches!(value.as_str(), Some("stats" | "both"))
                            {
                                values.insert(
                                    "showSessionTimeRemainingInStatsOverlay".to_owned(),
                                    Value::Bool(true),
                                );
                            }
                            passthrough.insert(key, value);
                        }
                    }
                }
                None => {
                    if policy == LoadPolicy::ReadWrite {
                        let corrupt_path = path.with_extension("json.corrupt");
                        let _ = fs::rename(&path, corrupt_path);
                    }
                }
            }
        }
        let mut store = Self {
            path,
            values,
            passthrough,
        };
        if policy == LoadPolicy::ReadWrite {
            store.migrate_native_fullscreen_shortcut();
        }
        // Old builds enabled automatic switching by default, so an existing
        // true value is not reliable evidence of opt-in. Reset that policy once;
        // subsequent explicit opt-ins survive every restart.
        let migrate_console_policy = policy == LoadPolicy::ReadWrite
            && store.passthrough.get(CONSOLE_POLICY_VERSION) != Some(&json!(1));
        if migrate_console_policy {
            store
                .values
                .insert("switchToConsoleOnPad".to_owned(), json!(false));
            store
                .passthrough
                .insert(CONSOLE_POLICY_VERSION.to_owned(), json!(1));
        }
        store.normalize();
        if policy == LoadPolicy::ReadWrite
            && (migrate_console_policy || migrate_onboarding)
            && store.path.exists()
        {
            store.save()?;
        }
        Ok(store)
    }

    pub fn all(&self) -> Value {
        Value::Object(self.values.clone())
    }

    pub fn set(&mut self, key: &str, mut value: Value) -> Result<Value, String> {
        if !defaults().contains_key(key) {
            return Err(format!("Unknown setting: {key}"));
        }
        if key == "audioOutputDevice" {
            validate_bounded_string(&value, key, 1024)?;
        }
        if key == WINDOWS_GPU_DEVICE_ID {
            validate_bounded_string(&value, key, MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES)?;
        }
        if key == "gameCollections" {
            value = normalize_game_collections(value)?;
        }
        if key == "sessionProxyUrl" {
            let raw = value
                .as_str()
                .ok_or_else(|| "sessionProxyUrl must be a string".to_owned())?
                .trim();
            value = if raw.is_empty() {
                Value::String(String::new())
            } else {
                Value::String(normalize_proxy_url(raw)?.normalized_url)
            };
        }
        if key == "sessionProxyEnabled" && value.as_bool() == Some(true) {
            let raw = self.values["sessionProxyUrl"].as_str().unwrap_or("");
            normalize_proxy_url(raw)?;
        }
        let previous_values = self.values.clone();
        self.values.insert(key.to_owned(), value);
        self.normalize();
        if key == "microphoneMode" && self.values.get(key) == Some(&json!("voice-activity")) {
            self.values
                .insert("microphoneDeviceId".to_owned(), json!(""));
        }
        if key == "themePack" {
            let light = matches!(self.values[key].as_str(), Some("bone" | "cobalt"));
            self.values.insert(
                "appTheme".to_owned(),
                json!(if light { "light" } else { "dark" }),
            );
            self.values
                .insert("themeAccentOverride".to_owned(), json!(false));
        } else if key == "appAccentColor" {
            self.values
                .insert("themeAccentOverride".to_owned(), json!(true));
        }
        if key == "launchInConsoleMode" && self.values.get(key) == Some(&json!(false)) {
            self.values
                .insert("switchToConsoleOnPad".to_owned(), json!(false));
        }
        if let Err(error) = self.save() {
            self.values = previous_values;
            return Err(format!("Could not save settings: {error}"));
        }
        Ok(self.values.get(key).cloned().unwrap_or(Value::Null))
    }

    pub fn reset(&mut self) -> Result<Value, String> {
        let previous_values = self.values.clone();
        self.values = defaults();
        self.values.insert(
            "onboardingCompleted".to_owned(),
            previous_values["onboardingCompleted"].clone(),
        );
        self.normalize();
        if let Err(error) = self.save() {
            self.values = previous_values;
            return Err(format!("Could not reset settings: {error}"));
        }
        Ok(self.all())
    }

    fn normalize(&mut self) {
        normalize_types(&mut self.values);
        if self.values["audioOutputDevice"]
            .as_str()
            .is_some_and(|device| device.len() > 1024 || device.contains('\0'))
        {
            self.values
                .insert("audioOutputDevice".to_owned(), json!(""));
        }
        if self.values[WINDOWS_GPU_DEVICE_ID]
            .as_str()
            .is_some_and(|device| {
                device.len() > MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES || device.contains('\0')
            })
        {
            self.values
                .insert(WINDOWS_GPU_DEVICE_ID.to_owned(), json!(""));
        }
        normalize_resolution(&mut self.values);
        normalize_choice(
            &mut self.values,
            "desktopBackground",
            &["art", "gradient", "solid", "custom"],
            "art",
        );
        clamp_integer(&mut self.values, "desktopBackgroundOpacity", 0, 100, 30);
        let background_image = self.values["desktopBackgroundImage"].as_str().unwrap_or("");
        if !background_image.is_empty()
            && (background_image.len() > 8192
                || !url::Url::parse(background_image).is_ok_and(|url| {
                    url.scheme() == "file"
                        && url.to_file_path().is_ok()
                        && url.query().is_none()
                        && url.fragment().is_none()
                }))
        {
            self.values
                .insert("desktopBackgroundImage".to_owned(), json!(""));
        }
        normalize_choice(
            &mut self.values,
            "aspectRatio",
            &["16:9", "16:10", "21:9", "32:9", "4:3"],
            "16:9",
        );
        normalize_choice(
            &mut self.values,
            "recordingResolution",
            &["720p", "1080p", "1440p"],
            "720p",
        );
        normalize_choice(&mut self.values, "streamClientMode", &["native"], "native");
        normalize_choice(
            &mut self.values,
            "nativeVideoBackend",
            &[
                "auto",
                "d3d11",
                "d3d12",
                "nvdec",
                "cuda",
                "vaapi",
                "v4l2",
                "vulkan",
                "videotoolbox",
                "software",
            ],
            "auto",
        );
        for key in ["nativeCloudGsyncMode", "nativeD3dFullscreenMode"] {
            normalize_choice(
                &mut self.values,
                key,
                &["auto", "disabled", "forced"],
                "auto",
            );
        }
        self.values.insert(
            "transportMode".to_owned(),
            Value::String(NATIVE_TRANSPORT.to_owned()),
        );
        normalize_choice(
            &mut self.values,
            "codec",
            &["auto", "av1", "h264", "h265"],
            "auto",
        );
        normalize_choice(
            &mut self.values,
            "fallbackCodec",
            &["auto", "h264", "h265"],
            "auto",
        );
        normalize_choice(
            &mut self.values,
            "colorQuality",
            &["8bit_420", "10bit_420", "8bit_444", "10bit_444"],
            "8bit_420",
        );
        normalize_choice(&mut self.values, "frameGeneration", &["off", "2x"], "off");
        normalize_choice(&mut self.values, "upscaling", &["off", "metalfx"], "off");
        clamp_integer(&mut self.values, "upscalingSharpness", 0, 15, 10);
        clamp_integer(&mut self.values, "upscalingDenoise", 0, 20, 0);
        for key in ["decoderPreference", "encoderPreference"] {
            normalize_choice(
                &mut self.values,
                key,
                &["auto", "hardware", "software"],
                "auto",
            );
        }
        normalize_choice(
            &mut self.values,
            "microphoneMode",
            &["disabled", "voice-activity"],
            "disabled",
        );
        normalize_choice(
            &mut self.values,
            "statsOverlayPosition",
            &["bottom-left", "bottom-right", "top-left", "top-right"],
            "bottom-left",
        );
        normalize_choice(
            &mut self.values,
            "appTheme",
            &["light", "dark", "auto"],
            "auto",
        );
        normalize_choice(
            &mut self.values,
            "appLanguage",
            &[
                "system", "de", "en", "es", "fr", "ja", "ko", "nl", "pl", "ro", "ru", "tr", "zh",
            ],
            "system",
        );
        normalize_choice(
            &mut self.values,
            "themePack",
            &[
                "default", "nocturne", "aurora", "kraft", "phosphor", "bone", "cobalt", "hibiscus",
                "chapel",
            ],
            "nocturne",
        );
        normalize_choice(
            &mut self.values,
            "appAccentColor",
            &["green", "blue", "violet", "amber", "rose", "coral", "white"],
            "green",
        );
        normalize_choice(
            &mut self.values,
            "updateChannel",
            &["stable", "nightly"],
            crate::version::update_channel(crate::version::APPLICATION_VERSION),
        );
        clamp_integer(&mut self.values, "mouseAcceleration", 1, 150, 1);
        clamp_integer(&mut self.values, "controllerLeftStickDeadzone", 0, 50, 5);
        clamp_integer(&mut self.values, "controllerRightStickDeadzone", 0, 50, 5);
        clamp_integer(
            &mut self.values,
            "controllerVibrationIntensity",
            0,
            100,
            100,
        );
        clamp_integer(&mut self.values, "fps", 30, 240, 60);
        clamp_integer(&mut self.values, "maxBitrateMbps", 1, 200, 75);
        clamp_integer(&mut self.values, "windowWidth", 960, 7680, 1400);
        clamp_integer(&mut self.values, "windowHeight", 540, 4320, 900);
        clamp_integer(&mut self.values, "recordingFps", 30, 60, 30);
        clamp_integer(&mut self.values, "replayBufferSeconds", 15, 120, 30);
        clamp_integer(&mut self.values, "replayBufferMemoryMiB", 64, 512, 256);
        clamp_integer(&mut self.values, "antiAfkReminderEveryMinutes", 1, 120, 15);
        clamp_integer(&mut self.values, "antiAfkReminderDurationSeconds", 1, 60, 5);
        clamp_integer(&mut self.values, "sessionClockShowEveryMinutes", 1, 240, 60);
        clamp_integer(
            &mut self.values,
            "sessionClockShowDurationSeconds",
            1,
            120,
            30,
        );
        clamp_number(&mut self.values, "posterSizeScale", 0.75, 1.5, 1.05);
        clamp_number(&mut self.values, "mouseSensitivity", 0.1, 3.0, 1.0);
        clamp_number(&mut self.values, "desktopUiScale", 0.85, 1.25, 1.0);
        clamp_number(&mut self.values, "statsOverlayScale", 0.85, 1.5, 1.0);
        clamp_number(&mut self.values, "statsOverlayOpacity", 40.0, 100.0, 85.0);
        normalize_optional_integer(&mut self.values, "recordingBitrateMbps", 1, 12);
        normalize_bounded_strings(&mut self.values);
        normalize_nested_settings(&mut self.values);
    }

    fn migrate_native_fullscreen_shortcut(&mut self) {
        let fullscreen = self
            .values
            .get("shortcutToggleFullscreen")
            .and_then(Value::as_str);
        let screenshot = self
            .values
            .get("shortcutScreenshot")
            .and_then(Value::as_str);
        if fullscreen == Some("F10") && screenshot == Some("F11") {
            self.values.insert(
                "shortcutToggleFullscreen".to_owned(),
                Value::String("F11".to_owned()),
            );
            self.values.insert(
                "shortcutScreenshot".to_owned(),
                Value::String("Ctrl+F11".to_owned()),
            );
            if self
                .values
                .get("statsOverlayPosition")
                .and_then(Value::as_str)
                == Some("bottom-left")
            {
                self.values.insert(
                    "statsOverlayPosition".to_owned(),
                    Value::String("top-right".to_owned()),
                );
            }
        }
    }

    fn save(&self) -> io::Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = self.path.with_extension("json.tmp");
        let backup = self.path.with_extension("json.bak");
        let mut persisted = self.passthrough.clone();
        persisted.extend(self.values.clone());
        let data = serde_json::to_vec_pretty(&persisted).map_err(io::Error::other)?;
        fs::write(&temporary, data)?;
        if self.path.exists() {
            let _ = fs::remove_file(&backup);
            fs::rename(&self.path, &backup)?;
        }
        if let Err(error) = fs::rename(&temporary, &self.path) {
            let _ = fs::rename(&backup, &self.path);
            return Err(error);
        }
        Ok(())
    }
}

fn read_persisted_settings(path: &Path, policy: LoadPolicy) -> Option<Map<String, Value>> {
    match policy {
        LoadPolicy::ReadWrite => fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok()),
        LoadPolicy::ReadOnly => {
            let mut data = Vec::new();
            fs::File::open(path)
                .ok()?
                .take(MAXIMUM_BOOTSTRAP_SETTINGS_BYTES + 1)
                .read_to_end(&mut data)
                .ok()?;
            if data.len() as u64 > MAXIMUM_BOOTSTRAP_SETTINGS_BYTES {
                return None;
            }
            serde_json::from_slice(&data).ok()
        }
    }
}

fn validate_bounded_string(value: &Value, key: &str, maximum_bytes: usize) -> Result<(), String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))?;
    if value.len() > maximum_bytes || value.contains('\0') {
        return Err(format!(
            "{key} must be at most {maximum_bytes} bytes without NUL characters"
        ));
    }
    Ok(())
}

fn normalize_game_collections(mut value: Value) -> Result<Value, String> {
    let collections = value
        .as_array_mut()
        .ok_or_else(|| "gameCollections must be an array".to_owned())?;
    if collections.len() > 100 {
        return Err("gameCollections must contain at most 100 collections".to_owned());
    }
    let mut collection_ids = HashSet::new();
    for (index, collection) in collections.iter_mut().enumerate() {
        let invalid = || format!("gameCollections[{index}] must contain id, name and gameIds");
        let collection = collection.as_object_mut().ok_or_else(invalid)?;
        if collection.len() != 3 {
            return Err(format!(
                "gameCollections[{index}] must contain only id, name and gameIds"
            ));
        }
        let id = collection
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if id.trim().is_empty() || id.chars().count() > 128 {
            return Err(format!(
                "gameCollections[{index}].id must be nonempty and at most 128 characters"
            ));
        }
        if !collection_ids.insert(id.to_owned()) {
            return Err(format!("gameCollections[{index}].id must be unique"));
        }
        let name = collection
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?
            .trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(format!(
                "gameCollections[{index}].name must be nonempty and at most 80 characters after trimming"
            ));
        }
        let name = name.to_owned();
        let game_ids = collection
            .get("gameIds")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if game_ids.len() > 10_000 {
            return Err(format!(
                "gameCollections[{index}].gameIds must contain at most 10000 IDs"
            ));
        }
        let mut seen = HashSet::new();
        for game_id in game_ids {
            let game_id = game_id
                .as_str()
                .ok_or_else(|| format!("gameCollections[{index}].gameIds must contain strings"))?;
            if game_id.trim().is_empty() || game_id.chars().count() > 128 {
                return Err(format!(
                    "gameCollections[{index}].gameIds must contain nonempty IDs of at most 128 characters"
                ));
            }
            if !seen.insert(game_id) {
                return Err(format!(
                    "gameCollections[{index}].gameIds must contain unique IDs"
                ));
            }
        }
        collection.insert("name".to_owned(), Value::String(name));
    }
    Ok(value)
}

fn normalize_resolution(values: &mut Map<String, Value>) {
    let valid = values["resolution"]
        .as_str()
        .and_then(|value| value.split_once('x'))
        .and_then(|(width, height)| Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?)))
        .is_some_and(|(width, height)| {
            (640..=7680).contains(&width)
                && (480..=4320).contains(&height)
                && width % 2 == 0
                && height % 2 == 0
        });
    if !valid {
        values.insert("resolution".to_owned(), json!("1920x1080"));
    }
}

pub fn resolve_data_dir(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir.unwrap_or_else(|| {
        let primary = default_data_dir();
        select_existing_data_dir(primary.clone(), legacy_data_dirs(&primary))
    })
}

fn select_existing_data_dir(
    primary: PathBuf,
    legacy_candidates: impl IntoIterator<Item = PathBuf>,
) -> PathBuf {
    if primary.exists() {
        return primary;
    }
    legacy_candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .unwrap_or(primary)
}

fn normalize_choice(values: &mut Map<String, Value>, key: &str, choices: &[&str], fallback: &str) {
    let valid = values
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|value| choices.contains(&value));
    if !valid {
        values.insert(key.to_owned(), Value::String(fallback.to_owned()));
    }
}

fn clamp_integer(
    values: &mut Map<String, Value>,
    key: &str,
    minimum: i64,
    maximum: i64,
    fallback: i64,
) {
    let value = values
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(fallback)
        .clamp(minimum, maximum);
    values.insert(key.to_owned(), Value::Number(value.into()));
}

fn clamp_number(
    values: &mut Map<String, Value>,
    key: &str,
    minimum: f64,
    maximum: f64,
    fallback: f64,
) {
    let value = values
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
        .clamp(minimum, maximum);
    values.insert(
        key.to_owned(),
        serde_json::Number::from_f64(value).map_or(Value::from(fallback), Value::Number),
    );
}

fn normalize_types(values: &mut Map<String, Value>) {
    let expected = defaults();
    for (key, default) in expected {
        let valid = values.get(&key).is_some_and(|value| match &default {
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            Value::Number(_) => value.is_number(),
            Value::Array(_) => value.is_array(),
            Value::Object(_) => value.is_object(),
            Value::Null => value.is_null() || value.is_number(),
        });
        if !valid {
            values.insert(key, default);
        }
    }
}

fn normalize_optional_integer(
    values: &mut Map<String, Value>,
    key: &str,
    minimum: i64,
    maximum: i64,
) {
    let Some(value) = values.get(key) else {
        return;
    };
    if value.is_null() {
        return;
    }
    let normalized = value.as_i64().map(|value| value.clamp(minimum, maximum));
    values.insert(
        key.to_owned(),
        normalized.map_or(Value::Null, |value| Value::Number(value.into())),
    );
}

fn normalize_bounded_strings(values: &mut Map<String, Value>) {
    for (key, maximum) in [
        ("region", 256_usize),
        ("sessionProxyUrl", 2_048),
        ("nativeStreamerExecutablePath", 2_048),
        ("microphoneDeviceId", 512),
        ("telemetryInstallId", 128),
        ("lastSeenReleaseHighlightsVersion", 128),
    ] {
        let value = values
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(maximum)
            .collect::<String>();
        values.insert(key.to_owned(), Value::String(value));
    }
    for key in [
        "shortcutToggleStats",
        "shortcutTogglePointerLock",
        "shortcutToggleFullscreen",
        "shortcutStopStream",
        "shortcutToggleAntiAfk",
        "shortcutToggleMicrophone",
        "shortcutScreenshot",
        "shortcutToggleRecording",
        "shortcutSaveClip",
    ] {
        let value = values
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect::<String>();
        values.insert(key.to_owned(), Value::String(value));
    }
    let favorites = values["favoriteGameIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.chars().take(128).collect::<String>())
        .filter(|value| !value.is_empty())
        .take(500)
        .map(Value::String)
        .collect();
    values.insert("favoriteGameIds".to_owned(), Value::Array(favorites));

    let hidden_games = values["hiddenGameIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.chars().take(128).collect::<String>())
        .filter(|value| !value.is_empty())
        .take(500)
        .map(Value::String)
        .collect();
    values.insert("hiddenGameIds".to_owned(), Value::Array(hidden_games));

    let tile_sizes = values["homeTileSizes"]
        .as_object()
        .into_iter()
        .flat_map(|entries| entries.iter())
        .filter_map(|(id, value)| {
            let id = id.chars().take(128).collect::<String>();
            let size = value.as_str()?;
            (!id.is_empty() && matches!(size, "square" | "wide"))
                .then(|| (id, Value::String(size.to_owned())))
        })
        .take(500)
        .collect();
    values.insert("homeTileSizes".to_owned(), Value::Object(tile_sizes));
}

fn normalize_nested_settings(values: &mut Map<String, Value>) {
    let mut interpolation = values["frameInterpolation"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let enabled = interpolation
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let factor = interpolation
        .get("factor")
        .and_then(Value::as_i64)
        .unwrap_or(2)
        .clamp(2, 4);
    let quality = interpolation
        .get("quality")
        .and_then(Value::as_i64)
        .unwrap_or(480)
        .clamp(120, 1_080);
    interpolation.clear();
    interpolation.insert("enabled".to_owned(), Value::Bool(enabled));
    interpolation.insert("factor".to_owned(), Value::Number(factor.into()));
    interpolation.insert("quality".to_owned(), Value::Number(quality.into()));
    values.insert(
        "frameInterpolation".to_owned(),
        Value::Object(interpolation),
    );

    let mut shader = values["videoShader"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut normalized = Map::new();
    normalized.insert(
        "enabled".to_owned(),
        Value::Bool(
            shader
                .remove("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        ),
    );
    for (key, fallback, minimum, maximum) in [
        ("sharpen", 40, 0, 100),
        ("saturation", 100, 0, 200),
        ("contrast", 100, 0, 200),
        ("brightness", 100, 0, 200),
        ("vibrance", 0, -100, 100),
        ("filmGrain", 0, 0, 100),
    ] {
        let value = shader
            .remove(key)
            .and_then(|value| value.as_i64())
            .unwrap_or(fallback)
            .clamp(minimum, maximum);
        normalized.insert(key.to_owned(), Value::Number(value.into()));
    }
    values.insert("videoShader".to_owned(), Value::Object(normalized));
}

fn default_data_dir() -> PathBuf {
    if let Some(path) = env::var_os("OPENNOW_DATA_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = env::var_os("APPDATA") {
        return PathBuf::from(path).join("OpenNOW");
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = env::var_os("HOME") {
        return PathBuf::from(path).join("Library/Application Support/OpenNOW");
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("OpenNOW");
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/OpenNOW")
}

fn legacy_data_dirs(primary: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let mut candidates = Vec::new();
        if let Some(parent) = primary.parent() {
            candidates.push(parent.join("opennow"));
        }
        candidates
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = primary;
        Vec::new()
    }
}

fn defaults() -> Map<String, Value> {
    json!({
        "onboardingCompleted":false,
        "resolution":"1920x1080", "aspectRatio":"16:9", "posterSizeScale":1.05,
        "fps":60, "frameGeneration":"off", "upscaling":"off", "upscalingSharpness":10, "upscalingDenoise":0,
        "maxBitrateMbps":75, "recordingBitrateMbps":null,
        "recordingResolution":"720p", "recordingFps":30, "streamClientMode":"native",
        "replayBufferEnabled":false, "replayBufferSeconds":30, "replayBufferMemoryMiB":256,
        "nativeVideoBackend":"auto", "nativeStreamerExecutablePath":"", "audioOutputDevice":"",
        "windowsGpuDeviceId":"",
        "nativeCloudGsyncMode":"auto", "nativeD3dFullscreenMode":"auto",
        "nativeExternalRenderer":false, "transportMode":"nvst", "showNativeStreamerStats":false,
        "codec":"auto", "fallbackCodec":"auto", "decoderPreference":"auto",
        "encoderPreference":"auto", "colorQuality":"8bit_420", "enableHdr":false, "region":"",
        "suppressTenBitWarning":false,
        "sessionProxyEnabled":false, "sessionProxyUrl":"", "clipboardPaste":false,
        "enableGyroscopeControls":false, "steamControllerCompatibilityMode":false,
        "nativeCursorOverlay":true, "mouseSensitivity":1, "mouseAcceleration":1,
        "shortcutToggleStats":"Ctrl+N", "shortcutTogglePointerLock":"F8",
        "shortcutToggleFullscreen":"F11", "shortcutStopStream":"Ctrl+Shift+Q",
        "shortcutToggleAntiAfk":"Ctrl+Shift+K", "shortcutToggleMicrophone":"Ctrl+Shift+M",
        "shortcutScreenshot":"Ctrl+F11", "shortcutToggleRecording":"F12",
        "shortcutSaveClip":"Ctrl+F12",
        "microphoneMode":"disabled", "microphoneDeviceId":"", "hideStreamButtons":false,
        "muteWhenOutOfFocus":false, "backgroundStreamReminder":false,
        "showAntiAfkIndicator":true, "antiAfkReminderEveryMinutes":15,
        "antiAfkReminderDurationSeconds":5, "showStatsOnLaunch":false,
        "statsOverlayPosition":"top-right", "hideServerSelector":false,
        "desktopUiScale":1.0, "statsOverlayScale":1.0, "statsOverlayOpacity":85,
        "themeAccentOverride":false,
        "statsShowFps":true, "statsShowRegion":true, "statsShowPing":true,
        "statsShowBitrate":true, "statsShowJitter":true, "statsShowDrops":true,
        "statsShowPacketLoss":true, "statsShowDecode":true, "statsShowLatency":true,
        "statsShowVideo":true, "statsShowClock":true, "statsShowGraphs":true,
        "appAccentColor":"green", "appTheme":"auto", "appLanguage":"system", "themePack":"nocturne", "translucentUI":false,
        "showTileLabels":true,
        "controllerMode":true, "controllerModePromptDismissed":false,
        "controllerLeftStickDeadzone":5, "controllerRightStickDeadzone":5,
        "controllerVibrationIntensity":100,
        "reducedMotion":false,
        "launchInConsoleMode":false, "consoleProfilePickerOnLaunch":true,
        "desktopRailCollapsed":true, "desktopSidebarHover":true, "desktopBackground":"art",
        "desktopBackgroundImage":"", "desktopBackgroundOpacity":30,
        "switchToConsoleOnPad":false, "leaveConsoleOnPointer":true,
        "autoFullScreen":true, "favoriteGameIds":[], "hiddenGameIds":[], "gameCollections":[], "homeTileSizes":{}, "sessionCounterEnabled":false,
        "showSessionReport":true, "showSessionTimeRemainingInStatsOverlay":false,
        "sessionClockShowEveryMinutes":60, "sessionClockShowDurationSeconds":30,
        "windowWidth":1400, "windowHeight":900, "keyboardLayout":"en-US",
        "gameLanguage":"en_US", "enablePersistingInGameSettings":true, "enableL4S":false,
        "identifyAsSteamDeck":false, "steamBigPictureMode":false,
        "enableCloudGsync":false, "discordRichPresence":false,
        "autoCheckForUpdates":true, "autoDownloadUpdates":false,
        "updateChannel":crate::version::update_channel(crate::version::APPLICATION_VERSION),
        "allowEscapeToExitFullscreen":false, "lastSeenReleaseHighlightsVersion":"",
        "videoShader":{"enabled":false,"sharpen":40,"saturation":100,"contrast":100,"brightness":100,"vibrance":0,"filmGrain":0},
        "frameInterpolation":{"enabled":false,"factor":2,"quality":480},
        "errorReportingConsent":"unset", "telemetryInstallId":""
    })
    .as_object()
    .cloned()
    .expect("settings defaults are an object")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn updater_preferences_are_independent_and_preserved() {
        let directory =
            env::temp_dir().join(format!("opennow-update-settings-{}", rand::random::<u64>()));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["autoDownloadUpdates"], json!(false));
        assert_eq!(
            store.all()["updateChannel"],
            json!(crate::version::update_channel(
                crate::version::APPLICATION_VERSION
            ))
        );
        store.set("autoCheckForUpdates", json!(false)).unwrap();
        store.set("autoDownloadUpdates", json!(true)).unwrap();
        store.set("updateChannel", json!("stable")).unwrap();
        let settings = SettingsStore::load(Some(directory.clone())).unwrap().all();
        assert_eq!(settings["autoCheckForUpdates"], json!(false));
        assert_eq!(settings["autoDownloadUpdates"], json!(true));
        assert_eq!(settings["updateChannel"], json!("stable"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_new_profile_stays_incomplete_across_unrelated_writes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-new-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["onboardingCompleted"], json!(false));
        assert!(!directory.join("settings.json").exists());
        for (key, value) in [
            ("launchInConsoleMode", json!(true)),
            ("windowWidth", json!(1600)),
            ("windowHeight", json!(1000)),
        ] {
            store.set(key, value).unwrap();
            store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(false));
        }
        store.set("onboardingCompleted", json!(true)).unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["onboardingCompleted"],
            json!(true)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_existing_profiles_migrate_and_persist_completion() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-existing-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for persisted in [
            json!({}),
            json!({"qtConsoleModePolicyVersion": 1, "switchToConsoleOnPad": true}),
        ] {
            fs::write(
                directory.join("settings.json"),
                serde_json::to_vec(&persisted).unwrap(),
            )
            .unwrap();
            let store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(true));
            if persisted.get("qtConsoleModePolicyVersion").is_some() {
                assert_eq!(store.all()["switchToConsoleOnPad"], json!(true));
            }
            let saved: Value =
                serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap())
                    .unwrap();
            assert_eq!(saved["onboardingCompleted"], json!(true));
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all()["onboardingCompleted"],
                json!(true)
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_explicit_values_survive_migration_and_reset() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-reset-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for completed in [false, true] {
            fs::write(
                directory.join("settings.json"),
                serde_json::to_vec(&json!({"onboardingCompleted": completed})).unwrap(),
            )
            .unwrap();
            let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(completed));
            store.set("windowWidth", json!(1600)).unwrap();
            store.set("launchInConsoleMode", json!(true)).unwrap();
            store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(completed));
            let reset = store.reset().unwrap();
            assert_eq!(reset["onboardingCompleted"], json!(completed));
            assert_eq!(reset["windowWidth"], json!(1400));
            assert_eq!(reset["launchInConsoleMode"], json!(false));
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all(),
                reset
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_malformed_files_are_new_profiles() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-corrupt-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for contents in ["{", "null", "[]", "true"] {
            fs::write(directory.join("settings.json"), contents).unwrap();
            let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(false));
            assert_eq!(
                fs::read_to_string(directory.join("settings.json.corrupt")).unwrap(),
                contents
            );
            fs::remove_file(directory.join("settings.json.corrupt")).unwrap();
            store.set("windowWidth", json!(1600)).unwrap();
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all()["onboardingCompleted"],
                json!(false)
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_requires_boolean_values() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-types-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for invalid in [json!("true"), json!(1), json!(null), json!([]), json!({})] {
            fs::write(
                directory.join("settings.json"),
                serde_json::to_vec(&json!({"onboardingCompleted": invalid})).unwrap(),
            )
            .unwrap();
            let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["onboardingCompleted"], json!(false));
            store.set("onboardingCompleted", json!(true)).unwrap();
            assert_eq!(
                store.set("onboardingCompleted", invalid).unwrap(),
                json!(false)
            );
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all()["onboardingCompleted"],
                json!(false)
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn background_stream_preferences_are_opt_in_and_persisted() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-background-stream-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for key in ["muteWhenOutOfFocus", "backgroundStreamReminder"] {
            assert_eq!(store.all()[key], json!(false));
            store.set(key, json!(true)).unwrap();
        }
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for key in ["muteWhenOutOfFocus", "backgroundStreamReminder"] {
            assert_eq!(store.all()[key], json!(true));
            for invalid in [json!("true"), json!(1), json!(null), json!([])] {
                assert_eq!(store.set(key, invalid).unwrap(), json!(false));
            }
        }
        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["muteWhenOutOfFocus"], json!(false));
        assert_eq!(store.all()["backgroundStreamReminder"], json!(false));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ten_bit_warning_opt_out_is_typed_persisted_and_resettable() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-ten-bit-warning-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["suppressTenBitWarning"], json!(false));
        store.set("suppressTenBitWarning", json!(true)).unwrap();
        store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["suppressTenBitWarning"], json!(true));
        for invalid in [json!("true"), json!(1), json!(null), json!([])] {
            assert_eq!(
                store.set("suppressTenBitWarning", invalid).unwrap(),
                json!(false)
            );
        }
        store.set("suppressTenBitWarning", json!(true)).unwrap();
        assert_eq!(
            store.reset().unwrap()["suppressTenBitWarning"],
            json!(false)
        );
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["suppressTenBitWarning"],
            json!(false)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_save_and_reset_failures_preserve_previous_state() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-onboarding-save-failure-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for completed in [false, true] {
            store.set("onboardingCompleted", json!(completed)).unwrap();
            store.set("windowWidth", json!(1600)).unwrap();
            let original = store.all();
            let persisted = fs::read(directory.join("settings.json")).unwrap();
            fs::create_dir(directory.join("settings.json.tmp")).unwrap();
            assert!(store.set("onboardingCompleted", json!(!completed)).is_err());
            assert_eq!(store.all(), original);
            assert!(store.reset().is_err());
            assert_eq!(store.all(), original);
            assert_eq!(
                fs::read(directory.join("settings.json")).unwrap(),
                persisted
            );
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all(),
                original
            );
            fs::remove_dir(directory.join("settings.json.tmp")).unwrap();
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn onboarding_migration_save_failure_leaves_existing_file_intact() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("opennow-onboarding-migration-failure-{unique}"));
        fs::create_dir_all(directory.join("settings.json.tmp")).unwrap();
        let persisted = br#"{"qtConsoleModePolicyVersion":1}"#;
        fs::write(directory.join("settings.json"), persisted).unwrap();
        assert!(SettingsStore::load(Some(directory.clone())).is_err());
        assert_eq!(
            fs::read(directory.join("settings.json")).unwrap(),
            persisted
        );
        fs::remove_dir(directory.join("settings.json.tmp")).unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["onboardingCompleted"],
            json!(true)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replay_is_opt_in_bounded_and_persisted() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-replay-settings-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["replayBufferEnabled"], json!(false));
        assert_eq!(store.all()["replayBufferSeconds"], json!(30));
        assert_eq!(store.all()["replayBufferMemoryMiB"], json!(256));
        assert_eq!(store.all()["shortcutToggleRecording"], json!("F12"));
        assert_eq!(store.all()["shortcutSaveClip"], json!("Ctrl+F12"));
        store.set("replayBufferEnabled", json!(true)).unwrap();
        store.set("replayBufferSeconds", json!(999)).unwrap();
        store.set("replayBufferMemoryMiB", json!(1)).unwrap();
        store.set("shortcutSaveClip", json!("Alt+F12")).unwrap();
        let mut reloaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(reloaded.all()["replayBufferEnabled"], json!(true));
        assert_eq!(reloaded.all()["replayBufferSeconds"], json!(120));
        assert_eq!(reloaded.all()["replayBufferMemoryMiB"], json!(64));
        assert_eq!(reloaded.all()["shortcutSaveClip"], json!("Alt+F12"));
        reloaded.set("replayBufferSeconds", json!(-1)).unwrap();
        reloaded.set("replayBufferMemoryMiB", json!(9999)).unwrap();
        assert_eq!(reloaded.all()["replayBufferSeconds"], json!(15));
        assert_eq!(reloaded.all()["replayBufferMemoryMiB"], json!(512));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn microphone_is_opt_in_and_open_mode_survives_reload() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-microphone-policy-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        store
            .set("audioOutputDevice", json!("Selected headphones"))
            .unwrap();
        assert_eq!(store.all()["microphoneMode"], json!("disabled"));
        store
            .set("microphoneDeviceId", json!("old-device-id"))
            .unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["microphoneDeviceId"],
            json!("old-device-id")
        );
        assert_eq!(
            store
                .set("microphoneMode", json!("voice-activity"))
                .unwrap(),
            json!("voice-activity")
        );
        let mut reloaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(reloaded.all()["microphoneMode"], json!("voice-activity"));
        assert_eq!(reloaded.all()["microphoneDeviceId"], json!(""));
        assert_eq!(
            reloaded.all()["audioOutputDevice"],
            json!("Selected headphones")
        );
        assert_eq!(
            reloaded
                .set("microphoneMode", json!("push-to-talk"))
                .unwrap(),
            json!("disabled")
        );
        assert_eq!(
            reloaded.set("microphoneMode", json!("invalid")).unwrap(),
            json!("disabled")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn audio_output_device_is_persisted_and_validated() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-audio-settings-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["audioOutputDevice"], json!(""));
        let name = "USB Headphones – Audio";
        assert_eq!(
            store.set("audioOutputDevice", json!(name)).unwrap(),
            json!(name)
        );
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["audioOutputDevice"], json!(name));
        for invalid in [
            json!(42),
            json!(null),
            json!("speaker\0other"),
            json!("é".repeat(513)),
        ] {
            assert!(store.set("audioOutputDevice", invalid).is_err());
            assert_eq!(store.all()["audioOutputDevice"], json!(name));
        }
        store.set("audioOutputDevice", json!("")).unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["audioOutputDevice"],
            json!("")
        );
        let mut persisted: Value =
            serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap()).unwrap();
        persisted["audioOutputDevice"] = json!("bad\0device");
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&persisted).unwrap(),
        )
        .unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["audioOutputDevice"],
            json!("")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn theme_packs_and_accent_overrides_are_atomic_and_persisted() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-theme-policy-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for pack in [
            "nocturne", "aurora", "kraft", "phosphor", "bone", "cobalt", "hibiscus", "chapel",
        ] {
            store.set("appAccentColor", json!("rose")).unwrap();
            assert_eq!(store.all()["themeAccentOverride"], json!(true));
            store.set("themePack", json!(pack)).unwrap();
            let mut loaded = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(loaded.all()["themePack"], json!(pack));
            assert_eq!(loaded.all()["themeAccentOverride"], json!(false));
            assert_eq!(
                loaded.all()["appTheme"],
                json!(if matches!(pack, "bone" | "cobalt") {
                    "light"
                } else {
                    "dark"
                })
            );
            loaded.set("appTheme", json!("auto")).unwrap();
            loaded.set("appAccentColor", json!("violet")).unwrap();
            store = SettingsStore::load(Some(directory.clone())).unwrap();
            assert_eq!(store.all()["appTheme"], json!("auto"));
            assert_eq!(store.all()["themePack"], json!(pack));
            assert_eq!(store.all()["themeAccentOverride"], json!(true));
            assert_eq!(store.all()["appAccentColor"], json!("violet"));
        }
        let before = store.all();
        fs::create_dir(directory.join("settings.json.tmp")).unwrap();
        assert!(store.set("themePack", json!("bone")).is_err());
        assert_eq!(store.all(), before);
        fs::remove_dir(directory.join("settings.json.tmp")).unwrap();
        store.set("themeAccentOverride", json!(false)).unwrap();
        let before = store.all();
        fs::create_dir(directory.join("settings.json.tmp")).unwrap();
        assert!(store.set("appAccentColor", json!("green")).is_err());
        assert_eq!(store.all(), before);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn in_game_settings_persistence_survives_restart_and_resets() {
        let directory = tempfile::tempdir().unwrap();
        let load = || SettingsStore::load(Some(directory.path().to_owned())).unwrap();
        let mut store = load();
        assert_eq!(store.all()["enablePersistingInGameSettings"], true);
        fs::write(directory.path().join("settings.json"), br#"{"fps":120}"#).unwrap();
        store = load();
        assert_eq!(store.all()["enablePersistingInGameSettings"], true);
        for enabled in [true, false, true] {
            store
                .set("enablePersistingInGameSettings", json!(enabled))
                .unwrap();
            store = load();
            assert_eq!(store.all()["enablePersistingInGameSettings"], enabled);
            store.set("fps", json!(120)).unwrap();
            store = load();
            assert_eq!(store.all()["enablePersistingInGameSettings"], enabled);
        }
        fs::create_dir(directory.path().join("settings.json.tmp")).unwrap();
        assert!(
            store
                .set("enablePersistingInGameSettings", json!(false))
                .is_err()
        );
        assert_eq!(store.all()["enablePersistingInGameSettings"], true);
        assert_eq!(load().all()["enablePersistingInGameSettings"], true);
        fs::remove_dir(directory.path().join("settings.json.tmp")).unwrap();
        store
            .set("enablePersistingInGameSettings", json!(false))
            .unwrap();
        store.reset().unwrap();
        assert_eq!(load().all()["enablePersistingInGameSettings"], true);
    }

    #[test]
    fn steam_big_picture_is_opt_in_and_persists_independently() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-big-picture-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], false);
        store.set("launchInConsoleMode", json!(true)).unwrap();
        store.set("controllerMode", json!(true)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], false);
        store.set("steamBigPictureMode", json!(true)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], true);
        store.set("launchInConsoleMode", json!(false)).unwrap();
        store.set("controllerMode", json!(false)).unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], true);
        store.set("steamBigPictureMode", json!(false)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], false);
        store.set("steamBigPictureMode", json!(true)).unwrap();
        store.reset().unwrap();
        assert_eq!(store.all()["steamBigPictureMode"], false);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn game_collections_persist_and_survive_unrelated_changes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-collections-persistence-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["gameCollections"], json!([]));
        let collections = json!([
            {"id":"stable-b", "name":"  Favorites  ", "gameIds":["game-b", "game-a"]},
            {"id":"stable-a", "name":"Favorites", "gameIds":["game-a"]},
            {"id":"empty", "name":"Empty", "gameIds":[]}
        ]);
        let mut expected = collections.clone();
        expected[0]["name"] = json!("Favorites");
        assert_eq!(
            store.set("gameCollections", collections.clone()).unwrap(),
            expected
        );
        store.set("fps", json!(120)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["gameCollections"], expected);
        let mut persisted: Value =
            serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap()).unwrap();
        persisted["gameCollections"] = collections;
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&persisted).unwrap(),
        )
        .unwrap();
        let loaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(loaded.all()["gameCollections"], expected);
        expected[0]["name"] = json!("Renamed");
        store.set("gameCollections", expected.clone()).unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["gameCollections"],
            expected
        );
        assert_eq!(store.reset().unwrap()["gameCollections"], json!([]));
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["gameCollections"],
            json!([])
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn game_collections_reject_invalid_updates_without_data_loss() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-collections-invalid-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        let collection = json!({"id":"stable", "name":"Keep", "gameIds":["game-a"]});
        store
            .set("gameCollections", json!([collection.clone()]))
            .unwrap();
        let original = store.all();
        let persisted = fs::read(directory.join("settings.json")).unwrap();
        let mut invalid = vec![
            json!(null),
            json!({}),
            json!("wrong"),
            json!(true),
            json!([null]),
            json!([[]]),
            json!([collection.clone(), collection.clone()]),
            json!([{"id":"stable", "name":"Keep", "gameIds":[], "unexpected":true}]),
        ];
        for (key, values) in [
            (
                "id",
                vec![
                    json!(null),
                    json!(42),
                    json!(""),
                    json!(" \t"),
                    json!("x".repeat(129)),
                ],
            ),
            (
                "name",
                vec![
                    json!(null),
                    json!(false),
                    json!(""),
                    json!(" \n"),
                    json!("界".repeat(81)),
                ],
            ),
            (
                "gameIds",
                vec![
                    json!(null),
                    json!({}),
                    json!([null]),
                    json!([1]),
                    json!([""]),
                    json!([" \t"]),
                    json!(["x".repeat(129)]),
                    json!(["game-a", "game-a"]),
                    json!(
                        (0..10_001)
                            .map(|id| format!("game-{id}"))
                            .collect::<Vec<_>>()
                    ),
                ],
            ),
        ] {
            let mut missing = collection.clone();
            missing.as_object_mut().unwrap().remove(key);
            invalid.push(json!([missing]));
            for value in values {
                let mut malformed = collection.clone();
                malformed[key] = value;
                invalid.push(json!([malformed]));
            }
        }
        invalid.push(json!(
            (0..101)
                .map(|id| json!({"id":format!("collection-{id}"), "name":"Name", "gameIds":[]}))
                .collect::<Vec<_>>()
        ));
        for value in invalid {
            let error = store.set("gameCollections", value).unwrap_err();
            assert!(error.contains("gameCollections"), "{error}");
            assert_eq!(store.all(), original);
            assert_eq!(
                fs::read(directory.join("settings.json")).unwrap(),
                persisted
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn game_collections_accept_inclusive_unicode_and_count_bounds() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-collections-bounds-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        let mut collections = (0..100)
            .map(|id| json!({"id":format!("collection-{id}"), "name":"Same name", "gameIds":[]}))
            .collect::<Vec<_>>();
        let mut game_ids = (0..9_999)
            .map(|id| format!("game-{id}"))
            .collect::<Vec<_>>();
        game_ids.push("界".repeat(128));
        collections[0] = json!({"id":"界".repeat(128), "name":"界".repeat(80), "gameIds":game_ids});
        let expected = json!(collections);
        assert_eq!(
            store.set("gameCollections", expected.clone()).unwrap(),
            expected
        );
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["gameCollections"],
            expected
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn game_collections_save_and_reset_failures_restore_previous_values() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-collections-save-failure-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        store
            .set(
                "gameCollections",
                json!([{"id":"stable", "name":"Keep", "gameIds":["game-a"]}]),
            )
            .unwrap();
        let original = store.all();
        let persisted = fs::read(directory.join("settings.json")).unwrap();
        fs::create_dir(directory.join("settings.json.tmp")).unwrap();
        assert!(
            store
                .set("gameCollections", json!([]))
                .unwrap_err()
                .contains("Could not save settings")
        );
        assert_eq!(store.all(), original);
        assert!(
            store
                .reset()
                .unwrap_err()
                .contains("Could not reset settings")
        );
        assert_eq!(store.all(), original);
        assert_eq!(
            fs::read(directory.join("settings.json")).unwrap(),
            persisted
        );
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all(),
            original
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn game_collections_invalid_persisted_data_is_not_rewritten() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-collections-load-failure-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for collections in [
            json!(null),
            json!([{"id":"stable", "name":"Keep", "gameIds":["game-a", "game-a"]}]),
        ] {
            let persisted = serde_json::to_vec(&json!({"gameCollections":collections})).unwrap();
            fs::write(directory.join("settings.json"), &persisted).unwrap();
            let error = SettingsStore::load(Some(directory.clone())).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("gameCollections"));
            assert_eq!(
                fs::read(directory.join("settings.json")).unwrap(),
                persisted
            );
            assert!(!directory.join("settings.json.bak").exists());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn console_switching_is_opt_in_and_manual_desktop_survives_reload() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-console-policy-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&json!({
                "launchInConsoleMode": true,
                "switchToConsoleOnPad": true,
                "unrelatedLegacySetting": "preserve"
            }))
            .unwrap(),
        )
        .unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["switchToConsoleOnPad"], json!(false));
        assert_eq!(store.all()["launchInConsoleMode"], json!(true));
        store.set("switchToConsoleOnPad", json!(true)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(
            store.all()["switchToConsoleOnPad"],
            json!(true),
            "do not migrate a new opt-in twice"
        );
        // A failed atomic save must not change the in-memory policy either.
        let blocked_temporary = directory.join("settings.json.tmp");
        fs::create_dir(&blocked_temporary).unwrap();
        assert!(store.set("launchInConsoleMode", json!(false)).is_err());
        assert_eq!(store.all()["launchInConsoleMode"], json!(true));
        assert_eq!(store.all()["switchToConsoleOnPad"], json!(true));
        fs::remove_dir(blocked_temporary).unwrap();
        store.set("launchInConsoleMode", json!(false)).unwrap();
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["launchInConsoleMode"], json!(false));
        assert_eq!(store.all()["switchToConsoleOnPad"], json!(false));
        store.set("launchInConsoleMode", json!(true)).unwrap();
        assert_eq!(
            store.all()["switchToConsoleOnPad"],
            json!(false),
            "manual console is not automatic opt-in"
        );
        let persisted: Value =
            serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap()).unwrap();
        assert_eq!(persisted["unrelatedLegacySetting"], json!("preserve"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn hdr_opt_in_persists_but_runtime_output_capability_does_not() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-hdr-settings-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["enableHdr"], false);
        assert!(store.set("nativeHdrSupported", json!(true)).is_err());
        assert_eq!(store.set("enableHdr", json!(true)).unwrap(), true);
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["enableHdr"],
            true
        );
        let path = directory.join("settings.json");
        let mut persisted: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        persisted["nativeHdrSupported"] = json!(true);
        fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();
        let mut loaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert!(loaded.all().get("nativeHdrSupported").is_none());
        loaded.set("enableHdr", json!(true)).unwrap();
        let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert!(persisted.get("nativeHdrSupported").is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn custom_background_preferences_are_bounded_and_persisted() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-background-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["desktopBackgroundImage"], json!(""));
        assert_eq!(store.all()["desktopBackgroundOpacity"], json!(30));
        assert_eq!(
            store.set("desktopBackground", json!("custom")).unwrap(),
            json!("custom")
        );

        let image = url::Url::from_file_path(directory.join("写真 #1.png"))
            .unwrap()
            .to_string();
        assert_eq!(
            store.set("desktopBackgroundImage", json!(image)).unwrap(),
            json!(image)
        );
        for (input, expected) in [
            (json!(-10), 0),
            (json!(110), 100),
            (json!(0), 0),
            (json!(65), 65),
            (json!(null), 30),
            (json!("50"), 30),
        ] {
            assert_eq!(
                store.set("desktopBackgroundOpacity", input).unwrap(),
                json!(expected)
            );
        }
        store.set("desktopBackgroundOpacity", json!(65)).unwrap();
        let reloaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(reloaded.all()["desktopBackground"], json!("custom"));
        assert_eq!(reloaded.all()["desktopBackgroundImage"], json!(image));
        assert_eq!(reloaded.all()["desktopBackgroundOpacity"], json!(65));

        for invalid in [
            "https://example.com/image.png",
            "qrc:/image.png",
            "relative.png",
            "file:///image.png?download=1",
            "file:///image.png#fragment",
        ] {
            assert_eq!(
                store.set("desktopBackgroundImage", json!(invalid)).unwrap(),
                json!("")
            );
        }
        assert_eq!(
            store
                .set("desktopBackgroundImage", json!("x".repeat(8193)))
                .unwrap(),
            json!("")
        );
        store.reset().unwrap();
        assert_eq!(store.all()["desktopBackground"], json!("art"));
        assert_eq!(store.all()["desktopBackgroundImage"], json!(""));
        assert_eq!(store.all()["desktopBackgroundOpacity"], json!(30));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn automatic_fullscreen_defaults_on_and_preserves_opt_out() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-fullscreen-settings-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["autoFullScreen"], json!(true));
        store.set("autoFullScreen", json!(false)).unwrap();
        let mut loaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(loaded.all()["autoFullScreen"], json!(false));
        loaded.reset().unwrap();
        assert_eq!(loaded.all()["autoFullScreen"], json!(true));
        fs::write(directory.join("settings.json"), r#"{"fps":60}"#).unwrap();
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()["autoFullScreen"],
            json!(true)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn windows_gpu_device_preference_roundtrips_and_resets() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-windows-gpu-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()[WINDOWS_GPU_DEVICE_ID], json!(""));

        let device_id = "é".repeat(MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES / 2);
        assert_eq!(device_id.len(), MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES);
        assert_eq!(
            store
                .set(WINDOWS_GPU_DEVICE_ID, json!(device_id.clone()))
                .unwrap(),
            json!(device_id)
        );
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()[WINDOWS_GPU_DEVICE_ID],
            json!(device_id)
        );

        let reset = store.reset().unwrap();
        assert_eq!(reset[WINDOWS_GPU_DEVICE_ID], json!(""));
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()[WINDOWS_GPU_DEVICE_ID],
            json!("")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn windows_gpu_device_preference_rejects_invalid_set_values() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-windows-gpu-invalid-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        store
            .set(WINDOWS_GPU_DEVICE_ID, json!("valid-device"))
            .unwrap();

        for invalid in [
            json!(null),
            json!(false),
            json!(42),
            json!(["device"]),
            json!({"device":"id"}),
            json!("x".repeat(MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES + 1)),
            json!("device\0id"),
        ] {
            assert!(store.set(WINDOWS_GPU_DEVICE_ID, invalid).is_err());
            assert_eq!(store.all()[WINDOWS_GPU_DEVICE_ID], json!("valid-device"));
        }
        assert_eq!(
            SettingsStore::load(Some(directory.clone())).unwrap().all()[WINDOWS_GPU_DEVICE_ID],
            json!("valid-device")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn windows_gpu_device_preference_normalizes_invalid_saved_values() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-windows-gpu-saved-{unique}"));
        fs::create_dir_all(&directory).unwrap();

        for invalid in [
            json!(null),
            json!(123),
            json!("x".repeat(MAXIMUM_WINDOWS_GPU_DEVICE_ID_BYTES + 1)),
            json!("device\0id"),
        ] {
            fs::write(
                directory.join("settings.json"),
                serde_json::to_vec(&json!({"windowsGpuDeviceId":invalid})).unwrap(),
            )
            .unwrap();
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all()[WINDOWS_GPU_DEVICE_ID],
                json!("")
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn read_only_gpu_preference_load_never_migrates_or_renames_settings() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-windows-gpu-read-only-{unique}"));
        let path = directory.join("settings.json");
        fs::create_dir_all(&directory).unwrap();

        let legacy = br#"{"windowsGpuDeviceId":"legacy-device","mouseAcceleration":true,"gameCollections":[{"invalid":true}]}"#;
        fs::write(&path, legacy).unwrap();
        assert_eq!(
            SettingsStore::windows_gpu_device_id_read_only(Some(directory.clone())).unwrap(),
            "legacy-device"
        );
        assert_eq!(fs::read(&path).unwrap(), legacy);
        assert!(!directory.join("settings.json.bak").exists());
        assert!(!directory.join("settings.json.tmp").exists());

        let corrupt = b"{";
        fs::write(&path, corrupt).unwrap();
        assert_eq!(
            SettingsStore::windows_gpu_device_id_read_only(Some(directory.clone())).unwrap(),
            ""
        );
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(!directory.join("settings.json.corrupt").exists());

        let oversized = vec![b' '; MAXIMUM_BOOTSTRAP_SETTINGS_BYTES as usize + 1];
        fs::write(&path, &oversized).unwrap();
        assert_eq!(
            SettingsStore::windows_gpu_device_id_read_only(Some(directory.clone())).unwrap(),
            ""
        );
        assert_eq!(fs::read(&path).unwrap(), oversized);
        assert!(!directory.join("settings.json.corrupt").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persists_and_normalizes_settings() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-core-settings-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["launchInConsoleMode"], json!(false));
        assert_eq!(store.all()["transportMode"], json!("nvst"));
        assert_eq!(store.all()["desktopRailCollapsed"], json!(true));
        assert_eq!(store.all()["desktopSidebarHover"], json!(true));
        assert_eq!(store.all()["desktopBackground"], json!("art"));
        assert_eq!(
            store.set("desktopBackground", json!("gradient")).unwrap(),
            json!("gradient")
        );
        assert_eq!(
            store.set("desktopBackground", json!("invalid")).unwrap(),
            json!("art")
        );
        assert_eq!(
            store.set("desktopSidebarHover", json!(false)).unwrap(),
            json!(false)
        );
        assert_eq!(store.all()["switchToConsoleOnPad"], json!(false));
        assert_eq!(store.all()["leaveConsoleOnPointer"], json!(true));
        assert_eq!(store.all()["shortcutToggleFullscreen"], json!("F11"));
        assert_eq!(store.all()["shortcutScreenshot"], json!("Ctrl+F11"));
        assert_eq!(store.all()["statsOverlayPosition"], json!("top-right"));
        for key in [
            "statsShowFps",
            "statsShowRegion",
            "statsShowPing",
            "statsShowBitrate",
            "statsShowJitter",
            "statsShowDrops",
            "statsShowPacketLoss",
            "statsShowDecode",
            "statsShowLatency",
            "statsShowVideo",
            "statsShowClock",
            "statsShowGraphs",
        ] {
            assert_eq!(store.all()[key], json!(true));
            assert_eq!(store.set(key, json!(false)).unwrap(), json!(false));
        }
        assert_eq!(
            store.set("statsOverlayScale", json!(99)).unwrap(),
            json!(1.5)
        );
        assert_eq!(
            store.set("statsOverlayOpacity", json!(0)).unwrap(),
            json!(40.0)
        );
        assert_eq!(store.set("desktopUiScale", json!(0)).unwrap(), json!(0.85));
        let preferences = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(preferences.all()["desktopSidebarHover"], json!(false));
        assert_eq!(preferences.all()["statsShowFps"], json!(false));
        assert_eq!(preferences.all()["statsShowRegion"], json!(false));
        assert_eq!(preferences.all()["statsOverlayScale"], json!(1.5));
        assert_eq!(store.set("fps", json!(999)).unwrap(), json!(240));
        assert_eq!(store.set("maxBitrateMbps", json!(200)).unwrap(), json!(200));
        assert_eq!(
            store.set("launchInConsoleMode", json!(false)).unwrap(),
            json!(false)
        );
        assert_eq!(
            store.set("reducedMotion", json!(true)).unwrap(),
            json!(true)
        );
        let loaded = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(loaded.all()["fps"], json!(240));
        assert_eq!(loaded.all()["maxBitrateMbps"], json!(200));
        assert_eq!(loaded.all()["launchInConsoleMode"], json!(false));
        assert_eq!(loaded.all()["reducedMotion"], json!(true));
        assert!(store.set("notASetting", json!(true)).is_err());
        assert_eq!(store.set("codec", json!("invalid")).unwrap(), json!("auto"));
        assert_eq!(
            store.set("resolution", json!("3440x1440")).unwrap(),
            json!("3440x1440")
        );
        assert_eq!(
            store.set("resolution", json!("99999x1")).unwrap(),
            json!("1920x1080")
        );
        assert_eq!(
            store.set("controllerMode", json!("yes")).unwrap(),
            json!(true)
        );
        assert_eq!(
            store.set("mouseSensitivity", json!(99)).unwrap(),
            json!(3.0)
        );
        assert_eq!(
            store
                .set("frameInterpolation", json!({"enabled":true,"factor":99}))
                .unwrap(),
            json!({"enabled":true,"factor":4,"quality":480})
        );
        assert_eq!(
            store.set("themePack", json!("chapel")).unwrap(),
            json!("chapel")
        );
        assert_eq!(
            store.set("themePack", json!("unknown")).unwrap(),
            json!("nocturne")
        );
        assert_eq!(store.set("appLanguage", json!("de")).unwrap(), json!("de"));
        assert_eq!(
            store.set("appLanguage", json!("unsupported")).unwrap(),
            json!("system")
        );
        assert!(store.set("sessionProxyEnabled", json!(true)).is_err());
        assert_eq!(
            store
                .set("sessionProxyUrl", json!("proxy.example:8080"))
                .unwrap(),
            json!("http://proxy.example:8080/")
        );
        assert_eq!(
            store.set("sessionProxyEnabled", json!(true)).unwrap(),
            json!(true)
        );
        assert_eq!(
            store
                .set(
                    "homeTileSizes",
                    json!({"game-a":"wide","game-b":"giant","":"square"}),
                )
                .unwrap(),
            json!({"game-a":"wide"})
        );
        assert_eq!(
            store
                .set("hiddenGameIds", json!(["game-a", "", "game-b"]))
                .unwrap(),
            json!(["game-a", "game-b"])
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn frame_generation_is_limited_to_off_or_2x() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-frame-generation-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();

        assert_eq!(store.all()["frameGeneration"], json!("off"));
        assert_eq!(
            store.set("frameGeneration", json!("2x")).unwrap(),
            json!("2x")
        );
        assert_eq!(
            store.set("frameGeneration", json!("3x")).unwrap(),
            json!("off")
        );
        assert_eq!(
            store.set("frameGeneration", json!(true)).unwrap(),
            json!("off")
        );

        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&json!({"frameGeneration": "invalid"})).unwrap(),
        )
        .unwrap();
        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["frameGeneration"], json!("off"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn upscaling_defaults_off_and_persists_only_supported_choices() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-upscaling-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["upscaling"], json!("off"));
        assert_eq!(
            store.set("upscaling", json!("metalfx")).unwrap(),
            json!("metalfx")
        );
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["upscaling"], json!("metalfx"));
        assert_eq!(store.set("upscaling", json!("off")).unwrap(), json!("off"));
        for invalid in [json!("unknown"), json!(true), json!(null)] {
            assert_eq!(store.set("upscaling", invalid).unwrap(), json!("off"));
        }
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&json!({"upscaling": "invalid"})).unwrap(),
        )
        .unwrap();
        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["upscaling"], json!("off"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn upscaling_enhancement_defaults_bounds_and_persistence() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-upscaling-enhancement-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for (key, maximum, fallback) in
            [("upscalingSharpness", 15, 10), ("upscalingDenoise", 20, 0)]
        {
            assert_eq!(store.all()[key], json!(fallback));
            assert_eq!(store.set(key, json!(-1)).unwrap(), json!(0));
            assert_eq!(store.set(key, json!(maximum + 1)).unwrap(), json!(maximum));
            for invalid in [json!("7"), json!(true), json!(null), json!(1.5)] {
                assert_eq!(store.set(key, invalid).unwrap(), json!(fallback));
            }
            assert_eq!(store.set(key, json!(7)).unwrap(), json!(7));
        }
        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["upscalingSharpness"], json!(7));
        assert_eq!(store.all()["upscalingDenoise"], json!(7));
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec(&json!({"upscalingSharpness": 100, "upscalingDenoise": -2}))
                .unwrap(),
        )
        .unwrap();
        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["upscalingSharpness"], json!(15));
        assert_eq!(store.all()["upscalingDenoise"], json!(0));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_f10_f11_pair_migrates_to_native_f11_fullscreen() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-core-f11-migration-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("settings.json"),
            r#"{"shortcutToggleFullscreen":"F10","shortcutScreenshot":"F11"}"#,
        )
        .unwrap();

        let store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["shortcutToggleFullscreen"], json!("F11"));
        assert_eq!(store.all()["shortcutScreenshot"], json!("Ctrl+F11"));
        assert_eq!(store.all()["statsOverlayPosition"], json!("top-right"));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn existing_legacy_profile_wins_only_when_primary_is_absent() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("opennow-core-profile-{unique}"));
        let primary = root.join("OpenNOW");
        let legacy = root.join("legacy-opennow");

        fs::create_dir_all(&legacy).unwrap();
        assert_eq!(
            select_existing_data_dir(primary.clone(), [legacy.clone()]),
            legacy
        );

        fs::create_dir_all(&primary).unwrap();
        assert_eq!(select_existing_data_dir(primary.clone(), [legacy]), primary);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn historical_profile_spelling_respects_filesystem_case_sensitivity() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("opennow-core-profile-case-{unique}"));
        let primary = root.join("OpenNOW");
        let legacy = root.join("opennow");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), b"{}").unwrap();

        let expected = if primary.canonicalize().is_ok() {
            primary.clone()
        } else {
            legacy.clone()
        };
        let selected = select_existing_data_dir(primary, [legacy.clone()]);
        assert_eq!(selected, expected);
        assert_eq!(fs::read(selected.join("settings.json")).unwrap(), b"{}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn electron_settings_migrate_without_destroying_rollback_fields() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-core-legacy-settings-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "fps": 120,
                "mouseAcceleration": true,
                "transportMode": "webrtc",
                "sessionTimeRemainingDisplay": "both",
                "futureElectronSetting": {"enabled": true}
            }))
            .unwrap(),
        )
        .unwrap();

        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        assert_eq!(store.all()["fps"], json!(120));
        assert_eq!(store.all()["mouseAcceleration"], json!(100));
        assert_eq!(store.all()["transportMode"], json!("nvst"));
        assert_eq!(
            store.all()["showSessionTimeRemainingInStatsOverlay"],
            json!(true)
        );
        assert!(store.all().get("futureElectronSetting").is_none());
        store.set("codec", json!("h264")).unwrap();

        let persisted: Value =
            serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap()).unwrap();
        assert_eq!(persisted["futureElectronSetting"], json!({"enabled": true}));
        assert_eq!(persisted["transportMode"], json!("nvst"));
        assert_eq!(persisted["sessionTimeRemainingDisplay"], json!("both"));
        assert_eq!(persisted["mouseAcceleration"], json!(100));
        assert_eq!(persisted["codec"], json!("h264"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn every_legacy_transport_selector_normalizes_and_persists_as_nvst() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-core-transport-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();

        for legacy in [
            json!("webrtc"),
            json!("browser"),
            json!("auto"),
            json!(null),
        ] {
            assert_eq!(store.set("transportMode", legacy).unwrap(), json!("nvst"));
        }

        let persisted: Value =
            serde_json::from_slice(&fs::read(directory.join("settings.json")).unwrap()).unwrap();
        assert_eq!(persisted["transportMode"], json!("nvst"));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn controller_tuning_is_bounded_and_persisted() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("opennow-controller-tuning-{unique}"));
        let mut store = SettingsStore::load(Some(directory.clone())).unwrap();
        for (key, default, maximum) in [
            ("controllerLeftStickDeadzone", 5, 50),
            ("controllerRightStickDeadzone", 5, 50),
            ("controllerVibrationIntensity", 100, 100),
        ] {
            assert_eq!(store.all()[key], json!(default));
            assert_eq!(store.set(key, json!(-1)).unwrap(), json!(0));
            assert_eq!(store.set(key, json!(101)).unwrap(), json!(maximum));
            for invalid in [json!("bad"), json!(null), json!(false), json!(0.5)] {
                assert_eq!(store.set(key, invalid).unwrap(), json!(default));
            }
            store.set(key, json!(12)).unwrap();
            assert_eq!(
                SettingsStore::load(Some(directory.clone())).unwrap().all()[key],
                json!(12)
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
