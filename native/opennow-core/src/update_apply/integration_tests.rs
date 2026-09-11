use super::*;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Fixture {
    _temporary: TempDir,
    plan: Plan,
    directory: PathBuf,
    original: Vec<u8>,
    candidate: Vec<u8>,
    application: OwnedChild,
    core: OwnedChild,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(self.directory.join("exit-fixture"), b"exit");
        if let Ok(ack) = read_json::<Acknowledgement>(&self.directory.join("ack.json"), 64 * 1024) {
            let start = Instant::now();
            while ack.application.is_running().unwrap_or(false)
                && start.elapsed() < Duration::from_secs(10)
            {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let _ = self.application.0.kill();
        let _ = self.application.0.wait();
        let _ = self.core.0.kill();
        let _ = self.core.0.wait();
    }
}

fn fixture_path(name: &str) -> PathBuf {
    fs::canonicalize(std::env::var_os(name).unwrap_or_else(|| {
        panic!("Run opennow-qt/tests/run_update_helper_integration.py; missing {name}")
    }))
    .unwrap()
}

#[cfg(target_os = "macos")]
fn command_ok(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sign(package: &Path) {
    let seed: [u8; 32] = fs::read(fixture_path("OPENNOW_TEST_UPDATE_SEED_FILE"))
        .unwrap()
        .try_into()
        .unwrap();
    let key = SigningKey::from_bytes(&seed);
    assert_eq!(
        verification::embedded_update_key().unwrap(),
        key.verifying_key(),
        "Test key must be compiled into the helper; runtime key overrides are forbidden"
    );
    let bytes = fs::read(package).unwrap();
    let mut manifest = verification::UpdateManifest {
        schema_version: 1,
        version: "1.2.3".to_owned(),
        asset: package.file_name().unwrap().to_str().unwrap().to_owned(),
        size: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        signature: String::new(),
    };
    manifest.signature = base64::engine::general_purpose::STANDARD.encode(
        key.sign(verification::signature_payload(&manifest).as_bytes())
            .to_bytes(),
    );
    atomic_json(&manifest_path(package).unwrap(), &manifest).unwrap();
}

fn stage_executable(source: &Path, target: &Path) {
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(source, target).unwrap();
    make_executable(target).unwrap();
}

#[cfg(target_os = "macos")]
fn sign_bundle(root: &Path, identifier: &str, version: &str) {
    fs::write(root.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{identifier}</string><key>CFBundleExecutable</key><string>OpenNOW</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>{version}</string><key>CFBundleVersion</key><string>{version}</string></dict></plist>"#)).unwrap();
    command_ok(
        Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--deep",
                "--sign",
                "-",
                "--identifier",
                identifier,
            ])
            .arg(root),
    );
    command_ok(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(root),
    );
}

#[cfg(windows)]
fn zip_tree(root: &Path, package: &Path) {
    let mut zip = zip::ZipWriter::new(File::create(package).unwrap());
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&fs::read(path).unwrap()).unwrap();
        }
    }
    zip.finish().unwrap();
}

fn fixture(wrong_identity: bool) -> Fixture {
    let temporary = TempDir::new().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    let directory = root.join(".opennow-update-integration");
    fs::create_dir(&directory).unwrap();
    let previous = fixture_path("OPENNOW_TEST_UPDATE_PREVIOUS");
    let candidate = fixture_path("OPENNOW_TEST_UPDATE_CANDIDATE");
    #[cfg(any(windows, target_os = "macos"))]
    let helper = fixture_path("OPENNOW_TEST_UPDATE_HELPER");
    #[cfg(target_os = "linux")]
    let (kind, target, application, package, candidate_bytes) = {
        let target = root.join("OpenNOW.AppImage");
        stage_executable(&previous, &target);
        let package = directory.join("OpenNOW-1.2.3.AppImage");
        let mut bytes = fs::read(&candidate).unwrap();
        assert_eq!(&bytes[..4], b"\x7fELF");
        bytes[8..11].copy_from_slice(b"AI\x02");
        fs::write(&package, &bytes).unwrap();
        (
            InstallKind::AppImage,
            target.clone(),
            target,
            package,
            bytes,
        )
    };
    #[cfg(any(windows, target_os = "macos"))]
    let (kind, target, application, package, candidate_bytes) = {
        #[cfg(windows)]
        let (kind, target, relative, package) = (
            InstallKind::WindowsPortable,
            root.join("installed"),
            PathBuf::from("bin/OpenNOW.exe"),
            directory.join("OpenNOW-1.2.3.zip"),
        );
        #[cfg(target_os = "macos")]
        let (kind, target, relative, package) = (
            InstallKind::MacBundle,
            root.join("OpenNOW.app"),
            PathBuf::from("Contents/MacOS/OpenNOW"),
            directory.join("OpenNOW-1.2.3.dmg"),
        );
        let application = target.join(&relative);
        stage_executable(&previous, &application);
        let image_root = root.join("image-root");
        #[cfg(target_os = "macos")]
        let payload = image_root.join("OpenNOW.app");
        #[cfg(windows)]
        let payload = image_root.clone();
        let payload_application = payload.join(&relative);
        stage_executable(&candidate, &payload_application);
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        for (name, source) in [
            (format!("opennow-core{suffix}"), &previous),
            (format!("opennow-update-helper{suffix}"), &helper),
        ] {
            stage_executable(source, &application.parent().unwrap().join(&name));
            stage_executable(source, &payload_application.parent().unwrap().join(&name));
        }
        #[cfg(windows)]
        {
            if wrong_identity {
                fs::rename(
                    &payload_application,
                    payload_application.with_file_name("OtherNOW.exe"),
                )
                .unwrap();
            }
            zip_tree(&payload, &package);
        }
        #[cfg(target_os = "macos")]
        {
            sign_bundle(&target, "org.opennow.integration", "1.0.0");
            sign_bundle(
                &payload,
                if wrong_identity {
                    "org.opennow.unrelated"
                } else {
                    "org.opennow.integration"
                },
                "1.2.3",
            );
            command_ok(
                Command::new("/usr/bin/hdiutil")
                    .args([
                        "create",
                        "-quiet",
                        "-format",
                        "UDZO",
                        "-volname",
                        "OpenNOW integration",
                        "-srcfolder",
                    ])
                    .arg(&image_root)
                    .arg(&package),
            );
        }
        let bytes = if payload_application.exists() {
            fs::read(&payload_application).unwrap()
        } else {
            Vec::new()
        };
        (kind, target, application, package, bytes)
    };
    sign(&package);
    let target = fs::canonicalize(target).unwrap();
    let application_path = fs::canonicalize(application).unwrap();
    let original = fs::read(&application_path).unwrap();
    let data_dir = if cfg!(windows) {
        target.join("user-data")
    } else {
        root.join("settings")
    };
    fs::create_dir(&data_dir).unwrap();
    fs::write(data_dir.join("preferences.json"), b"{\"preserved\":true}").unwrap();
    let application = OwnedChild(
        Command::new(&application_path)
            .arg("--hold")
            .spawn()
            .unwrap(),
    );
    let core = OwnedChild(Command::new(&previous).arg("--hold").spawn().unwrap());
    let plan = Plan {
        schema_version: 1,
        version: if cfg!(target_os = "linux") && wrong_identity {
            "1.2.4"
        } else {
            "1.2.3"
        }
        .to_owned(),
        kind,
        package,
        target,
        application_executable: application_path,
        data_dir: fs::canonicalize(data_dir).unwrap(),
        processes: vec![
            ProcessIdentity::capture(application.0.id()).unwrap(),
            ProcessIdentity::capture(core.0.id()).unwrap(),
        ],
        nonce: "a".repeat(64),
        managed_identity: None,
    };
    atomic_json(&directory.join("plan.json"), &plan).unwrap();
    write_outcome(
        &directory,
        &plan,
        OutcomeStatus::Prepared,
        "Integration transaction prepared",
        None,
        None,
    )
    .unwrap();
    Fixture {
        _temporary: temporary,
        plan,
        directory,
        original,
        candidate: candidate_bytes,
        application,
        core,
    }
}

fn wait_for_status(fixture: &Fixture, helper: &mut OwnedChild, status: OutcomeStatus) {
    let start = Instant::now();
    loop {
        if read_outcome(&fixture.directory.join("outcome.json"))
            .unwrap()
            .is_some_and(|outcome| outcome.status == status)
        {
            return;
        }
        assert!(
            helper.0.try_wait().unwrap().is_none(),
            "Helper exited before {status:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(150),
            "Helper never reached {status:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn apply_after_exit(fixture: &mut Fixture) -> std::process::ExitStatus {
    let mut helper = OwnedChild(
        Command::new(fixture_path("OPENNOW_TEST_UPDATE_HELPER"))
            .arg("--apply")
            .arg(fixture.directory.join("plan.json"))
            .spawn()
            .unwrap(),
    );
    wait_for_status(fixture, &mut helper, OutcomeStatus::WaitingForExit);
    assert_eq!(
        fs::read(&fixture.plan.application_executable).unwrap(),
        fixture.original
    );
    assert!(!fixture.directory.join("ack.json").exists());
    fixture.application.0.kill().unwrap();
    fixture.application.0.wait().unwrap();
    fixture.core.0.kill().unwrap();
    fixture.core.0.wait().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = helper.0.try_wait().unwrap() {
            return status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "Helper did not finish replacement"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "Run opennow-qt/tests/run_update_helper_integration.py with an ephemeral pinned test key"]
fn signed_native_package_replaces_and_candidate_acknowledges_its_own_restart() {
    let mut fixture = fixture(false);
    assert!(apply_after_exit(&mut fixture).success());
    let outcome = read_outcome(&fixture.directory.join("outcome.json"))
        .unwrap()
        .unwrap();
    assert_eq!(outcome.status, OutcomeStatus::Completed);
    assert_eq!(outcome.installed_version.as_deref(), Some("1.2.3"));
    assert_eq!(
        fs::read(&fixture.plan.application_executable).unwrap(),
        fixture.candidate
    );
    assert_ne!(fixture.candidate, fixture.original);
    let ack: Acknowledgement = read_json(&fixture.directory.join("ack.json"), 64 * 1024).unwrap();
    assert_eq!(ack.version, "1.2.3");
    assert_eq!(ack.nonce, fixture.plan.nonce);
    assert_ne!(ack.application.pid, std::process::id());
    assert_ne!(ack.application.pid, fixture.plan.processes[0].pid);
    assert_eq!(
        ack.application.executable,
        fixture.plan.application_executable
    );
    assert!(ack.application.is_running().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.directory.join("candidate-started")).unwrap(),
        ack.application.pid.to_string()
    );
    assert_eq!(
        fs::read(fixture.plan.data_dir.join("preferences.json")).unwrap(),
        b"{\"preserved\":true}"
    );
    assert!(!fixture.directory.join("previous").exists());
    assert!(!fixture.plan.package.exists());
    #[cfg(target_os = "macos")]
    assert!(!fixture.directory.join("payload.mount").exists());
}

#[test]
#[ignore = "Run opennow-qt/tests/run_update_helper_integration.py with an ephemeral pinned test key"]
fn signed_native_package_with_failed_startup_rolls_back_and_restarts_previous() {
    let mut fixture = fixture(false);
    fs::write(fixture.directory.join("reject-startup"), b"reject").unwrap();
    assert!(!apply_after_exit(&mut fixture).success());
    assert_eq!(
        read_outcome(&fixture.directory.join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::RolledBack
    );
    assert_eq!(
        fs::read(&fixture.plan.application_executable).unwrap(),
        fixture.original
    );
    assert!(!fixture.directory.join("ack.json").exists());
    assert!(fixture.directory.join("candidate-started").exists());
    let start = Instant::now();
    while !fixture.plan.data_dir.join("previous-restarted").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Previous application did not restart after rollback"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        fs::read(fixture.plan.data_dir.join("previous-restarted")).unwrap(),
        b"1.0.0"
    );
}

#[test]
#[ignore = "Run opennow-qt/tests/run_update_helper_integration.py with an ephemeral pinned test key"]
fn tampered_native_package_never_replaces_or_restarts() {
    let fixture = fixture(false);
    let mut bytes = fs::read(&fixture.plan.package).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&fixture.plan.package, bytes).unwrap();
    assert!(
        run_helper(&fixture.directory.join("plan.json"))
            .unwrap_err()
            .contains("SHA256")
    );
    assert_eq!(
        fs::read(&fixture.plan.application_executable).unwrap(),
        fixture.original
    );
    assert!(fixture.plan.processes[0].is_running().unwrap());
    assert!(!fixture.directory.join("candidate-started").exists());
    assert_eq!(
        read_outcome(&fixture.directory.join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::Failed
    );
}

#[test]
#[ignore = "Run opennow-qt/tests/run_update_helper_integration.py with an ephemeral pinned test key"]
fn signed_wrong_package_or_bundle_identity_never_replaces() {
    let fixture = fixture(true);
    assert!(run_helper(&fixture.directory.join("plan.json")).is_err());
    assert_eq!(
        fs::read(&fixture.plan.application_executable).unwrap(),
        fixture.original
    );
    assert!(fixture.plan.processes[0].is_running().unwrap());
    assert!(!fixture.directory.join("candidate-started").exists());
    assert_eq!(
        read_outcome(&fixture.directory.join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::Failed
    );
}
