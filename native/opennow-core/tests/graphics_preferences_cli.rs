use serde_json::json;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn graphics_preferences_cli_prints_one_exact_json_line_without_mutating_settings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-graphics-cli-{unique}"));
    let path = directory.join("settings.json");
    fs::create_dir_all(&directory).unwrap();
    let persisted = serde_json::to_vec(&json!({
        "windowsGpuDeviceId":"gpu-ü-1",
        "mouseAcceleration":true,
        "futureSecret":"do-not-print"
    }))
    .unwrap();
    fs::write(&path, &persisted).unwrap();

    let mut explicit = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
    explicit
        .arg("--graphics-preferences")
        .arg("--data-dir")
        .arg(&directory);
    let mut environment = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
    environment
        .arg("--graphics-preferences")
        .env("OPENNOW_DATA_DIR", &directory);

    for mut command in [explicit, environment] {
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            output.stdout,
            "{\"version\":1,\"windowsGpuDeviceId\":\"gpu-ü-1\"}\n".as_bytes()
        );
        assert!(output.stderr.is_empty());
    }
    assert_eq!(fs::read(&path).unwrap(), persisted);
    assert!(!directory.join("settings.json.bak").exists());
    assert!(!directory.join("settings.json.tmp").exists());
    assert!(!directory.join("settings.json.corrupt").exists());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_dir_all(directory).unwrap();
}
