use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_ENTRIES: usize = 100_000;
const MAX_EXPANDED_BYTES: u64 = 12 * 1024 * 1024 * 1024;

pub(super) fn extract_zip(
    package: &Path,
    destination: &Path,
    allow_links: bool,
) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| error.to_string())?;
    let result = extract(package, destination, allow_links);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract(package: &Path, destination: &Path, allow_links: bool) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(File::open(package).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid update ZIP: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err("Update archive has too many entries".to_owned());
    }
    let mut paths = HashSet::new();
    let mut expanded = 0u64;
    let mut links = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = archive_path(entry.name())?;
        if !paths.insert(relative.to_string_lossy().to_lowercase()) {
            return Err("Update archive contains duplicate paths".to_owned());
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or("Update archive size overflow")?;
        if expanded > MAX_EXPANDED_BYTES || entry.size() > super::verification::MAXIMUM_UPDATE_BYTES
        {
            return Err("Expanded update exceeds its size limit".to_owned());
        }
        let mode = entry.unix_mode().unwrap_or(0o100644);
        let file_type = mode & 0o170000;
        let output = destination.join(&relative);
        if file_type == 0o120000 {
            if !allow_links || entry.size() > 4096 {
                return Err("Unsupported update archive symlink".to_owned());
            }
            let mut target = String::new();
            entry
                .by_ref()
                .take(4097)
                .read_to_string(&mut target)
                .map_err(|error| error.to_string())?;
            validate_link(&relative, &target)?;
            links.push((output, PathBuf::from(target)));
            continue;
        }
        if !matches!(file_type, 0 | 0o040000 | 0o100000) {
            return Err("Update archive contains a special file".to_owned());
        }
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            continue;
        }
        fs::create_dir_all(output.parent().ok_or("Invalid archive path")?)
            .map_err(|error| error.to_string())?;
        let expected = entry.size();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| error.to_string())?;
        let copied = std::io::copy(&mut entry.by_ref().take(expected + 1), &mut file)
            .map_err(|error| error.to_string())?;
        if copied != expected {
            return Err("Update ZIP entry has an invalid expanded size".to_owned());
        }
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &output,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )
            .map_err(|error| error.to_string())?;
        }
    }
    #[cfg(not(unix))]
    if !links.is_empty() {
        return Err("Update symlinks are unsupported on this platform".to_owned());
    }
    #[cfg(unix)]
    for (path, target) in &links {
        fs::create_dir_all(path.parent().ok_or("Invalid symlink path")?)
            .map_err(|error| error.to_string())?;
        std::os::unix::fs::symlink(target, path).map_err(|error| error.to_string())?;
    }
    let root = fs::canonicalize(destination).map_err(|error| error.to_string())?;
    for (path, _) in links {
        if !fs::canonicalize(path)
            .map_err(|error| format!("Invalid update symlink: {error}"))?
            .starts_with(&root)
        {
            return Err("Update symlink escapes the package".to_owned());
        }
    }
    Ok(())
}

pub(super) fn archive_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains(['\\', ':', '\0']) || value.starts_with('/') {
        return Err("Unsafe update archive path".to_owned());
    }
    let value = value.trim_end_matches('/');
    for part in value.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err("Unsafe update archive component".to_owned());
        }
    }
    let path = PathBuf::from(value);
    if !path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("Unsafe update archive path".to_owned());
    }
    Ok(path)
}

fn validate_link(path: &Path, target: &str) -> Result<(), String> {
    if target.is_empty() || target.contains(['\\', ':', '\0']) || Path::new(target).is_absolute() {
        return Err("Unsafe update symlink target".to_owned());
    }
    let mut depth = path
        .parent()
        .map(|parent| parent.components().count())
        .unwrap_or(0);
    for part in Path::new(target).components() {
        match part {
            Component::ParentDir => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("Update symlink escapes the package")?;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => (),
            _ => return Err("Unsafe update symlink target".to_owned()),
        }
    }
    Ok(())
}
