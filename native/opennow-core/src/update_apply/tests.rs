use super::*;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use tempfile::TempDir;

fn signed_manifest(asset: &str, bytes: &[u8]) -> (verification::UpdateManifest, SigningKey) {
    let key = SigningKey::from_bytes(&[91; 32]);
    let mut manifest = verification::UpdateManifest {
        schema_version: 1,
        version: "1.2.3".to_owned(),
        asset: asset.to_owned(),
        size: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
        signature: String::new(),
    };
    manifest.signature = base64::engine::general_purpose::STANDARD.encode(
        key.sign(verification::signature_payload(&manifest).as_bytes())
            .to_bytes(),
    );
    (manifest, key)
}

#[test]
fn signature_rejects_tampering_wrong_key_and_invalid_schema() {
    let (manifest, key) = signed_manifest("OpenNOW.zip", b"valid package");
    verification::verify_manifest(&manifest, &key.verifying_key()).unwrap();
    assert!(
        verification::verify_manifest(
            &manifest,
            &SigningKey::from_bytes(&[90; 32]).verifying_key()
        )
        .is_err()
    );
    let mut tampered = manifest.clone();
    tampered.version = "1.2.4".to_owned();
    assert!(verification::verify_manifest(&tampered, &key.verifying_key()).is_err());
    let mut invalid = manifest;
    invalid.schema_version = 2;
    assert!(verification::verify_manifest(&invalid, &key.verifying_key()).is_err());
}

#[test]
fn package_verification_rejects_tampering_and_wrong_filename() {
    let dir = TempDir::new().unwrap();
    let bytes = b"valid package";
    let (manifest, _) = signed_manifest("OpenNOW.zip", bytes);
    let package = dir.path().join("OpenNOW.zip");
    fs::write(&package, bytes).unwrap();
    verification::verify_package(&package, &manifest).unwrap();
    fs::write(&package, b"evil! package").unwrap();
    assert!(
        verification::verify_package(&package, &manifest)
            .unwrap_err()
            .contains("SHA256")
    );
    let other = dir.path().join("Other.zip");
    fs::write(&other, bytes).unwrap();
    assert!(verification::verify_package(&other, &manifest).is_err());
}

fn zip_file(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn archive_rejects_traversal_case_aliases_and_partial_extraction() {
    for entries in [
        vec![
            ("safe.txt", b"first".as_slice()),
            ("../escaped", b"evil".as_slice()),
        ],
        vec![
            ("Bin/app", b"first".as_slice()),
            ("bin/App", b"evil".as_slice()),
        ],
        vec![
            ("safe.txt", b"first".as_slice()),
            ("C:/evil", b"evil".as_slice()),
        ],
        vec![
            ("safe.txt", b"first".as_slice()),
            ("dir/CON.txt", b"evil".as_slice()),
        ],
    ] {
        let dir = TempDir::new().unwrap();
        let package = dir.path().join("update.zip");
        zip_file(&package, &entries);
        let destination = dir.path().join("extracted");
        assert!(archive::extract_zip(&package, &destination, false).is_err());
        assert!(!destination.exists());
        assert!(!dir.path().join("escaped").exists());
    }
}

#[test]
fn archive_rejects_corruption_without_leaving_a_partial_tree() {
    let dir = TempDir::new().unwrap();
    let package = dir.path().join("update.zip");
    zip_file(&package, &[("bin/app", b"candidate executable")]);
    let mut bytes = fs::read(&package).unwrap();
    let offset = bytes
        .windows(20)
        .position(|window| window == b"candidate executable")
        .unwrap();
    bytes[offset] ^= 1;
    fs::write(&package, bytes).unwrap();
    let destination = dir.path().join("extracted");
    assert!(archive::extract_zip(&package, &destination, false).is_err());
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn framework_symlinks_are_preserved_and_escaping_links_are_rejected() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("OpenNOW.app");
    let framework = source.join("Contents/Frameworks/Qt.framework");
    fs::create_dir_all(framework.join("Versions/A")).unwrap();
    fs::write(framework.join("Versions/A/Qt"), b"framework").unwrap();
    std::os::unix::fs::symlink("A", framework.join("Versions/Current")).unwrap();
    std::os::unix::fs::symlink("Versions/Current/Qt", framework.join("Qt")).unwrap();
    let copy = dir.path().join("copied.app");
    bundle::copy_tree(
        &source,
        &copy,
        &mut bundle::CopyBudget::new(),
        bundle::CopyPolicy::Bundle,
    )
    .unwrap();
    assert!(
        fs::symlink_metadata(copy.join("Contents/Frameworks/Qt.framework/Qt"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        fs::read(copy.join("Contents/Frameworks/Qt.framework/Qt")).unwrap(),
        b"framework"
    );
    fs::write(dir.path().join("secret"), b"outside").unwrap();
    std::os::unix::fs::symlink("../secret", source.join("escape")).unwrap();
    let bad = dir.path().join("bad.app");
    assert!(
        bundle::copy_tree(
            &source,
            &bad,
            &mut bundle::CopyBudget::new(),
            bundle::CopyPolicy::Bundle
        )
        .is_err()
    );
    assert!(!bad.exists());
}

fn fixture_executable() -> &'static Path {
    static FIXTURE: OnceLock<TempDir> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            let directory = TempDir::new().unwrap();
            let source = directory.path().join("fixture.rs");
        fs::write(
            &source,
            r#"fn main() {
                if let Ok(path) = std::env::var("OPENNOW_FIXTURE_CHILD_PID") {
                    let child = std::process::Command::new(std::env::current_exe().unwrap()).env_remove("OPENNOW_FIXTURE_CHILD_PID").spawn().unwrap();
                    let path = std::path::PathBuf::from(path);
                    let temporary = path.with_extension("tmp");
                    std::fs::write(&temporary, child.id().to_string()).unwrap();
                    std::fs::rename(temporary, path).unwrap();
                }
                let lifetime = std::env::var("OPENNOW_FIXTURE_LIFETIME_MS").ok().and_then(|value| value.parse().ok()).unwrap_or(900);
                std::thread::sleep(std::time::Duration::from_millis(lifetime));
            }"#,
            )
            .unwrap();
            let mut compiler = Command::new("rustc");
            #[cfg(all(windows, target_env = "msvc"))]
            {
                let version = Command::new("rustc").arg("-vV").output().unwrap();
                assert!(version.status.success());
                let version = String::from_utf8(version.stdout).unwrap();
                let host = version.lines().find_map(|line| line.strip_prefix("host: ")).expect("rustc host triple");
                if host.ends_with("-windows-msvc") {
                    let variable = format!("CARGO_TARGET_{}_LINKER", host.replace('-', "_").to_ascii_uppercase());
                    if let Some(linker) = std::env::var_os(variable) {
                        let mut option = std::ffi::OsString::from("linker=");
                        option.push(linker);
                        compiler.arg("-C").arg(option);
                    }
                }
            }
            let output = compiler.arg(&source)
                .arg("-o")
                .arg(directory.path().join("fixture.exe"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            directory
        })
        .path()
}

#[test]
fn owned_application_accepts_descendants_and_stops_the_entire_tree() {
    let directory = TempDir::new().unwrap();
    let pid_file = directory.path().join("child.pid");
    let mut command = Command::new(fixture_executable().join("fixture.exe"));
    command
        .env("OPENNOW_FIXTURE_CHILD_PID", &pid_file)
        .env("OPENNOW_FIXTURE_LIFETIME_MS", "60000");
    let mut application = process::OwnedApplication::start(&mut command).unwrap();
    let start = Instant::now();
    while !pid_file.exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    let descendant = ProcessIdentity::capture(pid).unwrap();
    assert!(application.owns(&descendant).unwrap());
    assert!(
        !application
            .owns(&ProcessIdentity::capture(std::process::id()).unwrap())
            .unwrap()
    );
    application.stop().unwrap();
    assert!(!descendant.is_running().unwrap());
    assert!(!application.identity.is_running().unwrap());
}

#[cfg(unix)]
#[test]
fn zip_framework_links_are_preserved_but_external_links_are_rejected() {
    let directory = TempDir::new().unwrap();
    let package = directory.path().join("app.zip");
    let mut zip = zip::ZipWriter::new(File::create(&package).unwrap());
    zip.start_file(
        "Contents/Frameworks/Qt/Versions/A/Qt",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"framework").unwrap();
    zip.add_symlink(
        "Contents/Frameworks/Qt/Versions/Current",
        "A",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.add_symlink(
        "Contents/Frameworks/Qt/Qt",
        "Versions/Current/Qt",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    let payload = directory.path().join("payload");
    archive::extract_zip(&package, &payload, true).unwrap();
    assert_eq!(
        fs::read(payload.join("Contents/Frameworks/Qt/Qt")).unwrap(),
        b"framework"
    );
    let mut zip = zip::ZipWriter::new(File::create(&package).unwrap());
    zip.add_symlink(
        "escape",
        "../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    assert!(archive::extract_zip(&package, &directory.path().join("bad"), true).is_err());
    assert!(!directory.path().join("bad").exists());
}

#[cfg(unix)]
#[test]
fn fixture_plan_uses_canonical_installation_paths() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("actual");
    fs::create_dir(&target).unwrap();
    let alias = directory.path().join("alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    let plan = plan(&alias, InstallKind::WindowsPortable);
    assert_eq!(
        plan.target,
        fs::canonicalize(&target).unwrap().join("installed")
    );
    assert_eq!(
        plan.application_executable,
        plan.target.join("bin/OpenNOW.exe")
    );
}

fn plan(directory: &Path, kind: InstallKind) -> Plan {
    let directory = fs::canonicalize(directory).unwrap();
    let target = directory.join(if kind == InstallKind::AppImage {
        "OpenNOW.AppImage"
    } else {
        "installed"
    });
    let application = if kind == InstallKind::AppImage {
        target.clone()
    } else {
        target.join("bin/OpenNOW.exe")
    };
    Plan {
        schema_version: 1,
        version: "1.2.3".to_owned(),
        kind,
        package: directory.join("package.zip"),
        target,
        application_executable: application,
        data_dir: directory.join("settings"),
        processes: vec![],
        nonce: "a".repeat(64),
        managed_identity: None,
    }
}

fn stage_native_executable(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(fixture_executable().join("fixture.exe"), path).unwrap();
    make_executable(path).unwrap();
}

#[test]
fn failed_appimage_restart_restores_previous_binary() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::AppImage);
    fs::write(&plan.target, b"original appimage").unwrap();
    let candidate = directory.path().join("candidate");
    fs::write(&candidate, b"not an executable").unwrap();
    make_executable(&candidate).unwrap();
    assert!(
        replace_and_restart(
            &plan,
            directory.path(),
            &candidate,
            Duration::from_millis(100)
        )
        .is_err()
    );
    assert_eq!(fs::read(&plan.target).unwrap(), b"original appimage");
    assert_eq!(
        fs::read(directory.path().join("failed")).unwrap(),
        b"not an executable"
    );
    assert_eq!(
        read_outcome(&directory.path().join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::RolledBack
    );
}

#[test]
fn failed_directory_replacement_restores_all_original_files_and_settings() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    fs::create_dir_all(plan.application_executable.parent().unwrap()).unwrap();
    fs::write(&plan.application_executable, b"original executable").unwrap();
    fs::write(plan.target.join("original.dll"), b"original dependency").unwrap();
    fs::create_dir(&plan.data_dir).unwrap();
    fs::write(plan.data_dir.join("settings.json"), b"keep settings").unwrap();
    let missing = directory.path().join("missing");
    assert!(
        replace_and_restart(
            &plan,
            directory.path(),
            &missing,
            Duration::from_millis(100)
        )
        .is_err()
    );
    assert_eq!(
        fs::read(&plan.application_executable).unwrap(),
        b"original executable"
    );
    assert_eq!(
        fs::read(plan.target.join("original.dll")).unwrap(),
        b"original dependency"
    );
    assert_eq!(
        fs::read(plan.data_dir.join("settings.json")).unwrap(),
        b"keep settings"
    );
}

#[test]
fn failed_shutdown_and_failed_outcome_write_never_authorize_rollback() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::AppImage);
    fs::write(&plan.target, b"running candidate").unwrap();
    fs::write(directory.path().join("previous"), b"previous application").unwrap();
    write_outcome(
        directory.path(),
        &plan,
        OutcomeStatus::AwaitingStartup,
        "candidate running",
        None,
        None,
    )
    .unwrap();
    let before = fs::read(directory.path().join("outcome.json")).unwrap();
    let mut attempted_record = false;
    let failure = failed_restart(
        "startup failed".to_owned(),
        Err("process shutdown failed".to_owned()),
        |_| {
            attempted_record = true;
            Err("disk full".to_owned())
        },
    );
    assert!(attempted_record);
    assert!(matches!(failure, RestartFailure::ProcessesMayBeRunning(_)));
    assert!(recover_replacement(&plan, directory.path(), failure).is_err());
    assert_eq!(fs::read(&plan.target).unwrap(), b"running candidate");
    assert_eq!(
        fs::read(directory.path().join("previous")).unwrap(),
        b"previous application"
    );
    assert_eq!(
        fs::read(directory.path().join("outcome.json")).unwrap(),
        before
    );
    assert!(!directory.path().join("failed").exists());
}

#[test]
fn interrupted_startup_without_identity_does_not_roll_back_under_unknown_processes() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::AppImage);
    fs::write(&plan.target, b"candidate").unwrap();
    fs::write(directory.path().join("previous"), b"previous").unwrap();
    write_outcome(
        directory.path(),
        &plan,
        OutcomeStatus::AwaitingStartup,
        "startup interrupted",
        None,
        None,
    )
    .unwrap();
    assert!(
        apply(&plan, directory.path())
            .unwrap_err()
            .contains("unsafe")
    );
    assert_eq!(fs::read(&plan.target).unwrap(), b"candidate");
    assert_eq!(
        fs::read(directory.path().join("previous")).unwrap(),
        b"previous"
    );
}

#[cfg(unix)]
#[test]
fn failed_start_cleans_the_forked_group_before_reaping_the_parent() {
    use std::os::unix::process::CommandExt;
    let directory = TempDir::new().unwrap();
    let pid_file = directory.path().join("child.pid");
    let mut child = Command::new(fixture_executable().join("fixture.exe"))
        .process_group(0)
        .env("OPENNOW_FIXTURE_CHILD_PID", &pid_file)
        .env("OPENNOW_FIXTURE_LIFETIME_MS", "60000")
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !pid_file.exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let descendant =
        ProcessIdentity::capture(fs::read_to_string(pid_file).unwrap().parse().unwrap()).unwrap();
    let failure =
        process::failed_start(&mut child, "identity capture failed after fork".to_owned());
    assert!(
        matches!(failure, RestartFailure::RollbackSafe(_)),
        "{failure}"
    );
    assert!(!descendant.is_running().unwrap());
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
fn spawn_without_startup_acknowledgement_rolls_back_the_complete_directory() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    fs::create_dir_all(plan.application_executable.parent().unwrap()).unwrap();
    fs::write(&plan.application_executable, b"old executable").unwrap();
    let payload = directory.path().join("payload");
    stage_native_executable(&payload.join("bin/OpenNOW.exe"));
    assert!(
        replace_and_restart(&plan, directory.path(), &payload, Duration::from_millis(30))
            .unwrap_err()
            .contains("acknowledge")
    );
    assert_eq!(
        fs::read(&plan.application_executable).unwrap(),
        b"old executable"
    );
    assert!(directory.path().join("failed/bin/OpenNOW.exe").is_file());
}

#[test]
fn portable_directory_replacement_requires_ack_and_preserves_settings() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    fs::create_dir_all(plan.application_executable.parent().unwrap()).unwrap();
    fs::write(&plan.application_executable, b"old executable").unwrap();
    fs::write(plan.target.join("old-only.dll"), b"old dependency").unwrap();
    fs::create_dir(&plan.data_dir).unwrap();
    fs::write(plan.data_dir.join("settings.json"), b"unchanged").unwrap();
    let payload = directory.path().join("payload");
    stage_native_executable(&payload.join("bin/OpenNOW.exe"));
    fs::write(payload.join("new-only.dll"), b"new dependency").unwrap();
    let acknowledgement_directory = directory.path().to_path_buf();
    let nonce = plan.nonce.clone();
    let version = plan.version.clone();
    let worker = std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if let Some(outcome) =
                read_outcome(&acknowledgement_directory.join("outcome.json")).unwrap()
            {
                if let (OutcomeStatus::AwaitingStartup, Some(application)) =
                    (outcome.status, outcome.restarted_process)
                {
                    atomic_json(
                        &acknowledgement_directory.join("ack.json"),
                        &Acknowledgement {
                            nonce,
                            version,
                            application,
                        },
                    )
                    .unwrap();
                    break;
                }
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    replace_and_restart(&plan, directory.path(), &payload, Duration::from_secs(10)).unwrap();
    worker.join().unwrap();
    assert!(!plan.target.join("old-only.dll").exists());
    assert_eq!(
        fs::read(plan.target.join("new-only.dll")).unwrap(),
        b"new dependency"
    );
    assert_eq!(
        fs::read(plan.data_dir.join("settings.json")).unwrap(),
        b"unchanged"
    );
    assert!(!directory.path().join("previous").exists());
    assert_eq!(
        read_outcome(&directory.path().join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::Completed
    );
    std::thread::sleep(Duration::from_millis(950));
}

#[test]
fn acknowledged_application_remains_running_after_owner_drop() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    stage_native_executable(&plan.application_executable);
    let acknowledgement_directory = directory.path().to_path_buf();
    let nonce = plan.nonce.clone();
    let version = plan.version.clone();
    let worker = std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if let Some(outcome) =
                read_outcome(&acknowledgement_directory.join("outcome.json")).unwrap()
            {
                if let (OutcomeStatus::AwaitingStartup, Some(application)) =
                    (outcome.status, outcome.restarted_process)
                {
                    atomic_json(
                        &acknowledgement_directory.join("ack.json"),
                        &Acknowledgement {
                            nonce,
                            version,
                            application: application.clone(),
                        },
                    )
                    .unwrap();
                    return application;
                }
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    restart_and_acknowledge(&plan, directory.path(), Duration::from_secs(10)).unwrap();
    let application = worker.join().unwrap();
    assert!(application.is_running().unwrap());
    std::thread::sleep(Duration::from_millis(100));
    assert!(application.is_running().unwrap());
    let deadline = Instant::now();
    while application.is_running().unwrap() {
        assert!(deadline.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn interrupted_replacement_recovers_without_the_target_existing() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::AppImage);
    fs::write(directory.path().join("previous"), b"original bytes").unwrap();
    write_outcome(
        directory.path(),
        &plan,
        OutcomeStatus::Installing,
        "interrupted",
        None,
        None,
    )
    .unwrap();
    apply(&plan, directory.path()).unwrap();
    assert_eq!(fs::read(&plan.target).unwrap(), b"original bytes");
    assert_eq!(
        read_outcome(&directory.path().join("outcome.json"))
            .unwrap()
            .unwrap()
            .status,
        OutcomeStatus::RolledBack
    );
}

#[test]
fn repeated_helper_invocation_preserves_cleaned_up_terminal_outcomes() {
    for status in [OutcomeStatus::Completed, OutcomeStatus::RolledBack] {
        let directory = TempDir::new().unwrap();
        let plan = plan(directory.path(), InstallKind::AppImage);
        atomic_json(&directory.path().join("plan.json"), &plan).unwrap();
        write_outcome(
            directory.path(),
            &plan,
            status,
            "terminal result",
            Some(plan.version.clone()),
            None,
        )
        .unwrap();
        assert!(!plan.package.exists());
        let outcome = fs::read(directory.path().join("outcome.json")).unwrap();
        run_helper(&directory.path().join("plan.json")).unwrap();
        run_helper(&directory.path().join("plan.json")).unwrap();
        assert_eq!(
            fs::read(directory.path().join("outcome.json")).unwrap(),
            outcome
        );
    }
}

#[test]
fn advisory_lock_distinguishes_live_helper_from_stale_outcomes() {
    let directory = TempDir::new().unwrap();
    let prepared = PreparedUpdate {
        plan_path: directory.path().join("plan.json"),
        outcome_path: directory.path().join("outcome.json"),
        version: "1.2.3".to_owned(),
    };
    assert!(!helper_is_running(&prepared).unwrap());
    let lock = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.path().join("apply.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(helper_is_running(&prepared).unwrap());
    FileExt::unlock(&lock).unwrap();
    assert!(!helper_is_running(&prepared).unwrap());
}

#[test]
fn portable_nested_profile_and_extra_root_files_are_preserved() {
    let directory = TempDir::new().unwrap();
    let mut plan = plan(directory.path(), InstallKind::WindowsPortable);
    plan.data_dir = plan.target.join("bin/profile");
    fs::create_dir_all(&plan.data_dir).unwrap();
    fs::write(plan.data_dir.join("settings.json"), b"custom profile").unwrap();
    fs::write(plan.target.join("personal-notes.txt"), b"user-owned notes").unwrap();
    let payload = directory.path().join("payload");
    fs::create_dir_all(payload.join("bin")).unwrap();
    let paths = preserve_portable_data(&plan, &payload).unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(
        fs::read(payload.join("bin/profile/settings.json")).unwrap(),
        b"custom profile"
    );
    assert_eq!(
        fs::read(payload.join("personal-notes.txt")).unwrap(),
        b"user-owned notes"
    );
}

#[cfg(unix)]
#[test]
fn portable_profile_preserves_restrictive_unix_permissions_and_ownership() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("profile");
    fs::create_dir_all(source.join("credentials")).unwrap();
    fs::write(
        source.join("credentials/account.json"),
        b"private fixture data",
    )
    .unwrap();
    for path in [&source, &source.join("credentials")] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::set_permissions(
        source.join("credentials/account.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let destination = directory.path().join("preserved-profile");
    copy_user_data(&source, &destination, &mut bundle::CopyBudget::new()).unwrap();
    for relative in ["", "credentials", "credentials/account.json"] {
        let before = fs::metadata(source.join(relative)).unwrap();
        let after = fs::metadata(destination.join(relative)).unwrap();
        assert_eq!(before.mode() & 0o7777, after.mode() & 0o7777);
        assert_eq!(before.uid(), after.uid());
        assert_eq!(before.gid(), after.gid());
    }
    assert_eq!(
        fs::metadata(destination.join("credentials/account.json"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn transaction_directory_is_private_at_creation() {
    use std::os::unix::fs::PermissionsExt;
    let directory = TempDir::new().unwrap();
    let transaction = directory.path().join("transaction");
    security::create_private_directory(&transaction).unwrap();
    assert_eq!(
        fs::metadata(transaction).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn preserved_roots_share_one_entry_and_byte_budget() {
    let directory = TempDir::new().unwrap();
    for name in ["first", "second"] {
        fs::create_dir(directory.path().join(name)).unwrap();
        fs::write(directory.path().join(name).join("data"), b"123456").unwrap();
    }
    let mut bytes = bundle::CopyBudget::with_limits(10, 10);
    copy_user_data(
        &directory.path().join("first"),
        &directory.path().join("copy-first"),
        &mut bytes,
    )
    .unwrap();
    assert!(
        copy_user_data(
            &directory.path().join("second"),
            &directory.path().join("copy-second"),
            &mut bytes
        )
        .unwrap_err()
        .contains("byte budget")
    );
    assert!(!directory.path().join("copy-second").exists());
    let mut entries = bundle::CopyBudget::with_limits(3, 100);
    copy_user_data(
        &directory.path().join("first"),
        &directory.path().join("entries-first"),
        &mut entries,
    )
    .unwrap();
    assert!(
        copy_user_data(
            &directory.path().join("second"),
            &directory.path().join("entries-second"),
            &mut entries
        )
        .unwrap_err()
        .contains("entry limit")
    );
    assert!(!directory.path().join("entries-second").exists());
}

#[test]
fn streaming_copy_rejects_file_growth_without_writing_past_the_budget() {
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("live-data");
    fs::write(&source, b"before").unwrap();
    let mut input = File::open(&source).unwrap();
    let observed_size = input.metadata().unwrap().len();
    OpenOptions::new()
        .append(true)
        .open(&source)
        .unwrap()
        .write_all(b"appended after metadata")
        .unwrap();
    let mut output = Vec::new();
    assert!(
        copy_bounded(&mut input, &mut output, observed_size)
            .unwrap_err()
            .contains("streaming byte budget")
    );
    assert_eq!(output.len() as u64, observed_size);
    assert_eq!(output, b"before");
}

#[test]
fn preserved_roots_are_rediscovered_and_rebudgeted_after_shutdown() {
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    fs::create_dir_all(plan.target.join("bin")).unwrap();
    fs::write(plan.target.join("first-profile"), b"old").unwrap();
    let payload = directory.path().join("payload");
    fs::create_dir(&payload).unwrap();
    let prepared = preserve_portable_data(&plan, &payload).unwrap();
    fs::write(plan.target.join("first-profile"), b"latest").unwrap();
    fs::write(plan.target.join("created-during-shutdown"), b"new").unwrap();
    for (_, destination) in prepared {
        remove_path(&destination).unwrap();
    }
    let refreshed = preserve_portable_data(&plan, &payload).unwrap();
    assert_eq!(refreshed.len(), 2);
    assert_eq!(fs::read(payload.join("first-profile")).unwrap(), b"latest");
    assert_eq!(
        fs::read(payload.join("created-during-shutdown")).unwrap(),
        b"new"
    );
}

#[cfg(windows)]
#[test]
fn windows_busy_file_rejects_replacement_without_losing_original() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = TempDir::new().unwrap();
    let plan = plan(directory.path(), InstallKind::WindowsPortable);
    fs::create_dir_all(plan.application_executable.parent().unwrap()).unwrap();
    fs::write(&plan.application_executable, b"original executable").unwrap();
    let _busy = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&plan.application_executable)
        .unwrap();
    let payload = directory.path().join("payload");
    stage_native_executable(&payload.join("bin/OpenNOW.exe"));
    assert!(
        replace_and_restart(&plan, directory.path(), &payload, Duration::from_millis(30)).is_err()
    );
    assert!(plan.application_executable.exists());
    assert!(!directory.path().join("previous").exists());
}

#[test]
fn running_process_identity_has_a_bounded_wait() {
    let identity = ProcessIdentity::capture(std::process::id()).unwrap();
    assert!(identity.is_running().unwrap());
    assert!(
        process::wait_for_exit(&[identity], Duration::from_millis(20))
            .unwrap_err()
            .contains("Timed out")
    );
}
