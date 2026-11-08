use crate::{DispatchResult, settings::SettingsStore};
use serde_json::{Map, Value, json};
use std::sync::Mutex;

pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "settings.get" | "settings.set" | "settings.shortcuts.update" | "settings.reset"
    )
}

pub fn dispatch(
    store: &Mutex<SettingsStore>,
    method: &str,
    params: &Value,
    publish: impl FnOnce(&DispatchResult),
) -> DispatchResult {
    let mut settings = store.lock().expect("settings poisoned");
    let result = apply(&mut settings, method, params);
    if result.is_ok() {
        publish(&result);
    }
    result
}

fn apply(settings: &mut SettingsStore, method: &str, params: &Value) -> DispatchResult {
    match method {
        "settings.get" => Ok((
            json!({"settings":settings.all(), "keyboardLayouts":crate::language::keyboard_choices()}),
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
            let bindings = settings
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
            let values = settings
                .reset()
                .map_err(|message| ("settings_write_failed".to_owned(), message))?;
            Ok((
                json!({"settings":values}),
                Some(("settings.reset", json!({}))),
            ))
        }
        _ => unreachable!("not a settings RPC"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish_response;
    use std::sync::mpsc;
    use std::time::Duration;

    fn assert_ordered_pair(first: Value, second: Value) {
        let directory = tempfile::tempdir().unwrap();
        let settings = Mutex::new(SettingsStore::load(Some(directory.path().to_owned())).unwrap());
        let mut shell = settings.lock().unwrap().all();
        let (output, messages) = mpsc::channel();
        let (committed_tx, committed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (second_done_tx, second_done_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let settings = &settings;
            let output = &output;
            let first = &first;
            let second = &second;
            let first_worker = scope.spawn(move || {
                dispatch(settings, "settings.set", first, |result| {
                    committed_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    publish_response(output, "first", result);
                })
                .unwrap();
            });
            committed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let transaction_unlocked = settings.try_lock().is_ok();
            let second_worker = scope.spawn(move || {
                started_tx.send(()).unwrap();
                dispatch(settings, "settings.set", second, |result| {
                    publish_response(output, "second", result);
                })
                .unwrap();
                second_done_tx.send(()).unwrap();
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if transaction_unlocked {
                second_done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            release_tx.send(()).unwrap();
            first_worker.join().unwrap();
            second_worker.join().unwrap();
            let published: Vec<_> = messages.try_iter().collect();
            assert_eq!(published.len(), 4);
            assert_eq!(published[0]["name"], "settings.changed");
            assert_eq!(
                published[0]["payload"]["key"], first["key"],
                "later commit published first: {published:?}"
            );
            assert_eq!(published[1]["id"], "first");
            assert_eq!(published[2]["name"], "settings.changed");
            assert_eq!(published[2]["payload"]["key"], second["key"]);
            assert_eq!(published[3]["id"], "second");
            for pair in published.chunks_exact(2) {
                assert_eq!(pair[1]["ok"], true);
                assert_eq!(pair[1]["result"], pair[0]["payload"]);
                let event = &pair[0]["payload"];
                shell[event["key"].as_str().unwrap()] = event["value"].clone();
                if let Some(changes) = event["changes"].as_object() {
                    for (key, value) in changes {
                        shell[key] = value.clone();
                    }
                }
            }
            assert_eq!(shell, settings.lock().unwrap().all());
            assert_eq!(
                shell,
                SettingsStore::load(Some(directory.path().to_owned()))
                    .unwrap()
                    .all()
            );
            dispatch(settings, "settings.get", &json!({}), |result| {
                publish_response(output, "read", result);
            })
            .unwrap();
            assert_eq!(
                messages.recv_timeout(Duration::from_secs(5)).unwrap()["result"]["settings"],
                shell
            );
        });
    }

    #[test]
    fn concurrent_theme_and_accent_publish_in_commit_order() {
        let theme = json!({"key":"themePack","value":"bone"});
        let accent = json!({"key":"appAccentColor","value":"violet"});
        assert_ordered_pair(theme.clone(), accent.clone());
        assert_ordered_pair(accent, theme);
    }

    #[test]
    fn concurrent_compatibility_settings_publish_in_commit_order() {
        let mode = json!({"key":"webrtcCompatibilityMode","value":"on"});
        let legacy = json!({"key":"allianceWebrtcCompatibility","value":false});
        assert_ordered_pair(mode.clone(), legacy.clone());
        assert_ordered_pair(legacy, mode);
    }

    #[test]
    fn settings_failures_keep_state_and_error_publication_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Mutex::new(SettingsStore::load(Some(directory.path().to_owned())).unwrap());
        let before = settings.lock().unwrap().all();
        std::fs::create_dir(directory.path().join("settings.json.tmp")).unwrap();
        let (output, messages) = mpsc::channel();
        for (method, params, code) in [
            ("settings.set", json!({"key":"themePack"}), "invalid_params"),
            (
                "settings.set",
                json!({"key":"unknown","value":true}),
                "invalid_setting",
            ),
            (
                "settings.set",
                json!({"key":"themePack","value":"bone"}),
                "invalid_setting",
            ),
            (
                "settings.shortcuts.update",
                json!({"bindings":{"shortcutToggleStats":"F8"}}),
                "invalid_setting",
            ),
            ("settings.reset", json!({}), "settings_write_failed"),
        ] {
            let result = dispatch(&settings, method, &params, |_| {
                panic!("failed settings transaction published success")
            });
            assert_eq!(result.as_ref().unwrap_err().0, code);
            assert_eq!(settings.lock().unwrap().all(), before);
            publish_response(&output, "error", &result);
            let response = messages.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(response["type"], "response");
            assert_eq!(response["ok"], false);
            assert_eq!(response["error"]["code"], code);
            assert!(messages.try_recv().is_err());
        }
    }
}
