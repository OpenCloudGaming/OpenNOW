use super::{InstallKind, OutcomeStatus, Plan, PreparedUpdate, UpdateOutcome, managed};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug)]
pub enum ManagedRecovery {
    Pending(String),
    RebootRequired(String),
    Finished(UpdateOutcome),
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallBoot {
    schema_version: u32,
    boot_id: String,
}

pub(super) fn record_install_boot(directory: &Path) -> Result<(), String> {
    super::atomic_json(
        &directory.join("install-boot.json"),
        &InstallBoot {
            schema_version: 1,
            boot_id: current_boot_id()?,
        },
    )
}

pub fn recover_managed_update(
    prepared: &PreparedUpdate,
    outcome: &UpdateOutcome,
    running_version: &str,
) -> Result<ManagedRecovery, String> {
    match super::helper_is_running(prepared) {
        Ok(true) => {
            return Ok(ManagedRecovery::Pending(
                "The update helper is still finishing the native installation.".to_owned(),
            ));
        }
        Err(error) => {
            return Ok(ManagedRecovery::Pending(format!(
                "Could not confirm that the update helper has finished: {error}"
            )));
        }
        Ok(false) => (),
    }
    let directory = prepared
        .plan_path
        .parent()
        .ok_or("Missing update transaction directory.")?;
    let unknown_installer = match &outcome.restarted_process {
        Some(installer) => match installer.is_running() {
            Ok(true) => return Ok(ManagedRecovery::Pending(
                "The native package manager is still running; update completion is not confirmed."
                    .to_owned(),
            )),
            Err(error) => Some(error),
            Ok(false) => None,
        },
        None if outcome.status == OutcomeStatus::ManagedPending => {
            Some("The native installer process identity was not recorded.".to_owned())
        }
        None => None,
    };
    if let Some(reason) = unknown_installer {
        match reboot_completed(directory) {
            Ok(true) => (),
            Ok(false) => {
                return Ok(ManagedRecovery::Pending(format!(
                    "Native installer completion cannot be confirmed: {reason} Restart your system before starting another update or session."
                )));
            }
            Err(error) => {
                return Ok(ManagedRecovery::Pending(format!(
                    "Native installer completion cannot be confirmed: {reason} The restart boundary also could not be verified: {error}"
                )));
            }
        }
    }
    let plan: Plan = super::read_json(&prepared.plan_path, 64 * 1024)?;
    if plan.schema_version != 1
        || plan.version != prepared.version
        || outcome.version != prepared.version
        || !matches!(
            plan.kind,
            InstallKind::WindowsMsi | InstallKind::DebianPackage
        )
        || !matches!(
            outcome.status,
            OutcomeStatus::ManagedPending | OutcomeStatus::RebootRequired
        )
        || (outcome.status == OutcomeStatus::RebootRequired && plan.kind != InstallKind::WindowsMsi)
    {
        return Err("Persisted managed update identity does not match its outcome.".to_owned());
    }
    let identity = plan
        .managed_identity
        .as_ref()
        .ok_or("Missing persisted native package identity.")?;
    let reboot_completed = if outcome.status == OutcomeStatus::RebootRequired {
        match reboot_completed(directory) {
            Ok(value) => value,
            Err(error) => {
                return Ok(ManagedRecovery::RebootRequired(format!(
                    "Windows Installer requires a restart; reboot completion could not be verified: {error}"
                )));
            }
        }
    } else {
        true
    };
    let recovery = classify_recovery(outcome, running_version, reboot_completed, || {
        managed::verify_installed(plan.kind, identity, &plan.target)
    });
    if let ManagedRecovery::Finished(result) = &recovery {
        super::atomic_json(&prepared.outcome_path, result)?;
    }
    Ok(recovery)
}

fn classify_recovery(
    outcome: &UpdateOutcome,
    running_version: &str,
    reboot_completed: bool,
    verify_installed: impl FnOnce() -> Result<(), String>,
) -> ManagedRecovery {
    if outcome.status == OutcomeStatus::RebootRequired && !reboot_completed {
        return ManagedRecovery::RebootRequired("Windows Installer requires a system restart before all updated files can be confirmed. Sessions remain available, but restart before checking for another update.".to_owned());
    }
    let mut result = outcome.clone();
    result.restarted_process = None;
    result.installed_version = None;
    if let Err(error) = verify_installed() {
        result.status = OutcomeStatus::Failed;
        result.message = format!(
            "The native installer has finished, but the expected installed package could not be verified: {error}"
        );
    } else if running_version.trim_start_matches('v') != outcome.version.trim_start_matches('v') {
        result.status = OutcomeStatus::Failed;
        result.message = format!(
            "The native package is installed, but OpenNOW is running {running_version} instead of {}. Restart OpenNOW before retrying the update.",
            outcome.version
        );
    } else {
        result.status = OutcomeStatus::Completed;
        result.installed_version = Some(outcome.version.clone());
        result.message = format!(
            "Native package identity and running OpenNOW {} were verified after installation.",
            outcome.version
        );
    }
    ManagedRecovery::Finished(result)
}

fn reboot_completed(directory: &Path) -> Result<bool, String> {
    let path = directory.join("install-boot.json");
    if !path.try_exists().map_err(|error| error.to_string())? {
        record_install_boot(directory)?;
        return Ok(false);
    }
    recorded_boot_changed(directory)
}

fn recorded_boot_changed(directory: &Path) -> Result<bool, String> {
    let previous: InstallBoot = super::read_json(&directory.join("install-boot.json"), 1024)?;
    if previous.schema_version != 1 || previous.boot_id.is_empty() {
        return Err("The recorded installation boot identity is invalid.".to_owned());
    }
    Ok(previous.boot_id != current_boot_id()?)
}

#[cfg(windows)]
fn current_boot_id() -> Result<String, String> {
    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessTelemetryIdInformation,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut returned = 0u32;
    unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessTelemetryIdInformation,
            std::ptr::null_mut(),
            0,
            &mut returned,
        );
    }
    if !(64..=1024 * 1024).contains(&returned) {
        return Err("Windows did not provide bounded process telemetry.".to_owned());
    }
    let mut information = vec![0u64; (returned as usize).div_ceil(8)];
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessTelemetryIdInformation,
            information.as_mut_ptr().cast(),
            (information.len() * 8) as u32,
            &mut returned,
        )
    };
    let header_size = information[0] as u32;
    let process_id = (information[0] >> 32) as u32;
    let boot_id = (information[7] >> 32) as u32;
    if status < 0
        || returned < 64
        || header_size < 64
        || header_size > returned
        || process_id != std::process::id()
    {
        return Err("Windows did not provide a valid boot sequence number.".to_owned());
    }
    Ok(format!("windows-boot-sequence:{boot_id}"))
}

#[cfg(target_os = "linux")]
fn current_boot_id() -> Result<String, String> {
    let value = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|error| error.to_string())?;
    let value = value.trim();
    if value.len() != 36
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("The operating system boot identifier is invalid.".to_owned());
    }
    Ok(value.to_owned())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn current_boot_id() -> Result<String, String> {
    Err("Managed reboot recovery is not supported on this platform.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(status: OutcomeStatus) -> UpdateOutcome {
        UpdateOutcome {
            schema_version: 1,
            version: "1.2.3".to_owned(),
            status,
            message: "Native installation is pending".to_owned(),
            installed_version: None,
            restarted_process: None,
        }
    }

    #[test]
    fn recovery_requires_native_identity_and_the_running_version() {
        for status in [OutcomeStatus::ManagedPending, OutcomeStatus::RebootRequired] {
            for (version, installed, expected) in [
                ("1.2.3", Ok(()), OutcomeStatus::Completed),
                ("1.2.2", Ok(()), OutcomeStatus::Failed),
                (
                    "1.2.3",
                    Err("Native registration does not match".to_owned()),
                    OutcomeStatus::Failed,
                ),
            ] {
                let ManagedRecovery::Finished(result) =
                    classify_recovery(&outcome(status), version, true, || installed)
                else {
                    panic!("Finished installer must resolve to an outcome")
                };
                assert_eq!(result.status, expected);
                assert_eq!(
                    result.installed_version.is_some(),
                    expected == OutcomeStatus::Completed
                );
            }
        }
    }

    #[test]
    fn reboot_required_never_claims_success_before_a_proven_reboot() {
        assert!(matches!(
            classify_recovery(
                &outcome(OutcomeStatus::RebootRequired),
                "1.2.3",
                false,
                || panic!("Native package verification must not run before reboot")
            ),
            ManagedRecovery::RebootRequired(_)
        ));
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn legacy_reboot_marker_requires_a_subsequent_boot() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!reboot_completed(directory.path()).unwrap());
        assert!(!reboot_completed(directory.path()).unwrap());
        super::super::atomic_json(
            &directory.path().join("install-boot.json"),
            &InstallBoot {
                schema_version: 1,
                boot_id: "a-previous-boot".to_owned(),
            },
        )
        .unwrap();
        assert!(reboot_completed(directory.path()).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn windows_boot_sequence_is_available_and_stable() {
        let first = current_boot_id().unwrap();
        assert!(first.starts_with("windows-boot-sequence:"));
        assert_eq!(first, current_boot_id().unwrap());
    }
}
