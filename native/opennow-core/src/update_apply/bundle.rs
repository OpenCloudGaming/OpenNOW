use std::path::Path;

pub(super) const MAX_COPY_ENTRIES: u64 = 100_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyPolicy {
    Bundle,
    UserData,
}

pub(super) struct CopyBudget {
    remaining_entries: u64,
    remaining_bytes: u64,
}

impl CopyBudget {
    pub fn new() -> Self {
        Self {
            remaining_entries: MAX_COPY_ENTRIES,
            remaining_bytes: 12 * 1024 * 1024 * 1024,
        }
    }

    #[cfg(test)]
    pub fn with_limits(entries: u64, bytes: u64) -> Self {
        Self {
            remaining_entries: entries,
            remaining_bytes: bytes,
        }
    }

    pub fn consume_entry(&mut self) -> Result<(), String> {
        self.remaining_entries = self
            .remaining_entries
            .checked_sub(1)
            .ok_or("Preserved data exceeds its aggregate entry limit")?;
        Ok(())
    }

    pub fn copy_file(
        &mut self,
        source: &Path,
        destination: &Path,
        policy: CopyPolicy,
    ) -> Result<(), String> {
        let copied = super::copy_synced_bounded(
            source,
            destination,
            self.remaining_bytes
                .min(super::verification::MAXIMUM_UPDATE_BYTES),
            policy,
        )?;
        self.remaining_bytes -= copied;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn run(command: &mut std::process::Command) -> Result<(), String> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err("macOS package operation failed".to_owned())
            };
        }
        if start.elapsed() >= Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("macOS package operation exceeded its deadline".to_owned());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn extract_dmg(package: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let mount = destination.with_extension("mount");
        std::fs::create_dir(&mount).map_err(|error| error.to_string())?;
        let attached = run(Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(package));
        let result = attached.and_then(|_| {
            let apps: Vec<_> = std::fs::read_dir(&mount)
                .map_err(|error| error.to_string())?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.extension().is_some_and(|value| value == "app") && path.is_dir()
                })
                .collect();
            if apps.len() != 1 || apps[0].file_name().is_none_or(|name| name != "OpenNOW.app") {
                return Err("DMG must contain exactly one OpenNOW.app bundle".to_owned());
            }
            std::fs::create_dir(destination).map_err(|error| error.to_string())?;
            copy_tree(
                &apps[0],
                &destination.join("OpenNOW.app"),
                &mut CopyBudget::new(),
                CopyPolicy::Bundle,
            )
        });
        let detached =
            run(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount)).or_else(|_| {
                run(Command::new("/usr/bin/hdiutil")
                    .args(["detach", "-force"])
                    .arg(&mount))
            });
        if detached.is_ok() {
            let _ = std::fs::remove_dir(&mount);
        }
        result.and(detached)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (package, destination);
        Err("DMG updates require macOS".to_owned())
    }
}

pub(super) fn copy_tree(
    source: &Path,
    destination: &Path,
    budget: &mut CopyBudget,
    policy: CopyPolicy,
) -> Result<(), String> {
    use std::fs;
    budget.consume_entry()?;
    let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
    if policy == CopyPolicy::UserData {
        super::security::create_private_directory(destination)?;
    } else {
        fs::create_dir(destination).map_err(|error| error.to_string())?;
    }
    let result = (|| {
        let mut pending = vec![(source.clone(), destination.to_path_buf())];
        let mut links = Vec::new();
        let mut directory_permissions = Vec::new();
        while let Some((from, to)) = pending.pop() {
            if policy == CopyPolicy::UserData {
                directory_permissions.push((from.clone(), to.clone()));
            }
            for entry in fs::read_dir(&from).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                budget.consume_entry()?;
                let metadata =
                    fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
                let path = to.join(entry.file_name());
                if metadata.is_symlink() {
                    let resolved =
                        fs::canonicalize(entry.path()).map_err(|error| error.to_string())?;
                    if !resolved.starts_with(&source) {
                        return Err("Application bundle symlink escapes its root".to_owned());
                    }
                    let target = fs::read_link(entry.path()).map_err(|error| error.to_string())?;
                    if target.is_absolute() {
                        return Err("Application bundle contains an absolute symlink".to_owned());
                    }
                    links.push((path, target));
                } else if metadata.is_dir() {
                    if policy == CopyPolicy::UserData {
                        super::security::create_private_directory(&path)?;
                    } else {
                        fs::create_dir(&path).map_err(|error| error.to_string())?;
                    }
                    pending.push((entry.path(), path));
                } else if metadata.is_file() {
                    budget.copy_file(&entry.path(), &path, policy)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if policy == CopyPolicy::Bundle {
                            fs::set_permissions(
                                &path,
                                fs::Permissions::from_mode(
                                    if metadata.permissions().mode() & 0o111 != 0 {
                                        0o755
                                    } else {
                                        0o644
                                    },
                                ),
                            )
                            .map_err(|error| error.to_string())?;
                        }
                    }
                } else {
                    return Err("Application bundle contains a special file".to_owned());
                }
            }
        }
        #[cfg(not(unix))]
        if !links.is_empty() {
            return Err("Bundle symlinks require macOS or Unix".to_owned());
        }
        #[cfg(unix)]
        for (path, target) in links {
            std::os::unix::fs::symlink(&target, &path).map_err(|error| error.to_string())?;
        }
        for (source, destination) in directory_permissions.into_iter().rev() {
            super::security::preserve_permissions(&source, &destination)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

pub(super) fn verify_bundle(bundle: &Path, installed_bundle: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        run(Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(bundle))?;
        let identity = |path: &Path| -> Result<String, String> {
            let output = super::command_output(
                Command::new("/usr/bin/codesign")
                    .args(["-dv", "--verbose=4"])
                    .arg(path),
                std::time::Duration::from_secs(120),
            )?;
            if !output.status.success() || output.stderr.len() > 64 * 1024 {
                return Err("Cannot read application signing identity".to_owned());
            }
            let value = String::from_utf8_lossy(&output.stderr);
            Ok(value
                .lines()
                .filter(|line| {
                    line.starts_with("TeamIdentifier=") || line.starts_with("Identifier=")
                })
                .collect::<Vec<_>>()
                .join("\n"))
        };
        let old = identity(installed_bundle)?;
        if old.is_empty() || old != identity(bundle)? {
            return Err(
                "Updated app bundle signing identity does not match the installation".to_owned(),
            );
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (bundle, installed_bundle);
        Err("macOS bundle verification requires macOS".to_owned())
    }
}
