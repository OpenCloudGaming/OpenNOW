mod archive;
mod bundle;
mod managed;
mod process;
mod recovery;
mod security;
pub mod verification;

use fs2::FileExt;
pub use process::ProcessIdentity;
pub use recovery::{ManagedRecovery, recover_managed_update};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    AppImage,
    WindowsPortable,
    MacBundle,
    WindowsMsi,
    DebianPackage,
}

#[derive(Debug)]
pub(super) enum RestartFailure {
    RollbackSafe(String),
    ProcessesMayBeRunning(String),
}

impl std::fmt::Display for RestartFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RollbackSafe(message) | Self::ProcessesMayBeRunning(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrepareRequest {
    pub package: PathBuf,
    pub expected_version: String,
    pub application_executable: PathBuf,
    pub application_pid: u32,
    pub core_pid: u32,
    pub kind: InstallKind,
    pub data_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedUpdate {
    pub plan_path: PathBuf,
    pub outcome_path: PathBuf,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    schema_version: u32,
    version: String,
    kind: InstallKind,
    package: PathBuf,
    target: PathBuf,
    application_executable: PathBuf,
    data_dir: PathBuf,
    processes: Vec<ProcessIdentity>,
    nonce: String,
    managed_identity: Option<managed::Identity>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeStatus {
    Prepared,
    WaitingForExit,
    BackingUp,
    Installing,
    AwaitingStartup,
    Completed,
    RolledBack,
    Failed,
    ManagedPending,
    RebootRequired,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateOutcome {
    pub schema_version: u32,
    pub version: String,
    pub status: OutcomeStatus,
    pub message: String,
    pub installed_version: Option<String>,
    pub restarted_process: Option<ProcessIdentity>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Acknowledgement {
    nonce: String,
    version: String,
    application: ProcessIdentity,
}

pub fn detect_install_kind(application: &Path, package: &Path) -> Result<InstallKind, String> {
    let extension = package
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "appimage" if cfg!(target_os = "linux") => Ok(InstallKind::AppImage),
        "deb" if cfg!(target_os = "linux") => Ok(InstallKind::DebianPackage),
        "msi" if cfg!(windows) => Ok(InstallKind::WindowsMsi),
        "zip" if cfg!(windows) => Ok(InstallKind::WindowsPortable),
        "zip" | "dmg"
            if cfg!(target_os = "macos")
                && application
                    .ancestors()
                    .any(|path| path.extension().is_some_and(|extension| extension == "app")) =>
        {
            Ok(InstallKind::MacBundle)
        }
        _ => Err(
            "No safe native update mechanism exists for this package and installation".to_owned(),
        ),
    }
}

pub fn compatible_package_extension() -> Result<&'static str, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(if std::env::var_os("APPIMAGE").is_some() {
            "appimage"
        } else {
            "deb"
        })
    }
    #[cfg(target_os = "macos")]
    {
        Ok("dmg")
    }
    #[cfg(windows)]
    {
        let executable = std::env::var_os("OPENNOW_APP_EXECUTABLE")
            .ok_or("Missing trusted application executable")?;
        let executable = canonical_file(Path::new(&executable))?;
        let root = installation_target(InstallKind::WindowsMsi, &executable)?;
        Ok(if managed::windows_managed(&root)? {
            "msi"
        } else {
            "zip"
        })
    }
}

pub fn prepare_update(request: PrepareRequest) -> Result<PreparedUpdate, String> {
    let package = canonical_file(&request.package)?;
    let manifest = read_manifest(&package)?;
    if manifest.version.trim_start_matches('v') != request.expected_version.trim_start_matches('v')
    {
        return Err("Signed update version does not match the selected release".to_owned());
    }
    verification::verify_package(&package, &manifest)?;
    let application = canonical_file(&request.application_executable)?;
    if detect_install_kind(&application, &package)? != request.kind {
        return Err("Update package does not match the installation type".to_owned());
    }
    let app_process = ProcessIdentity::capture(request.application_pid)?;
    app_process.matches_executable(&application)?;
    let core_process = ProcessIdentity::capture(request.core_pid)?;
    core_process
        .matches_executable(&std::env::current_exe().map_err(|error| error.to_string())?)?;
    let target = installation_target(request.kind, &application)?;
    if request.kind == InstallKind::WindowsPortable && managed::windows_managed(&target)? {
        return Err(
            "Windows Installer owns this installation; a portable ZIP cannot replace it".to_owned(),
        );
    }
    let data_dir = fs::canonicalize(&request.data_dir).map_err(|error| error.to_string())?;
    if (data_dir.starts_with(&target) && request.kind != InstallKind::WindowsPortable)
        || target.starts_with(&data_dir)
    {
        return Err(
            "Application settings must be outside the installation being replaced".to_owned(),
        );
    }
    let parent = if matches!(
        request.kind,
        InstallKind::WindowsMsi | InstallKind::DebianPackage
    ) {
        data_dir.join("updates")
    } else {
        target
            .parent()
            .ok_or("Installation has no parent")?
            .to_path_buf()
    };
    fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let directory = parent.join(format!(".opennow-update-{:032x}", rand::random::<u128>()));
    security::create_private_directory(&directory)
        .map_err(|error| format!("Cannot write beside the installed application: {error}"))?;
    let result = (|| {
        let private_package = directory.join(&manifest.asset);
        copy_synced(&package, &private_package)?;
        copy_synced(&manifest_path(&package)?, &manifest_path(&private_package)?)?;
        verification::verify_package(&private_package, &manifest)?;
        let managed_identity =
            managed::prepare(request.kind, &private_package, &target, &manifest.version)?;
        let plan = Plan {
            schema_version: 1,
            version: manifest.version.trim_start_matches('v').to_owned(),
            kind: request.kind,
            package: private_package,
            target,
            application_executable: application,
            data_dir,
            processes: vec![app_process, core_process],
            nonce: format!("{:064x}", rand::random::<u128>()),
            managed_identity,
        };
        let preflight = prepare_payload(&plan, &directory.join("preflight"))?;
        preserve_portable_data(&plan, &preflight)?;
        remove_path(&directory.join("preflight"))?;
        let helper = std::env::current_exe()
            .map_err(|error| error.to_string())?
            .parent()
            .ok_or("Core has no executable directory")?
            .join(if cfg!(windows) {
                "opennow-update-helper.exe"
            } else {
                "opennow-update-helper"
            });
        copy_synced(
            &canonical_file(&helper)?,
            &directory.join(helper.file_name().ok_or("Invalid helper name")?),
        )?;
        make_executable(&directory.join(helper.file_name().ok_or("Invalid helper name")?))?;
        #[cfg(windows)]
        for name in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
            let runtime = helper
                .parent()
                .ok_or("Missing helper directory")?
                .join(name);
            if runtime.is_file() {
                copy_synced(&canonical_file(&runtime)?, &directory.join(name))?;
            }
        }
        let plan_path = directory.join("plan.json");
        atomic_json(&plan_path, &plan)?;
        write_outcome(
            &directory,
            &plan,
            OutcomeStatus::Prepared,
            "Update verified and prepared; application is still running",
            None,
            None,
        )?;
        Ok(PreparedUpdate {
            plan_path,
            outcome_path: directory.join("outcome.json"),
            version: plan.version,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

pub fn launch_prepared_update(prepared: &PreparedUpdate) -> Result<u32, String> {
    let directory = prepared
        .plan_path
        .parent()
        .ok_or("Update plan has no directory")?;
    let helper = directory.join(if cfg!(windows) {
        "opennow-update-helper.exe"
    } else {
        "opennow-update-helper"
    });
    let mut command = Command::new(helper);
    command
        .current_dir(directory)
        .arg("--apply")
        .arg(&prepared.plan_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    let plan: Plan = read_json(&prepared.plan_path, 64 * 1024)?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("Cannot start update helper: {error}");
            write_outcome(
                directory,
                &plan,
                OutcomeStatus::Failed,
                &message,
                None,
                None,
            )?;
            return Err(message);
        }
    };
    let result = (|| {
        let start = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return Err(read_outcome(&prepared.outcome_path)?
                    .map(|outcome| outcome.message)
                    .unwrap_or_else(|| "Update helper stopped before it was ready".to_owned()));
            }
            if read_outcome(&prepared.outcome_path)?
                .is_some_and(|outcome| outcome.status == OutcomeStatus::WaitingForExit)
                && helper_is_running(prepared)?
            {
                return Ok(child.id());
            }
            if start.elapsed() > Duration::from_secs(300) {
                return Err(
                    "Update helper preparation timed out; the application remains running"
                        .to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })();
    if let Err(error) = &result {
        let _ = child.kill();
        let _ = child.wait();
        write_outcome(directory, &plan, OutcomeStatus::Failed, error, None, None)?;
    }
    result
}

pub fn helper_is_running(prepared: &PreparedUpdate) -> Result<bool, String> {
    let path = prepared
        .plan_path
        .parent()
        .ok_or("Invalid update plan directory")?
        .join("apply.lock");
    if !path.exists() {
        return Ok(false);
    }
    let lock = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    match lock.try_lock_exclusive() {
        Ok(()) => Ok(false),
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            Ok(true)
        }
        Err(error) => Err(error.to_string()),
    }
}

pub fn read_outcome(path: &Path) -> Result<Option<UpdateOutcome>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let outcome: UpdateOutcome = read_json(path, 64 * 1024)?;
    if outcome.schema_version != 1 {
        return Err("Unsupported update outcome schema".to_owned());
    }
    Ok(Some(outcome))
}

pub fn acknowledge_startup_from_env(application_version: &str) -> Result<bool, String> {
    let Some(plan_path) = std::env::var_os("OPENNOW_UPDATE_PLAN") else {
        return Ok(false);
    };
    let plan_path = PathBuf::from(plan_path);
    let plan: Plan = read_json(&plan_path, 64 * 1024)?;
    let nonce =
        std::env::var("OPENNOW_UPDATE_NONCE").map_err(|_| "Missing update startup nonce")?;
    if plan.schema_version != 1
        || plan.version != application_version.trim_start_matches('v')
        || nonce != plan.nonce
    {
        return Err(
            "Updated application startup identity does not match the update plan".to_owned(),
        );
    }
    let pid = std::env::var("OPENNOW_APP_PID")
        .map_err(|_| "Missing trusted application PID")?
        .parse()
        .map_err(|_| "Invalid trusted application PID")?;
    let application = ProcessIdentity::capture(pid)?;
    if plan.kind == InstallKind::AppImage {
        let image = std::env::var_os("APPIMAGE")
            .ok_or("Updated process is not running from an AppImage")?;
        if canonical_file(Path::new(&image))? != plan.target {
            return Err("Updated AppImage does not match the installation target".to_owned());
        }
    } else {
        application.matches_executable(&plan.application_executable)?;
    }
    atomic_json(
        &plan_path
            .parent()
            .ok_or("Invalid update plan path")?
            .join("ack.json"),
        &Acknowledgement {
            nonce,
            version: plan.version,
            application,
        },
    )?;
    Ok(true)
}

pub fn run_helper(plan_path: &Path) -> Result<(), String> {
    let plan_path = canonical_file(plan_path)?;
    let directory = plan_path.parent().ok_or("Invalid update plan path")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("apply.lock"))
        .map_err(|error| error.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "Another helper owns this update transaction")?;
    let plan: Plan = read_json(&plan_path, 64 * 1024)?;
    if read_outcome(&directory.join("outcome.json"))?.is_some_and(|outcome| {
        matches!(
            outcome.status,
            OutcomeStatus::Completed | OutcomeStatus::RolledBack | OutcomeStatus::RebootRequired
        )
    }) {
        return Ok(());
    }
    let result = validate_plan(&plan, directory).and_then(|_| apply(&plan, directory));
    if let Err(error) = &result {
        let outcome = read_outcome(&directory.join("outcome.json"))?;
        if outcome.as_ref().is_none_or(|outcome| {
            !matches!(
                outcome.status,
                OutcomeStatus::RolledBack
                    | OutcomeStatus::Failed
                    | OutcomeStatus::ManagedPending
                    | OutcomeStatus::RebootRequired
            )
        }) {
            write_outcome(
                directory,
                &plan,
                OutcomeStatus::Failed,
                error,
                outcome
                    .as_ref()
                    .and_then(|outcome| outcome.installed_version.clone()),
                outcome.and_then(|outcome| outcome.restarted_process),
            )?;
        }
    }
    result
}

fn validate_plan(plan: &Plan, directory: &Path) -> Result<(), String> {
    if plan.schema_version != 1
        || plan.nonce.len() != 64
        || !plan.nonce.bytes().all(|value| value.is_ascii_hexdigit())
        || plan.processes.len() != 2
    {
        return Err("Invalid update plan schema or process identities".to_owned());
    }
    if !plan.target.is_absolute()
        || !plan.application_executable.is_absolute()
        || !plan.data_dir.is_absolute()
        || plan.package.parent() != Some(directory)
        || (plan.kind != InstallKind::AppImage
            && installation_target(plan.kind, &plan.application_executable)? != plan.target)
        || (plan.data_dir.starts_with(&plan.target) && plan.kind != InstallKind::WindowsPortable)
        || plan.target.starts_with(&plan.data_dir)
    {
        return Err("Invalid update installation paths".to_owned());
    }
    if !matches!(
        plan.kind,
        InstallKind::WindowsMsi | InstallKind::DebianPackage
    ) && plan.target.parent() != directory.parent()
    {
        return Err("Update payload is not beside its installation".to_owned());
    }
    if plan.processes[0].executable != plan.application_executable {
        return Err("Update process identity does not match the application".to_owned());
    }
    let manifest = read_manifest(&plan.package)?;
    if manifest.version.trim_start_matches('v') != plan.version
        || detect_install_kind(&plan.application_executable, &plan.package)? != plan.kind
    {
        return Err("Update plan does not match the signed manifest".to_owned());
    }
    verification::verify_package(&plan.package, &manifest)
}

fn apply(plan: &Plan, directory: &Path) -> Result<(), String> {
    let previous =
        read_outcome(&directory.join("outcome.json"))?.ok_or("Missing prepared update outcome")?;
    if previous.status != OutcomeStatus::Prepared {
        if matches!(
            previous.status,
            OutcomeStatus::BackingUp
                | OutcomeStatus::Installing
                | OutcomeStatus::AwaitingStartup
                | OutcomeStatus::Failed
        ) && !matches!(
            plan.kind,
            InstallKind::WindowsMsi | InstallKind::DebianPackage
        ) {
            if previous.restarted_process.is_none()
                && (previous.status == OutcomeStatus::AwaitingStartup
                    || (previous.status == OutcomeStatus::Failed
                        && directory.join("previous").exists()))
            {
                return Err("Interrupted startup has no confirmed process identity; automatic rollback is unsafe".to_owned());
            }
            if previous
                .restarted_process
                .as_ref()
                .map(process::owned_tree_is_running)
                .transpose()?
                .unwrap_or(false)
            {
                return Err(
                    "Interrupted update still has a running application; close it before recovery"
                        .to_owned(),
                );
            }
            rollback(plan, directory)?;
            let message = match restart_previous(plan) {
                Ok(_) => "Recovered an interrupted replacement; previous installation restored and restart requested".to_owned(),
                Err(error) => format!("Recovered an interrupted replacement; previous installation restored but restart failed: {error}"),
            };
            write_outcome(
                directory,
                plan,
                OutcomeStatus::RolledBack,
                &message,
                None,
                None,
            )?;
            return Ok(());
        }
        return Err("Update transaction has already started or completed".to_owned());
    }
    let payload = prepare_payload(plan, &directory.join("payload"))?;
    let preserved = preserve_portable_data(plan, &payload)?;
    managed::verify_identity(
        plan.kind,
        &plan.package,
        &plan.target,
        &plan.version,
        plan.managed_identity.as_ref(),
    )?;
    write_outcome(
        directory,
        plan,
        OutcomeStatus::WaitingForExit,
        "Helper independently verified the signed update and is waiting for application shutdown",
        None,
        None,
    )?;
    process::wait_for_exit(&plan.processes, Duration::from_secs(90))?;
    let refresh = (|| {
        for (_, destination) in preserved {
            remove_path(&destination)?;
        }
        preserve_portable_data(plan, &payload).map(|_| ())
    })();
    if let Err(error) = refresh {
        let _ = restart_previous(plan);
        return Err(error);
    }
    if matches!(
        plan.kind,
        InstallKind::WindowsMsi | InstallKind::DebianPackage
    ) {
        write_outcome(
            directory,
            plan,
            OutcomeStatus::Installing,
            "Native package manager is installing the verified update",
            None,
            None,
        )?;
        return managed::install(plan, directory);
    }
    replace_and_restart(plan, directory, &payload, Duration::from_secs(90))
}

fn replace_and_restart(
    plan: &Plan,
    directory: &Path,
    payload: &Path,
    startup_timeout: Duration,
) -> Result<(), String> {
    write_outcome(
        directory,
        plan,
        OutcomeStatus::BackingUp,
        "Preserving the previous installation",
        None,
        None,
    )?;
    let installed = rename_synced(&plan.target, &directory.join("previous")).and_then(|_| {
        write_outcome(
            directory,
            plan,
            OutcomeStatus::Installing,
            "Replacing the complete installation",
            None,
            None,
        )?;
        rename_synced(payload, &plan.target)?;
        if plan.kind == InstallKind::MacBundle {
            bundle::verify_bundle(&plan.target, &directory.join("previous"))?;
        }
        Ok(())
    });
    let result = installed
        .map_err(RestartFailure::RollbackSafe)
        .and_then(|_| restart_and_acknowledge(plan, directory, startup_timeout));
    if let Err(error) = result {
        return recover_replacement(plan, directory, error);
    }
    write_outcome(
        directory,
        plan,
        OutcomeStatus::Completed,
        "Updated application acknowledged healthy startup",
        Some(plan.version.clone()),
        None,
    )?;
    cleanup_completed(plan, directory)?;
    Ok(())
}

fn recover_replacement(
    plan: &Plan,
    directory: &Path,
    failure: RestartFailure,
) -> Result<(), String> {
    let error = match failure {
        RestartFailure::ProcessesMayBeRunning(error) => return Err(error),
        RestartFailure::RollbackSafe(error) => error,
    };
    if let Err(rollback_error) = rollback(plan, directory) {
        let message = format!(
            "{error}; rollback failed: {rollback_error}. Previous installation remains in {}",
            directory.join("previous").display()
        );
        write_outcome(directory, plan, OutcomeStatus::Failed, &message, None, None)?;
        return Err(message);
    }
    let restart = restart_previous(plan);
    let message = format!(
        "{error}; previous installation restored{}",
        restart
            .err()
            .map(|error| format!(" but restart failed: {error}"))
            .unwrap_or_default()
    );
    write_outcome(
        directory,
        plan,
        OutcomeStatus::RolledBack,
        &message,
        None,
        None,
    )?;
    Err(message)
}

fn cleanup_completed(plan: &Plan, directory: &Path) -> Result<(), String> {
    for path in [
        directory.join("previous"),
        directory.join("payload"),
        plan.package.clone(),
        manifest_path(&plan.package)?,
    ] {
        if let Err(error) = remove_path(&path) {
            write_outcome(
                directory,
                plan,
                OutcomeStatus::Completed,
                &format!(
                    "Updated application acknowledged healthy startup; cleanup of {} was deferred: {error}",
                    path.display()
                ),
                Some(plan.version.clone()),
                None,
            )?;
            return Ok(());
        }
    }
    Ok(())
}

fn rollback(plan: &Plan, directory: &Path) -> Result<(), String> {
    let backup = directory.join("previous");
    if !backup.exists() {
        if plan.target.exists() {
            return Ok(());
        }
        return Err("Previous installation is missing".to_owned());
    }
    if plan.target.exists() {
        rename_synced(&plan.target, &directory.join("failed"))?;
    }
    rename_synced(&backup, &plan.target)
}

fn restart_previous(plan: &Plan) -> Result<Child, String> {
    Command::new(restart_executable(plan))
        .current_dir(
            restart_executable(plan)
                .parent()
                .ok_or("Application executable has no directory")?,
        )
        .env("OPENNOW_DATA_DIR", &plan.data_dir)
        .env_remove("APPIMAGE")
        .env_remove("APPDIR")
        .env_remove("ARGV0")
        .env_remove("OPENNOW_UPDATE_PLAN")
        .env_remove("OPENNOW_UPDATE_NONCE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())
}

fn restart_executable(plan: &Plan) -> &Path {
    if plan.kind == InstallKind::AppImage {
        &plan.target
    } else {
        &plan.application_executable
    }
}

fn restart_and_acknowledge(
    plan: &Plan,
    directory: &Path,
    timeout: Duration,
) -> Result<(), RestartFailure> {
    let mut command = Command::new(restart_executable(plan));
    command
        .env("OPENNOW_DATA_DIR", &plan.data_dir)
        .current_dir(restart_executable(plan).parent().ok_or_else(|| {
            RestartFailure::RollbackSafe("Application executable has no directory".to_owned())
        })?)
        .env_remove("APPIMAGE")
        .env_remove("APPDIR")
        .env_remove("ARGV0")
        .env("OPENNOW_UPDATE_PLAN", directory.join("plan.json"))
        .env("OPENNOW_UPDATE_NONCE", &plan.nonce)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    write_outcome(
        directory,
        plan,
        OutcomeStatus::AwaitingStartup,
        "Preparing to start the updated application",
        Some(plan.version.clone()),
        None,
    )
    .map_err(RestartFailure::RollbackSafe)?;
    let mut application = process::OwnedApplication::start(&mut command)?;
    let result = (|| {
        write_outcome(
            directory,
            plan,
            OutcomeStatus::AwaitingStartup,
            "Updated application started; waiting for healthy UI and core acknowledgement",
            Some(plan.version.clone()),
            Some(application.identity.clone()),
        )?;
        let started = Instant::now();
        loop {
            if application
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                break Err("Updated application exited before healthy startup".to_owned());
            }
            if directory.join("ack.json").exists() {
                match read_json::<Acknowledgement>(&directory.join("ack.json"), 64 * 1024) {
                    Ok(ack)
                        if ack.nonce == plan.nonce
                            && ack.version == plan.version
                            && application.owns(&ack.application)?
                            && (plan.kind == InstallKind::AppImage
                                || ack.application.executable == plan.application_executable)
                            && ack.application.is_running()? =>
                    {
                        break Ok(());
                    }
                    _ => {
                        break Err(
                            "Updated application returned an invalid startup acknowledgement"
                                .to_owned(),
                        );
                    }
                }
            }
            if started.elapsed() >= timeout {
                break Err(
                    "Updated application did not acknowledge healthy startup before the deadline"
                        .to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })();
    let Err(error) = result else {
        return Ok(());
    };
    let stopped = application.stop();
    Err(failed_restart(error, stopped, |message| {
        write_outcome(
            directory,
            plan,
            OutcomeStatus::Failed,
            message,
            Some(plan.version.clone()),
            Some(application.identity.clone()),
        )
    }))
}

fn failed_restart(
    error: String,
    stopped: Result<(), String>,
    record: impl FnOnce(&str) -> Result<(), String>,
) -> RestartFailure {
    match stopped {
        Ok(()) => RestartFailure::RollbackSafe(error),
        Err(shutdown_error) => {
            let message = format!(
                "{error}; {shutdown_error}; rollback was not attempted while updated processes may still be running"
            );
            let _ = record(&message);
            RestartFailure::ProcessesMayBeRunning(message)
        }
    }
}

fn prepare_payload(plan: &Plan, destination: &Path) -> Result<PathBuf, String> {
    match plan.kind {
        InstallKind::AppImage => {
            let mut file = File::open(&plan.package).map_err(|error| error.to_string())?;
            let mut magic = [0u8; 11];
            file.read_exact(&mut magic)
                .map_err(|error| error.to_string())?;
            if &magic[..4] != b"\x7fELF" || &magic[8..11] != b"AI\x02" {
                return Err("Signed package is not a type-2 AppImage".to_owned());
            }
            validate_executable_architecture(&plan.package, plan.kind)?;
            copy_synced(&plan.package, destination)?;
            make_executable(destination)?;
            Ok(destination.to_path_buf())
        }
        InstallKind::WindowsPortable | InstallKind::MacBundle => {
            if plan.kind == InstallKind::MacBundle
                && plan
                    .package
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case("dmg"))
            {
                bundle::extract_dmg(&plan.package, destination)?;
            } else {
                archive::extract_zip(
                    &plan.package,
                    destination,
                    plan.kind == InstallKind::MacBundle,
                )?;
            }
            let relative = plan
                .application_executable
                .strip_prefix(&plan.target)
                .map_err(|error| error.to_string())?;
            let candidates = std::iter::once(destination.to_path_buf()).chain(
                fs::read_dir(destination)
                    .map_err(|error| error.to_string())?
                    .filter_map(|entry| entry.ok().map(|entry| entry.path())),
            );
            let roots: Vec<_> = candidates
                .filter(|root| root.join(relative).is_file())
                .collect();
            if roots.len() != 1 {
                return Err(
                    "Update archive does not contain exactly one expected application root"
                        .to_owned(),
                );
            }
            let root = roots.into_iter().next().ok_or("Missing update root")?;
            let executable = root.join(relative);
            canonical_file(&executable)?;
            let core = executable
                .parent()
                .ok_or("Missing update executable directory")?
                .join(if cfg!(windows) {
                    "opennow-core.exe"
                } else {
                    "opennow-core"
                });
            canonical_file(&core)?;
            let helper = executable
                .parent()
                .ok_or("Missing update executable directory")?
                .join(if cfg!(windows) {
                    "opennow-update-helper.exe"
                } else {
                    "opennow-update-helper"
                });
            canonical_file(&helper)?;
            for path in [&executable, &core, &helper] {
                validate_executable_architecture(path, plan.kind)?;
            }
            if plan.kind == InstallKind::MacBundle {
                bundle::verify_bundle(&root, &plan.target)?;
            }
            Ok(root)
        }
        InstallKind::WindowsMsi | InstallKind::DebianPackage => Ok(plan.package.clone()),
    }
}

fn validate_executable_architecture(path: &Path, kind: InstallKind) -> Result<(), String> {
    use std::io::{Seek, SeekFrom};
    if kind == InstallKind::MacBundle {
        #[cfg(target_os = "macos")]
        {
            let architecture = if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x86_64"
            };
            if !command_output(
                Command::new("/usr/bin/lipo")
                    .arg(path)
                    .args(["-verify_arch", architecture]),
                Duration::from_secs(120),
            )?
            .status
            .success()
            {
                return Err(
                    "Application bundle does not contain this processor architecture".to_owned(),
                );
            }
        }
        return Ok(());
    }
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if kind == InstallKind::AppImage {
        let expected: u16 = if cfg!(target_arch = "aarch64") {
            183
        } else {
            62
        };
        if header[4] != 2
            || header[5] != 1
            || u16::from_le_bytes([header[18], header[19]]) != expected
        {
            return Err("AppImage architecture does not match this installation".to_owned());
        }
        return Ok(());
    }
    if &header[..2] != b"MZ" {
        return Err("Windows package contains a non-PE executable".to_owned());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().map_err(|_| "Invalid PE header")?);
    if offset > 1024 * 1024 {
        return Err("Windows PE header exceeds its bound".to_owned());
    }
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|error| error.to_string())?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|error| error.to_string())?;
    let expected: u16 = if cfg!(target_arch = "aarch64") {
        0xaa64
    } else {
        0x8664
    };
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != expected {
        return Err("Windows executable architecture does not match this installation".to_owned());
    }
    Ok(())
}

fn preserve_portable_data(plan: &Plan, payload: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    if plan.kind != InstallKind::WindowsPortable {
        return Ok(Vec::new());
    }
    let mut sources = Vec::new();
    for (index, entry) in fs::read_dir(&plan.target)
        .map_err(|error| error.to_string())?
        .enumerate()
    {
        if index as u64 >= bundle::MAX_COPY_ENTRIES {
            return Err(
                "Portable installation has too many root entries to preserve safely".to_owned(),
            );
        }
        let entry = entry.map_err(|error| error.to_string())?;
        if !matches!(
            entry.file_name().to_str(),
            Some("bin" | "share" | "LICENSE" | "THIRD_PARTY_NOTICES.json")
        ) {
            sources.push(entry.path());
        }
    }
    if plan.data_dir.starts_with(&plan.target)
        && !sources.iter().any(|path| plan.data_dir.starts_with(path))
    {
        sources.push(plan.data_dir.clone());
    }
    let mut copied = Vec::new();
    let mut budget = bundle::CopyBudget::new();
    for source in sources {
        let destination = payload.join(
            source
                .strip_prefix(&plan.target)
                .map_err(|error| error.to_string())?,
        );
        if destination.exists() {
            return Err(format!(
                "Update package overlaps preserved user data at {}; move that data outside the installation before updating",
                source.display()
            ));
        }
        copy_user_data(&source, &destination, &mut budget)?;
        copied.push((source, destination));
    }
    Ok(copied)
}

fn copy_user_data(
    source: &Path,
    destination: &Path,
    budget: &mut bundle::CopyBudget,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    fs::create_dir_all(destination.parent().ok_or("User data has no parent")?)
        .map_err(|error| error.to_string())?;
    if metadata.is_dir() {
        return bundle::copy_tree(source, destination, budget, bundle::CopyPolicy::UserData);
    }
    if !metadata.is_file() || metadata.len() > verification::MAXIMUM_UPDATE_BYTES {
        return Err(
            "Portable user data must contain bounded regular files or internal relative symlinks"
                .to_owned(),
        );
    }
    budget.consume_entry()?;
    budget.copy_file(source, destination, bundle::CopyPolicy::UserData)
}

fn installation_target(kind: InstallKind, executable: &Path) -> Result<PathBuf, String> {
    let parent = executable.parent().ok_or("Application has no directory")?;
    match kind {
        InstallKind::AppImage => {
            let image = std::env::var_os("APPIMAGE")
                .map(PathBuf::from)
                .unwrap_or_else(|| executable.to_path_buf());
            canonical_file(&image)
        }
        InstallKind::MacBundle => {
            if parent.file_name().is_none_or(|name| name != "MacOS")
                || parent
                    .parent()
                    .and_then(Path::file_name)
                    .is_none_or(|name| name != "Contents")
            {
                return Err("Application is not inside a standard macOS bundle".to_owned());
            }
            let root = parent
                .parent()
                .and_then(Path::parent)
                .ok_or("Invalid app bundle")?;
            if root.extension().is_none_or(|extension| extension != "app") {
                return Err("Application bundle has no .app suffix".to_owned());
            }
            Ok(root.to_path_buf())
        }
        InstallKind::WindowsPortable | InstallKind::WindowsMsi => {
            if parent.file_name().is_none_or(|name| name != "bin")
                || executable
                    .file_name()
                    .is_none_or(|name| name != "OpenNOW.exe")
            {
                return Err(
                    "Application does not use the supported Windows bin/OpenNOW.exe layout"
                        .to_owned(),
                );
            }
            Ok(parent
                .parent()
                .ok_or("Invalid Windows installation")?
                .to_path_buf())
        }
        InstallKind::DebianPackage => Ok(executable.to_path_buf()),
    }
}

fn read_manifest(package: &Path) -> Result<verification::UpdateManifest, String> {
    let mut bytes = Vec::new();
    File::open(canonical_file(&manifest_path(package)?)?)
        .map_err(|error| error.to_string())?
        .take(verification::MAXIMUM_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    verification::verify_signed_manifest(&bytes)
}

fn manifest_path(package: &Path) -> Result<PathBuf, String> {
    Ok(package.with_file_name(format!(
        "{}.manifest.json",
        package
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("Invalid update package name")?
    )))
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    if !fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Expected a regular file, not a link or directory".to_owned());
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

fn copy_synced(source: &Path, destination: &Path) -> Result<(), String> {
    copy_synced_bounded(
        source,
        destination,
        verification::MAXIMUM_UPDATE_BYTES,
        bundle::CopyPolicy::Bundle,
    )
    .map(|_| ())
}

fn copy_synced_bounded(
    source: &Path,
    destination: &Path,
    maximum: u64,
    policy: bundle::CopyPolicy,
) -> Result<u64, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut input = options.open(source).map_err(|error| error.to_string())?;
    let metadata = input.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(
            "Update copy exceeds its remaining byte budget or is not a regular file".to_owned(),
        );
    }
    let mut output_options = OpenOptions::new();
    output_options.write(true).create_new(true);
    #[cfg(unix)]
    if policy == bundle::CopyPolicy::UserData {
        use std::os::unix::fs::OpenOptionsExt;
        output_options.mode(0o600);
    }
    let mut output = output_options
        .open(destination)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        #[cfg(windows)]
        if policy == bundle::CopyPolicy::UserData {
            security::preserve_permissions(source, destination)?;
        }
        let copied = copy_bounded(&mut input, &mut output, maximum)?;
        #[cfg(unix)]
        if policy == bundle::CopyPolicy::UserData {
            security::preserve_permissions(source, destination)?;
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(copied)
    })();
    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn copy_bounded(
    input: &mut impl Read,
    output: &mut impl Write,
    maximum: u64,
) -> Result<u64, String> {
    let copied = std::io::copy(&mut (&mut *input).take(maximum), output)
        .map_err(|error| error.to_string())?;
    let mut overflow = [0u8; 1];
    if input
        .read(&mut overflow)
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err("Update copy exceeds its remaining streaming byte budget".to_owned());
    }
    Ok(copied)
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let _ = path;
    Ok(())
}

fn command_output(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("Missing command stdout")?;
    let stderr = child.stderr.take().ok_or("Missing command stderr")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    let read = |index: usize, stream: Box<dyn Read + Send>| {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut bytes = Vec::new();
                stream
                    .take(64 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if bytes.len() > 64 * 1024 {
                    return Err("Native package command output exceeds its limit".to_owned());
                }
                Ok(bytes)
            })();
            let _ = sender.send((index, result));
        });
    };
    read(0, Box::new(stdout));
    read(1, Box::new(stderr));
    drop(sender);
    let start = Instant::now();
    let result = (|| {
        let mut streams = [None, None];
        let mut status = None;
        loop {
            if status.is_none() {
                status = child.try_wait().map_err(|error| error.to_string())?;
            }
            while let Ok((index, result)) = receiver.try_recv() {
                streams[index] = Some(result?);
            }
            if let (Some(status), true) = (status, streams.iter().all(Option::is_some)) {
                return Ok(std::process::Output {
                    status,
                    stdout: streams[0].take().ok_or("Missing command stdout")?,
                    stderr: streams[1].take().ok_or("Missing command stderr")?,
                });
            }
            if start.elapsed() > timeout {
                return Err("Native package command exceeded its deadline".to_owned());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    if result.is_err() {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, maximum: u64) -> Result<T, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum {
        return Err("Update transaction metadata exceeds its size limit".to_owned());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid update transaction metadata: {error}"))
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temporary = path.with_extension(format!("{:016x}.tmp", rand::random::<u64>()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(value).map_err(|error| error.to_string())?)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            };
            let old: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let new: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    old.as_ptr(),
                    new.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        sync_directory(path.parent().ok_or("Metadata has no parent")?)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn rename_synced(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err("Update rename destination already exists".to_owned());
    }
    fs::rename(source, destination)
        .map_err(|error| format!("Could not replace {}: {error}", source.display()))?;
    sync_directory(source.parent().ok_or("Missing source parent")?)?;
    sync_directory(destination.parent().ok_or("Missing destination parent")?)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    let _ = path;
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
    .map_err(|error| error.to_string())
}

fn write_outcome(
    directory: &Path,
    plan: &Plan,
    status: OutcomeStatus,
    message: &str,
    installed_version: Option<String>,
    restarted_process: Option<ProcessIdentity>,
) -> Result<(), String> {
    atomic_json(
        &directory.join("outcome.json"),
        &UpdateOutcome {
            schema_version: 1,
            version: plan.version.clone(),
            status,
            message: message.to_owned(),
            installed_version,
            restarted_process,
        },
    )
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
