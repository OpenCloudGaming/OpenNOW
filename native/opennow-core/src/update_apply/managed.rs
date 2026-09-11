use super::{InstallKind, OutcomeStatus, Plan, write_outcome};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    package: String,
    version: String,
    architecture: String,
    installed_product: String,
}

pub(super) fn prepare(
    kind: InstallKind,
    package: &Path,
    target: &Path,
    version: &str,
) -> Result<Option<Identity>, String> {
    match kind {
        InstallKind::DebianPackage => deb_identity(package, target, version).map(Some),
        InstallKind::WindowsMsi => msi_identity(package, target, version).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn verify_identity(
    kind: InstallKind,
    package: &Path,
    target: &Path,
    version: &str,
    expected: Option<&Identity>,
) -> Result<(), String> {
    if prepare(kind, package, target, version)?.as_ref() != expected {
        return Err("Native package identity changed after preparation".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn output(command: &mut Command) -> Result<String, String> {
    let result = super::command_output(command, Duration::from_secs(120))?;
    if !result.status.success() {
        return Err("Native package identity query failed".to_owned());
    }
    if result.stdout.len() > 64 * 1024 {
        return Err("Native package query exceeded its limit".to_owned());
    }
    String::from_utf8(result.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn deb_identity(package: &Path, target: &Path, version: &str) -> Result<Identity, String> {
    if !Path::new("/usr/bin/pkexec").is_file() {
        return Err(
            "DEB updates require PolicyKit's pkexec and an interactive authorization agent"
                .to_owned(),
        );
    }
    let field = |name| {
        output(
            Command::new("/usr/bin/dpkg-deb")
                .arg("--field")
                .arg(package)
                .arg(name),
        )
    };
    let name = field("Package")?;
    let architecture = field("Architecture")?;
    let expected_arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        return Err("Unsupported DEB architecture".to_owned());
    };
    let version = version
        .trim_start_matches('v')
        .replace("-nightly.", "~nightly.")
        .replace("-supporter.", "~supporter.");
    if name != "opennow" || architecture != expected_arch || field("Version")? != version {
        return Err(
            "DEB package identity does not match OpenNOW, architecture, or signed version"
                .to_owned(),
        );
    }
    let owner = output(
        Command::new("/usr/bin/dpkg-query")
            .arg("--search")
            .arg(target),
    )?;
    if !owner.lines().any(|line| {
        line.strip_prefix("opennow: ")
            .is_some_and(|path| Path::new(path) == target)
    }) {
        return Err(
            "Running application is not owned by the installed OpenNOW DEB package".to_owned(),
        );
    }
    let installed = output(Command::new("/usr/bin/dpkg-query").args([
        "--show",
        "--showformat=${db:Status-Status}\t${Architecture}",
        "opennow",
    ]))?;
    if installed != format!("installed\t{expected_arch}") {
        return Err("Installed OpenNOW DEB is not configured for this architecture".to_owned());
    }
    Ok(Identity {
        package: name,
        version,
        architecture,
        installed_product: "opennow".to_owned(),
    })
}

#[cfg(not(target_os = "linux"))]
fn deb_identity(_: &Path, _: &Path, _: &str) -> Result<Identity, String> {
    Err("DEB updates require Linux".to_owned())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::*;
    pub(super) fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    pub(super) fn argument_path(path: &Path) -> Result<String, String> {
        let value = path
            .to_str()
            .ok_or("Windows Installer requires a Unicode installation path")?;
        if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
            return Ok(format!("\\\\{unc}"));
        }
        Ok(value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned())
    }

    struct Handle(u32);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                MsiCloseHandle(self.0);
            }
        }
    }

    pub(super) fn property(package: &Path, name: &str) -> Result<String, String> {
        let mut database = 0;
        if unsafe { MsiOpenDatabaseW(wide(package).as_ptr(), std::ptr::null(), &mut database) } != 0
        {
            return Err("Cannot read signed MSI database".to_owned());
        }
        let database = Handle(database);
        let mut view = 0;
        let query = format!("SELECT `Value` FROM `Property` WHERE `Property`='{name}'");
        if unsafe { MsiDatabaseOpenViewW(database.0, wide(query).as_ptr(), &mut view) } != 0 {
            return Err("Cannot query MSI identity".to_owned());
        }
        let view = Handle(view);
        if unsafe { MsiViewExecute(view.0, 0) } != 0 {
            return Err("Cannot execute MSI identity query".to_owned());
        }
        let mut record = 0;
        if unsafe { MsiViewFetch(view.0, &mut record) } != 0 {
            return Err(format!("MSI identity property {name} is missing"));
        }
        let record = Handle(record);
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        if unsafe { MsiRecordGetStringW(record.0, 1, buffer.as_mut_ptr(), &mut size) } != 0 {
            return Err("Cannot read MSI identity property".to_owned());
        }
        String::from_utf16(&buffer[..size as usize]).map_err(|error| error.to_string())
    }

    pub(super) fn registered(product: &str, property: &str) -> Result<String, String> {
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        if unsafe {
            MsiGetProductInfoW(
                wide(product).as_ptr(),
                wide(property).as_ptr(),
                buffer.as_mut_ptr(),
                &mut size,
            )
        } != 0
        {
            return Err("Cannot read registered MSI installation identity".to_owned());
        }
        String::from_utf16(&buffer[..size as usize]).map_err(|error| error.to_string())
    }

    fn architecture(package: &Path) -> Result<String, String> {
        let mut summary = 0;
        if unsafe { MsiGetSummaryInformationW(0, wide(package).as_ptr(), 0, &mut summary) } != 0 {
            return Err("Cannot read MSI architecture".to_owned());
        }
        let summary = Handle(summary);
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let mut kind = 0;
        let mut integer = 0;
        let mut time = windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        if unsafe {
            MsiSummaryInfoGetPropertyW(
                summary.0,
                7,
                &mut kind,
                &mut integer,
                &mut time,
                buffer.as_mut_ptr(),
                &mut size,
            )
        } != 0
        {
            return Err("Cannot read MSI architecture template".to_owned());
        }
        Ok(String::from_utf16(&buffer[..size as usize])
            .map_err(|error| error.to_string())?
            .split(';')
            .next()
            .unwrap_or_default()
            .to_owned())
    }

    pub(super) fn family(version: &str) -> Result<&'static str, String> {
        let version = semver::Version::parse(version.trim_start_matches('v'))
            .map_err(|error| error.to_string())?;
        if version.pre.is_empty() {
            Ok("{6E81F7AE-B19D-4E87-A94A-2B2F01EBF762}")
        } else if version.pre.as_str().starts_with("nightly.") {
            Ok("{9661F4F8-656C-4B64-9035-01B04F4822B1}")
        } else if version.pre.as_str().starts_with("supporter.") {
            Ok("{B3AF8A40-5F44-445A-AD99-CECE00593601}")
        } else {
            Err("Unsupported MSI release channel".to_owned())
        }
    }

    pub(super) fn installed_at(target: &Path, upgrade: &str) -> Result<Option<String>, String> {
        for index in 0..256 {
            let mut product = [0u16; 39];
            let status = unsafe {
                MsiEnumRelatedProductsW(wide(upgrade).as_ptr(), 0, index, product.as_mut_ptr())
            };
            if status == 259 {
                return Ok(None);
            }
            if status != 0 {
                return Err("Cannot enumerate registered MSI products".to_owned());
            }
            let product = String::from_utf16(&product[..38]).map_err(|error| error.to_string())?;
            let location = registered(&product, "InstallLocation")?;
            if location.is_empty() {
                return Err("An OpenNOW MSI registration has no installation location; repair it before updating".to_owned());
            }
            let location = std::fs::canonicalize(location).map_err(|_| "An OpenNOW MSI registration points to an unavailable installation; repair it before updating")?;
            if location == target {
                return Ok(Some(product));
            }
        }
        Err("Too many registered MSI products".to_owned())
    }

    pub(super) fn identity(
        package: &Path,
        target: &Path,
        version: &str,
    ) -> Result<Identity, String> {
        let upgrade = family(version)?;
        let parsed = semver::Version::parse(version.trim_start_matches('v'))
            .map_err(|error| error.to_string())?;
        let (name, expected_version) = if parsed.pre.is_empty() {
            (
                "OpenNOW",
                format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch),
            )
        } else {
            let parts: Vec<_> = parsed.pre.as_str().split('.').collect();
            if parts.len() != 3 {
                return Err("Invalid MSI channel version".to_owned());
            }
            let run: u32 = parts[1].parse().map_err(|_| "Invalid MSI release run")?;
            let attempt: u32 = parts[2]
                .parse()
                .map_err(|_| "Invalid MSI release attempt")?;
            if !(1..=65535).contains(&run) || !(1..=65535).contains(&attempt) {
                return Err("MSI release sequence is outside its supported range".to_owned());
            }
            (
                if parts[0] == "nightly" {
                    "OpenNOW Nightly"
                } else {
                    "OpenNOW Supporter"
                },
                format!("{}.{}.{}", run / 256, run % 256, attempt),
            )
        };
        if !property(package, "UpgradeCode")?.eq_ignore_ascii_case(upgrade)
            || property(package, "ProductName")? != name
            || property(package, "Manufacturer")? != "OpenCloudGaming"
        {
            return Err("MSI product family or channel does not match OpenNOW".to_owned());
        }
        let arch = architecture(package)?;
        let expected_arch = if cfg!(target_arch = "aarch64") {
            "Arm64"
        } else {
            "x64"
        };
        if !arch.eq_ignore_ascii_case(expected_arch) {
            return Err("MSI architecture does not match this installation".to_owned());
        }
        let installed_product = installed_at(target, upgrade)?
            .ok_or("No matching Windows Installer registration owns this application")?;
        let cached = registered(&installed_product, "LocalPackage")?;
        if architecture(Path::new(&cached))? != arch {
            return Err("MSI update architecture differs from the installed product".to_owned());
        }
        let numeric = property(package, "ProductVersion")?;
        if numeric != expected_version {
            return Err("MSI product version does not match the signed release version".to_owned());
        }
        Ok(Identity {
            package: property(package, "ProductCode")?,
            version: numeric,
            architecture: arch,
            installed_product,
        })
    }
}

#[cfg(windows)]
fn msi_identity(package: &Path, target: &Path, version: &str) -> Result<Identity, String> {
    windows::identity(package, target, version)
}
#[cfg(not(windows))]
fn msi_identity(_: &Path, _: &Path, _: &str) -> Result<Identity, String> {
    Err("MSI updates require Windows".to_owned())
}

pub(super) fn windows_managed(target: &Path) -> Result<bool, String> {
    #[cfg(windows)]
    {
        for version in ["1.0.0", "1.0.0-nightly.1.1", "1.0.0-supporter.1.1"] {
            if windows::installed_at(target, windows::family(version)?)?.is_some() {
                return Ok(true);
            }
        }
    }
    let _ = target;
    Ok(false)
}

pub(super) fn install(plan: &Plan, directory: &Path) -> Result<(), String> {
    let identity = plan
        .managed_identity
        .as_ref()
        .ok_or("Missing prepared managed-package identity")?;
    if matches!(
        plan.kind,
        InstallKind::WindowsMsi | InstallKind::DebianPackage
    ) {
        super::recovery::record_install_boot(directory)?;
    }
    let mut command = match plan.kind {
        InstallKind::DebianPackage => {
            let mut command = Command::new("/usr/bin/pkexec");
            command
                .args(["/usr/bin/dpkg", "--install"])
                .arg(&plan.package);
            command
        }
        InstallKind::WindowsMsi => {
            let system =
                std::env::var_os("SystemRoot").ok_or("Windows system directory is unavailable")?;
            let mut command = Command::new(Path::new(&system).join("System32/msiexec.exe"));
            command.arg("/i");
            #[cfg(windows)]
            command.arg(windows::argument_path(&plan.package)?);
            #[cfg(not(windows))]
            command.arg(&plan.package);
            command.args(["/passive", "/norestart", "REBOOT=ReallySuppress"]);
            #[cfg(windows)]
            command.arg(format!(
                "INSTALL_ROOT={}",
                windows::argument_path(&plan.target)?
            ));
            #[cfg(not(windows))]
            command.arg(format!("INSTALL_ROOT={}", plan.target.display()));
            command
        }
        _ => return Err("Not a managed package update".to_owned()),
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Cannot start native package manager: {error}"))?;
    let started = Instant::now();
    let code = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status.code().unwrap_or(-1);
        }
        if started.elapsed() > Duration::from_secs(15 * 60) {
            write_outcome(
                directory,
                plan,
                OutcomeStatus::ManagedPending,
                "Native package manager is still running; installation completion is not confirmed. Do not start another update.",
                None,
                super::ProcessIdentity::capture(child.id()).ok(),
            )?;
            return Err("Native installer exceeded the monitoring deadline; it was not killed or reported as successful".to_owned());
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    if plan.kind == InstallKind::WindowsMsi && matches!(code, 3010 | 1641) {
        write_outcome(
            directory,
            plan,
            OutcomeStatus::RebootRequired,
            "Windows Installer requires a system restart; application startup is not yet confirmed",
            Some(identity.version.clone()),
            None,
        )?;
        return Ok(());
    }
    if code != 0 {
        let mut message = format!(
            "Native package manager exited with code {code}; update completion is not confirmed"
        );
        if prepare(plan.kind, &plan.package, &plan.target, &plan.version).is_ok()
            && super::canonical_file(&plan.application_executable).is_ok()
        {
            match super::restart_previous(plan) {
                Ok(_) => message.push_str("; the surviving registered application was restarted"),
                Err(error) => message.push_str(&format!(
                    "; the surviving application could not restart: {error}"
                )),
            }
        } else {
            message
                .push_str("; a usable registered installation could not be confirmed for restart");
        }
        write_outcome(directory, plan, OutcomeStatus::Failed, &message, None, None)?;
        return Err(message);
    }
    verify_installed(plan.kind, identity, &plan.target)?;
    if let Err(error) = super::restart_and_acknowledge(plan, directory, Duration::from_secs(90)) {
        let message = format!(
            "Native package manager installed {}; application restart failed: {error}. No filesystem rollback was attempted for a managed installation.",
            identity.version
        );
        write_outcome(
            directory,
            plan,
            OutcomeStatus::Failed,
            &message,
            Some(identity.version.clone()),
            None,
        )?;
        return Err(message);
    }
    write_outcome(
        directory,
        plan,
        OutcomeStatus::Completed,
        "Native package manager confirmed installation and the updated application acknowledged healthy startup",
        Some(plan.version.clone()),
        None,
    )?;
    super::cleanup_completed(plan, directory)
}

pub(super) fn verify_installed(
    kind: InstallKind,
    identity: &Identity,
    target: &Path,
) -> Result<(), String> {
    let _ = target;
    match kind {
        InstallKind::DebianPackage => {
            let value = super::command_output(
                Command::new("/usr/bin/dpkg-query").args([
                    "--show",
                    "--showformat=${db:Status-Status}\t${Version}\t${Architecture}",
                    &identity.package,
                ]),
                Duration::from_secs(5),
            )?;
            if !value.status.success()
                || value.stdout
                    != format!("installed\t{}\t{}", identity.version, identity.architecture)
                        .as_bytes()
            {
                return Err(
                    "The OpenNOW DEB is not configured at the expected version and architecture."
                        .to_owned(),
                );
            }
            Ok(())
        }
        InstallKind::WindowsMsi => {
            #[cfg(windows)]
            {
                if windows::registered(&identity.package, "VersionString")? != identity.version {
                    return Err(
                        "Windows Installer registration does not confirm the installed version"
                            .to_owned(),
                    );
                }
                if std::fs::canonicalize(windows::registered(&identity.package, "InstallLocation")?)
                    .map_err(|error| error.to_string())?
                    != target
                {
                    return Err(
                        "Windows Installer registered an unexpected installation location"
                            .to_owned(),
                    );
                }
                Ok(())
            }
            #[cfg(not(windows))]
            Err("MSI updates require Windows".to_owned())
        }
        _ => Err("Not a managed package".to_owned()),
    }
}
