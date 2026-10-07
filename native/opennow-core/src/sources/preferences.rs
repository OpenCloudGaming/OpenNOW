use super::contract::SourceError;
use opennow_plugin_api::media::{Chroma, RequestedVideo, VideoEncoding};
use opennow_plugin_api::provider as api;
use opennow_plugin_api::{BUILTIN_GFN_ID, PluginId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileOverrides {
    source: PluginId,
    account: Option<api::AccountKey>,
    values: BTreeMap<String, api::SettingValue>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    version: u32,
    generation: u64,
    selected_source: Option<PluginId>,
    builtin_enabled: bool,
    #[serde(default)]
    profiles: Vec<ProfileOverrides>,
}

pub struct SourcePreferences {
    path: PathBuf,
    state: Mutex<Result<Document, ()>>,
}

impl SourcePreferences {
    pub fn open(root: &Path) -> Self {
        let path = root.join("source-preferences-v2.json");
        let read = || -> Result<Document, ()> {
            let file = match fs::File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Document {
                        version: 1,
                        generation: 1,
                        selected_source: Some(
                            PluginId::new(BUILTIN_GFN_ID).expect("built-in identity"),
                        ),
                        builtin_enabled: true,
                        profiles: Vec::new(),
                    });
                }
                Err(_) => return Err(()),
            };
            let mut bytes = Vec::new();
            file.take(128 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| ())?;
            if bytes.len() > 128 * 1024 {
                return Err(());
            }
            let document: Document = serde_json::from_slice(&bytes).map_err(|_| ())?;
            if document.version != 1 || document.generation == 0 {
                return Err(());
            }
            Ok(document)
        };
        let state = read();
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    pub fn builtin_enabled(&self) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.as_ref().ok().map(|doc| doc.builtin_enabled))
            .unwrap_or(false)
    }

    pub fn selected(&self) -> Option<PluginId> {
        self.state.lock().ok().and_then(|state| {
            state
                .as_ref()
                .ok()
                .and_then(|doc| doc.selected_source.clone())
        })
    }

    pub fn generation(&self) -> u64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.as_ref().ok().map(|doc| doc.generation))
            .unwrap_or(0)
    }

    pub fn select(&self, source: PluginId) -> Result<(), SourceError> {
        self.save(|doc| doc.selected_source = Some(source))
    }

    pub fn enable_builtin(&self, enabled: bool) -> Result<(), SourceError> {
        self.save(|doc| {
            doc.builtin_enabled = enabled;
            if !enabled
                && doc
                    .selected_source
                    .as_ref()
                    .is_some_and(PluginId::is_builtin)
            {
                doc.selected_source = None;
            }
        })
    }

    pub fn effective(
        &self,
        source: &PluginId,
        account: Option<&api::AccountKey>,
        global: &Value,
    ) -> Result<Value, SourceError> {
        let state = self.state.lock().map_err(|_| failure())?;
        let document = state.as_ref().map_err(|_| failure())?;
        let mut result = global.clone();
        for scope in [None, account] {
            if let Some(profile) = document
                .profiles
                .iter()
                .find(|profile| profile.source == *source && profile.account.as_ref() == scope)
            {
                for (key, value) in &profile.values {
                    apply_override(&mut result, key, value)?;
                }
            }
        }
        Ok(result)
    }

    pub fn set_profile(
        &self,
        source: PluginId,
        account: Option<api::AccountKey>,
        request: &api::SetSetting,
    ) -> Result<(), SourceError> {
        let mut proof = json!({"resolution":"1920x1080"});
        apply_override(&mut proof, request.key.as_str(), &request.value)?;
        self.save_checked(Some(request.expected_revision), |document| {
            if let Some(profile) = document
                .profiles
                .iter_mut()
                .find(|profile| profile.source == source && profile.account == account)
            {
                profile
                    .values
                    .insert(request.key.as_str().into(), request.value.clone());
            } else {
                document.profiles.push(ProfileOverrides {
                    source,
                    account,
                    values: BTreeMap::from([(request.key.as_str().into(), request.value.clone())]),
                });
            }
        })
    }

    fn save(&self, mutate: impl FnOnce(&mut Document)) -> Result<(), SourceError> {
        self.save_checked(None, mutate)
    }

    fn save_checked(
        &self,
        expected: Option<u64>,
        mutate: impl FnOnce(&mut Document),
    ) -> Result<(), SourceError> {
        let mut state = self.state.lock().map_err(|_| failure())?;
        let mut document = state.as_ref().map_err(|_| failure())?.clone();
        if expected.is_some_and(|revision| revision != document.generation) {
            return Err(SourceError::new(
                "stale_settings",
                "Source settings changed before this request",
            ));
        }
        mutate(&mut document);
        if document.profiles.len() > 128
            || document
                .profiles
                .iter()
                .any(|profile| profile.values.len() > 16)
        {
            return Err(failure());
        }
        document.generation = document.generation.checked_add(1).ok_or_else(failure)?;
        let bytes = serde_json::to_vec(&document).map_err(|_| failure())?;
        if bytes.len() > 128 * 1024 {
            return Err(failure());
        }
        let temporary = self.path.with_extension("json.tmp");
        let write = || -> std::io::Result<()> {
            fs::create_dir_all(self.path.parent().expect("preferences parent"))?;
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(temporary, &self.path)?;
            #[cfg(unix)]
            fs::File::open(self.path.parent().expect("preferences parent"))?.sync_all()?;
            Ok(())
        };
        write().map_err(|_| failure())?;
        *state = Ok(document);
        Ok(())
    }
}

pub fn stream_preferences(settings: &Value) -> Result<api::StreamPreferences, SourceError> {
    let (width, height) = dimensions(settings)?;
    let encoding = match settings["codec"]
        .as_str()
        .unwrap_or("auto")
        .to_ascii_lowercase()
        .as_str()
    {
        "auto" => None,
        "h264" => Some(VideoEncoding::H264AnnexB),
        "h265" => Some(VideoEncoding::HevcAnnexB),
        "av1" => Some(VideoEncoding::Av1Obu),
        _ => return Err(invalid_profile()),
    };
    let fps = settings["fps"].as_u64().unwrap_or(60);
    if fps > 360 {
        return Err(invalid_profile());
    }
    let color = settings["colorQuality"].as_str().unwrap_or("8bit_420");
    let hdr = settings["enableHdr"].as_bool().unwrap_or(false);
    let video = RequestedVideo {
        width,
        height,
        encoding,
        fps: (fps > 0).then_some(fps as u32),
        bit_depth: if hdr || color.starts_with("10bit") {
            10
        } else {
            8
        },
        chroma: if color.ends_with("444") {
            Chroma::Yuv444
        } else {
            Chroma::Yuv420
        },
        hdr,
    };
    video.validate().map_err(|_| invalid_profile())?;
    let bitrate = settings["maxBitrateMbps"].as_f64().unwrap_or(75.0);
    if !bitrate.is_finite() || !(0.22..=200.0).contains(&bitrate) {
        return Err(invalid_profile());
    }
    Ok(api::StreamPreferences {
        video,
        bitrate_kbps: (bitrate * 1000.0).round() as u32,
    })
}

pub fn profile_view(settings: &Value, revision: u64) -> Result<api::SettingsView, SourceError> {
    let (width, height) = dimensions(settings)?;
    let integer = |key: &str, label: &str, value: i64, min, max| -> api::SettingDefinition {
        api::SettingDefinition {
            key: api::SettingKey::new(key).expect("profile key"),
            label: api::Text::new(label).expect("profile label"),
            control: api::SettingControl::Integer { min, max, step: 1 },
            value: api::SettingValue::Integer(value),
        }
    };
    let choice = |key: &str, label: &str, value: &str, values: &[&str]| -> api::SettingDefinition {
        api::SettingDefinition {
            key: api::SettingKey::new(key).expect("profile key"),
            label: api::Text::new(label).expect("profile label"),
            control: api::SettingControl::Choice {
                choices: api::List::new(
                    values
                        .iter()
                        .map(|value| api::SettingChoice {
                            value: api::Text::new(*value).expect("profile choice"),
                            label: api::Text::new(*value).expect("profile choice"),
                        })
                        .collect(),
                )
                .expect("bounded choices"),
            },
            value: api::SettingValue::Choice(
                api::Text::new(value)
                    .map_err(|_| invalid_profile())
                    .expect("validated profile choice"),
            ),
        }
    };
    let mut definitions = vec![
        integer("stream.width", "Requested width", width.into(), 320, 8192),
        integer(
            "stream.height",
            "Requested height",
            height.into(),
            240,
            8192,
        ),
        integer(
            "stream.fps",
            "Requested frame rate (0 = Auto)",
            settings["fps"].as_i64().unwrap_or(60),
            0,
            360,
        ),
        choice(
            "stream.codec",
            "Requested codec",
            settings["codec"].as_str().unwrap_or("auto"),
            &["auto", "h264", "h265", "av1"],
        ),
        choice(
            "stream.colorQuality",
            "Requested color quality",
            settings["colorQuality"].as_str().unwrap_or("8bit_420"),
            &["8bit_420", "10bit_420", "8bit_444", "10bit_444"],
        ),
    ];
    definitions.push(api::SettingDefinition {
        key: api::SettingKey::new("stream.hdr").unwrap(),
        label: api::Text::new("HDR").unwrap(),
        control: api::SettingControl::Boolean,
        value: api::SettingValue::Boolean(settings["enableHdr"] == true),
    });
    definitions.push(api::SettingDefinition {
        key: api::SettingKey::new("stream.bitrateMbps").unwrap(),
        label: api::Text::new("Maximum requested bitrate (Mbps)").unwrap(),
        control: api::SettingControl::Number {
            min: api::FiniteNumber::new(0.22).unwrap(),
            max: api::FiniteNumber::new(200.0).unwrap(),
            step: api::FiniteNumber::new(0.01).unwrap(),
        },
        value: api::SettingValue::Number(
            api::FiniteNumber::new(settings["maxBitrateMbps"].as_f64().unwrap_or(75.0))
                .map_err(|_| invalid_profile())?,
        ),
    });
    Ok(api::SettingsView {
        revision,
        settings: api::List::new(definitions).expect("bounded profile schema"),
    })
}

fn dimensions(settings: &Value) -> Result<(u32, u32), SourceError> {
    let resolution = settings["resolution"].as_str().unwrap_or("1920x1080");
    let (w, h) = resolution.split_once('x').ok_or_else(invalid_profile)?;
    Ok((
        w.parse().map_err(|_| invalid_profile())?,
        h.parse().map_err(|_| invalid_profile())?,
    ))
}

fn apply_override(
    settings: &mut Value,
    key: &str,
    value: &api::SettingValue,
) -> Result<(), SourceError> {
    match (key, value) {
        ("stream.width", api::SettingValue::Integer(width)) if (320..=8192).contains(width) => {
            let (_, h) = dimensions(settings)?;
            settings["resolution"] = json!(format!("{width}x{h}"));
        }
        ("stream.height", api::SettingValue::Integer(height)) if (240..=8192).contains(height) => {
            let (w, _) = dimensions(settings)?;
            settings["resolution"] = json!(format!("{w}x{height}"));
        }
        ("stream.fps", api::SettingValue::Integer(fps)) if (0..=360).contains(fps) => {
            settings["fps"] = json!(fps)
        }
        ("stream.codec", api::SettingValue::Choice(codec))
            if ["auto", "h264", "h265", "av1"].contains(&codec.as_str()) =>
        {
            settings["codec"] = json!(codec.as_str())
        }
        ("stream.colorQuality", api::SettingValue::Choice(color))
            if ["8bit_420", "10bit_420", "8bit_444", "10bit_444"].contains(&color.as_str()) =>
        {
            settings["colorQuality"] = json!(color.as_str())
        }
        ("stream.hdr", api::SettingValue::Boolean(hdr)) => settings["enableHdr"] = json!(hdr),
        ("stream.bitrateMbps", api::SettingValue::Number(bitrate))
            if (0.22..=200.0).contains(&bitrate.get()) =>
        {
            settings["maxBitrateMbps"] = json!(bitrate.get())
        }
        _ => return Err(invalid_profile()),
    }
    Ok(())
}

fn invalid_profile() -> SourceError {
    SourceError::new(
        "invalid_setting",
        "The requested source stream profile is invalid",
    )
}

fn failure() -> SourceError {
    SourceError::new(
        "source_preferences_unavailable",
        "Source preferences could not be read or saved",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_selection_and_optional_builtin_survive_restart() {
        let root = tempfile::tempdir().unwrap();
        let state = SourcePreferences::open(root.path());
        assert!(state.builtin_enabled());
        state.enable_builtin(false).unwrap();
        state
            .select(PluginId::new("org.example.provider").unwrap())
            .unwrap();
        let restored = SourcePreferences::open(root.path());
        assert!(!restored.builtin_enabled());
        assert_eq!(
            restored.selected().unwrap().as_str(),
            "org.example.provider"
        );
        assert_eq!(restored.generation(), 3);
    }

    #[test]
    fn profile_overrides_are_account_scoped_durable_and_revision_checked() {
        let root = tempfile::tempdir().unwrap();
        let state = SourcePreferences::open(root.path());
        let source = PluginId::new("org.example.provider").unwrap();
        let other = PluginId::new("org.other.provider").unwrap();
        let account = api::AccountKey {
            authority: api::AuthorityId::new("authority").unwrap(),
            account: api::AccountId::new("user").unwrap(),
        };
        let global = json!({"resolution":"1920x1080","codec":"auto","fps":60,"nativeVideoBackend":"hardware"});
        let mut request = api::SetSetting {
            scope: api::SettingsScope { account: None },
            expected_revision: 1,
            key: api::SettingKey::new("stream.width").unwrap(),
            value: api::SettingValue::Integer(1280),
        };
        state.set_profile(source.clone(), None, &request).unwrap();
        request.expected_revision = state.generation();
        request.key = api::SettingKey::new("stream.height").unwrap();
        request.value = api::SettingValue::Integer(720);
        state
            .set_profile(source.clone(), Some(account.clone()), &request)
            .unwrap();
        assert_eq!(
            state.effective(&source, None, &global).unwrap()["resolution"],
            "1280x1080"
        );
        assert_eq!(
            state.effective(&source, Some(&account), &global).unwrap()["resolution"],
            "1280x720"
        );
        assert_eq!(
            state.effective(&other, Some(&account), &global).unwrap(),
            global
        );
        let restored = SourcePreferences::open(root.path());
        assert_eq!(
            restored
                .effective(&source, Some(&account), &global)
                .unwrap()["resolution"],
            "1280x720"
        );
        assert_eq!(
            restored
                .set_profile(source.clone(), Some(account.clone()), &request)
                .unwrap_err()
                .code,
            "stale_settings"
        );
        request.expected_revision = restored.generation();
        request.key = api::SettingKey::new("nativeVideoBackend").unwrap();
        request.value = api::SettingValue::Choice(api::Text::new("software").unwrap());
        assert_eq!(
            restored
                .set_profile(source, Some(account), &request)
                .unwrap_err()
                .code,
            "invalid_setting"
        );
        assert_eq!(global["nativeVideoBackend"], "hardware");
    }
}
