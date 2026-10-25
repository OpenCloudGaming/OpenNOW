use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

fn command(base: &Path, profile: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
    command
        .env("OPENNOW_DATA_DIR", base)
        .env_remove("OPENNOW_PROFILE");
    if let Some(profile) = profile {
        command.env("OPENNOW_PROFILE", profile);
    }
    command
}

struct Core {
    child: Child,
    output: Receiver<Value>,
    serial: u64,
}

impl Core {
    fn start(base: &Path, profile: Option<&str>) -> Self {
        let mut child = command(base, profile)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut core = Self {
            child,
            output,
            serial: 0,
        };
        core.request("core.hello", json!({"protocolVersion":5,"shell":"qt"}));
        core
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.serial += 1;
        let id = format!("profile-test-{}", self.serial);
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{}",
            json!({
                "type":"request","id":id,"method":method,"params":params
            })
        )
        .unwrap();
        loop {
            let response = self
                .output
                .recv_timeout(Duration::from_secs(10))
                .expect("core response timed out");
            if response["type"] == "response" && response["id"] == id {
                assert_eq!(response["ok"], true, "{response}");
                return response;
            }
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn named_profiles_run_concurrently_with_independent_settings_and_device_ids() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path();
    let original = json!({"fps":90,"onboardingCompleted":true});
    std::fs::write(
        base.join("settings.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    let mut one = Core::start(base, Some("Player_1"));
    let mut two = Core::start(base, Some("Player_2"));
    one.request("settings.set", json!({"key":"fps","value":120}));
    two.request("settings.set", json!({"key":"fps","value":60}));
    let one_dir = base.join("profiles/Player_1");
    let two_dir = base.join("profiles/Player_2");
    let read =
        |path: &Path| -> Value { serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap() };
    assert_eq!(read(&one_dir.join("settings.json"))["fps"], 120);
    assert_eq!(read(&two_dir.join("settings.json"))["fps"], 60);
    assert_eq!(read(&base.join("settings.json")), original);
    let first_identity = read(&one_dir.join("device-identity.json"));
    assert_ne!(
        first_identity["id"],
        read(&two_dir.join("device-identity.json"))["id"]
    );

    let rejected = command(base, Some("Player_1"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("data directory is already in use"));
    assert!(rejected.stdout.is_empty());
    let readonly = command(base, Some("Player_1"))
        .arg("--graphics-preferences")
        .output()
        .unwrap();
    assert!(readonly.status.success());
    let default = Core::start(base, None);
    drop(default);
    drop(one);
    let mut restarted = Core::start(base, Some("Player_1"));
    restarted.request("settings.get", json!({}));
    assert_eq!(first_identity, read(&one_dir.join("device-identity.json")));
    assert_eq!(read(&one_dir.join("settings.json"))["fps"], 120);
}

#[test]
fn named_profile_graphics_preferences_use_the_base_override_once_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path();
    let selected = base.join("profiles/Player_1");
    std::fs::create_dir_all(&selected).unwrap();
    std::fs::write(
        base.join("settings.json"),
        r#"{"windowsGpuDeviceId":"default"}"#,
    )
    .unwrap();
    std::fs::write(
        selected.join("settings.json"),
        r#"{"windowsGpuDeviceId":"selected"}"#,
    )
    .unwrap();
    for explicit in [false, true] {
        let mut command = command(base, Some("Player_1"));
        command.arg("--graphics-preferences");
        if explicit {
            command.arg("--data-dir").arg(base);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["windowsGpuDeviceId"],
            "selected"
        );
        assert_eq!(std::fs::read_dir(&selected).unwrap().count(), 1);
    }
}

#[test]
fn invalid_standalone_profile_environment_fails_before_creating_files() {
    let directory = tempfile::tempdir().unwrap();
    for invalid in [
        "",
        "../escape",
        "a/b",
        "a\\b",
        "/absolute",
        "_bad",
        "has space",
        &"a".repeat(65),
    ] {
        for readonly in [false, true] {
            let mut command = command(directory.path(), Some(invalid));
            command.stdin(Stdio::null());
            if readonly {
                command.arg("--graphics-preferences");
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("OPENNOW_PROFILE must match"));
        }
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn named_profiles_do_not_follow_default_legacy_directory_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("opennow");
    let primary = directory.path().join("OpenNOW");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::write(
        legacy.join("settings.json"),
        r#"{"windowsGpuDeviceId":"legacy"}"#,
    )
    .unwrap();
    for profile in [None, Some("Player_1")] {
        let mut command = command(&primary, profile);
        command
            .env_remove("OPENNOW_DATA_DIR")
            .env("XDG_CONFIG_HOME", directory.path())
            .arg("--graphics-preferences");
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["windowsGpuDeviceId"],
            if profile.is_some() { "" } else { "legacy" }
        );
    }
    assert!(!primary.exists());
    std::fs::create_dir_all(primary.join("profiles/Player_1")).unwrap();
    for profile in [None, Some("Player_1")] {
        let mut command = command(&primary, profile);
        command
            .env_remove("OPENNOW_DATA_DIR")
            .env("XDG_CONFIG_HOME", directory.path())
            .arg("--graphics-preferences");
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["windowsGpuDeviceId"],
            if profile.is_some() { "" } else { "legacy" }
        );
    }
    std::fs::write(
        primary.join("settings.json"),
        r#"{"windowsGpuDeviceId":"primary"}"#,
    )
    .unwrap();
    let output = command(&primary, None)
        .env_remove("OPENNOW_DATA_DIR")
        .env("XDG_CONFIG_HOME", directory.path())
        .arg("--graphics-preferences")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["windowsGpuDeviceId"],
        "primary"
    );
}
