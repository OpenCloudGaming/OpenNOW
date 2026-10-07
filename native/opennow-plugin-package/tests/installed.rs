use opennow_plugin_package::{InstalledManifest, PackagePin, Role, current_target, verify};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

fn package() -> (tempfile::TempDir, std::path::PathBuf, InstalledManifest) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("version");
    fs::create_dir(&root).unwrap();
    let executable = fs::read(std::env::current_exe().unwrap()).unwrap();
    fs::write(root.join("control"), &executable).unwrap();
    fs::write(root.join("media"), &executable).unwrap();
    let digest = format!("{:x}", Sha256::digest(&executable));
    let manifest = json!({
        "schemaVersion":2,"protocolVersion":2,"id":"org.opennow.test.provider",
        "name":"Provider test","version":"1.0.0","publisher":"Test","description":"A test provider",
        "authKinds":["anonymous"],
        "capabilities":["auth.anonymous.v2","catalog.library.v2","catalog.details.v2","launch.v2","sessions.v2","media.worker.v1"],
        "entrypoints":{current_target():{"control":"control","media":"media"}},
        "files":[{"path":"control","sha256":digest},{"path":"media","sha256":digest}]
    });
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    (temp, root, serde_json::from_value(manifest).unwrap())
}

#[test]
fn both_roles_are_verified_and_package_clones_retain_the_pin() {
    let (_temp, root, manifest) = package();
    let verified = verify(&root, &manifest).unwrap();
    assert_eq!(
        verified.entrypoint(Role::Control),
        Some(root.join("control").as_path())
    );
    assert_eq!(
        verified.entrypoint(Role::Media),
        Some(root.join("media").as_path())
    );
    assert!(verified.entrypoint(Role::Catalog).is_none());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 3);
    let clone = verified.clone();
    drop(verified);
    assert_eq!(
        PackagePin::try_exclusive(&root).unwrap_err().code,
        "plugin_in_use"
    );
    drop(clone);
    let exclusive = PackagePin::try_exclusive(&root).unwrap();
    assert_eq!(PackagePin::shared(&root).unwrap_err().code, "plugin_in_use");
    drop(exclusive);
    assert!(PackagePin::shared(&root).is_ok());
}

#[test]
fn changed_media_role_is_rejected_even_when_control_is_unchanged() {
    let (_temp, root, manifest) = package();
    fs::write(root.join("media"), b"not a native executable").unwrap();
    assert!(verify(&root, &manifest).is_err());
}

#[test]
fn media_role_cannot_escape_the_installed_inventory() {
    let (_temp, root, manifest) = package();
    let mut value = serde_json::to_value(&manifest).unwrap();
    value["entrypoints"][current_target()]["media"] = json!("../outside");
    assert!(serde_json::from_value::<InstalledManifest>(value).is_err());
    fs::write(root.join("unlisted"), b"extra").unwrap();
    assert!(verify(&root, &manifest).is_err());
}

#[cfg(unix)]
#[test]
fn pin_and_payload_links_are_rejected() {
    use std::os::unix::fs::symlink;
    let (temp, root, manifest) = package();
    let victim = temp.path().join("victim");
    fs::write(&victim, b"unchanged").unwrap();
    drop(PackagePin::shared(&root).unwrap());
    let namespace = format!(
        "{:x}",
        Sha256::digest(
            temp.path()
                .canonicalize()
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
        )
    );
    let pin_path = temp
        .path()
        .parent()
        .unwrap()
        .join(".opennow-package-pins")
        .join(format!("{namespace}.version.pin"));
    fs::remove_file(&pin_path).unwrap();
    symlink(&victim, &pin_path).unwrap();
    assert!(verify(&root, &manifest).is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"unchanged");
    fs::remove_file(pin_path).unwrap();
    fs::remove_file(root.join("media")).unwrap();
    symlink(root.join("control"), root.join("media")).unwrap();
    assert!(verify(&root, &manifest).is_err());
}

#[test]
fn package_pin_child() {
    let Some(root) = std::env::var_os("OPENNOW_TEST_PIN_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let _pin = PackagePin::shared(&root).unwrap();
    fs::write(root.parent().unwrap().join("ready"), b"ready").unwrap();
    let mut byte = [0u8; 1];
    let _ = std::io::Read::read(&mut std::io::stdin(), &mut byte);
}

#[test]
fn separate_worker_keeps_version_pinned_without_a_core_handle() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let (temp, root, _) = package();
    let mut worker = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "package_pin_child", "--nocapture"])
        .env("OPENNOW_TEST_PIN_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !temp.path().join("ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(temp.path().join("ready").exists());
    for _ in 0..2 {
        assert_eq!(
            PackagePin::try_exclusive(&root).unwrap_err().code,
            "plugin_in_use"
        );
    }
    drop(worker.stdin.take());
    assert!(worker.wait().unwrap().success());
    assert!(PackagePin::try_exclusive(&root).is_ok());
}

#[test]
fn deleting_and_recreating_package_parent_cannot_replace_a_locked_pin_inode() {
    let (temp, root, _) = package();
    let mutation = PackagePin::try_exclusive(&root).unwrap();
    fs::remove_dir_all(temp.path()).unwrap();
    fs::create_dir_all(&root).unwrap();
    assert_eq!(PackagePin::shared(&root).unwrap_err().code, "plugin_in_use");
    drop(mutation);
    assert!(PackagePin::shared(&root).is_ok());
}
