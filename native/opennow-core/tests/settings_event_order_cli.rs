use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn settings_events_precede_acknowledgements_and_other_events_keep_their_order() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .args(["--data-dir", directory.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender
                .send(serde_json::from_str::<Value>(&line).unwrap())
                .is_err()
            {
                break;
            }
        }
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut serial = 0;
        let mut request = |method: &str, params: Value| {
            serial += 1;
            let id = serial.to_string();
            writeln!(
                child.stdin.as_mut().unwrap(),
                "{}",
                json!({"type":"request","id":id,"method":method,"params":params})
            )
            .unwrap();
            id
        };
        let next = || {
            receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("core response timed out")
        };
        for (key, value, changes) in [
            (
                "webrtcCompatibilityMode",
                json!("on"),
                Some(json!({"allianceWebrtcCompatibility":true})),
            ),
            (
                "webrtcCompatibilityMode",
                json!("off"),
                Some(json!({"allianceWebrtcCompatibility":false})),
            ),
            (
                "webrtcCompatibilityMode",
                json!("auto"),
                Some(json!({"allianceWebrtcCompatibility":false})),
            ),
            (
                "allianceWebrtcCompatibility",
                json!(true),
                Some(json!({"webrtcCompatibilityMode":"on"})),
            ),
            (
                "allianceWebrtcCompatibility",
                json!(false),
                Some(json!({"webrtcCompatibilityMode":"off"})),
            ),
            ("gameLanguage", json!("es_419"), None),
            ("gameLanguage", json!("zh_Hant_TW"), None),
            ("launchInConsoleMode", json!(true), None),
            (
                "launchInConsoleMode",
                json!(false),
                Some(json!({"switchToConsoleOnPad":false})),
            ),
            (
                "themePack",
                json!("bone"),
                Some(json!({"appTheme":"light","themeAccentOverride":false})),
            ),
            (
                "microphoneMode",
                json!("voice-activity"),
                Some(json!({"microphoneDeviceId":""})),
            ),
        ] {
            let id = request("settings.set", json!({"key":key,"value":value}));
            let event = next();
            assert_eq!(
                event["type"], "event",
                "a queued next write must not start before this event"
            );
            assert_eq!(event["name"], "settings.changed");
            assert_eq!(event["payload"]["key"], key);
            assert_eq!(event["payload"]["value"], value);
            if let Some(changes) = changes {
                assert_eq!(event["payload"]["changes"], changes);
            }
            let response = next();
            assert_eq!(response["id"], id);
            assert_eq!(response["ok"], true);
            assert_eq!(response["result"], event["payload"]);
        }
        let id = request("settings.set", json!({"key":"gameLanguage","value":"auto"}));
        let rejected = next();
        assert_eq!(rejected["id"], id);
        assert_eq!(rejected["ok"], false);
        let id = request("settings.reset", json!({}));
        let response = next();
        assert_eq!(response["id"], id);
        assert_eq!(response["ok"], true);
        assert_eq!(next()["name"], "settings.reset");
    }));
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
