use opennow_plugin_api::{PluginManifest, validate_package_path};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

const ARCHIVE_LIMIT: u64 = 64 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 128 * 1024 * 1024;
const FILE_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;
const MEMBER_LIMIT: usize = 128;
const MANIFEST: &str = "manifest.json";

#[derive(Debug)]
pub(super) struct Error {
    pub code: &'static str,
    pub message: &'static str,
}

fn invalid(message: &'static str) -> Error {
    Error {
        code: "invalid_plugin_package",
        message,
    }
}

fn io_error(_: std::io::Error) -> Error {
    Error {
        code: "plugin_storage_error",
        message: "The plugin package could not be read or stored",
    }
}

#[derive(Debug)]
pub(super) struct StagedPackage {
    pub manifest: PluginManifest,
    pub package_sha256: String,
    pub root: PathBuf,
}

pub(super) fn current_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") if cfg!(target_env = "gnu") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") if cfg!(target_env = "gnu") => "aarch64-unknown-linux-gnu",
        ("x86_64", "linux") if cfg!(target_env = "musl") => "x86_64-unknown-linux-musl",
        ("aarch64", "linux") if cfg!(target_env = "musl") => "aarch64-unknown-linux-musl",
        ("x86_64", "windows") if cfg!(target_env = "msvc") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") if cfg!(target_env = "msvc") => "aarch64-pc-windows-msvc",
        ("x86_64", "windows") if cfg!(target_env = "gnu") => "x86_64-pc-windows-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        _ => "unsupported",
    }
}

pub(super) fn inspect(source: &Path, staging_dir: &Path) -> Result<StagedPackage, Error> {
    let root_metadata = fs::symlink_metadata(staging_dir).map_err(io_error)?;
    if !root_metadata.is_dir() || is_link(&root_metadata) {
        return Err(invalid("The staging directory must be a private directory"));
    }
    if fs::read_dir(staging_dir)
        .map_err(io_error)?
        .next()
        .is_some()
    {
        return Err(invalid("The staging directory must be empty"));
    }
    set_mode(staging_dir, 0o700)?;
    let snapshot = read_bounded(open_regular(source)?, ARCHIVE_LIMIT)?;
    let package_sha256 = format!("{:x}", Sha256::digest(&snapshot));
    let names = central_directory_names(&snapshot)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(snapshot.as_slice()))
        .map_err(|_| invalid("The plugin package is not a supported ZIP archive"))?;
    if archive.len() != names.len() {
        return Err(invalid("The archive contains duplicate members"));
    }
    let mut manifest_bytes = None;
    for index in 0..archive.len() {
        let mut member = archive
            .by_index(index)
            .map_err(|_| invalid("The archive member is invalid"))?;
        if !member.is_file()
            || member
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 != 0o100000)
            || member.name().as_bytes() != member.name_raw()
            || !names.contains(member.name())
        {
            return Err(invalid(
                "The archive must contain only regular files with exact UTF-8 paths",
            ));
        }
        let limit = if member.name() == MANIFEST {
            MANIFEST_LIMIT
        } else {
            FILE_LIMIT
        };
        if member.size() > limit {
            return Err(invalid("The archive member exceeds its size limit"));
        }
        if member.name() == MANIFEST {
            manifest_bytes = Some(read_bounded(&mut member, MANIFEST_LIMIT)?);
        }
    }
    let manifest_bytes =
        manifest_bytes.ok_or_else(|| invalid("The package manifest is missing"))?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| invalid("The package manifest is invalid or incompatible"))?;
    let (expected_files, _) = inventory(&manifest)?;
    if names != expected_files {
        return Err(invalid("The archive does not match the manifest inventory"));
    }
    let mut expanded = manifest_bytes.len() as u64;
    let mut manifest_file = create_private(&staging_dir.join(MANIFEST))?;
    manifest_file.write_all(&manifest_bytes).map_err(io_error)?;
    manifest_file.sync_all().map_err(io_error)?;
    for payload in &manifest.files {
        let destination = staging_dir.join(&payload.path);
        create_parents(staging_dir, &payload.path)?;
        let mut output = create_private(&destination)?;
        let member = archive
            .by_name(&payload.path)
            .map_err(|_| invalid("The payload is missing"))?;
        let digest = copy_hash(member, &mut output, FILE_LIMIT, &mut expanded)?;
        if digest != payload.sha256 {
            return Err(invalid("The payload checksum does not match the manifest"));
        }
        output.sync_all().map_err(io_error)?;
    }
    let executable = staging_dir.join(&manifest.entrypoints[current_target()]);
    validate_executable(&executable)?;
    set_mode(&executable, 0o755)?;
    Ok(StagedPackage {
        manifest,
        package_sha256,
        root: staging_dir.to_path_buf(),
    })
}

pub(super) fn verify(root: &Path, expected: &PluginManifest) -> Result<PathBuf, Error> {
    let (files, directories) = inventory(expected)?;
    let root_metadata = fs::symlink_metadata(root).map_err(io_error)?;
    if !root_metadata.is_dir() || is_link(&root_metadata) {
        return Err(invalid(
            "The installed package root is not a regular directory",
        ));
    }
    let mut pending = vec![PathBuf::new()];
    let mut found = BTreeSet::new();
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(root.join(&relative)).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("The installed package has an invalid path"))?;
            validate_path(&name)?;
            let path = relative.join(&name);
            let portable = path
                .components()
                .map(|part| part.as_os_str().to_str().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("/");
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if is_link(&metadata) {
                return Err(invalid("The installed package contains a link"));
            }
            if metadata.is_dir() && directories.contains(&portable) {
                pending.push(path);
            } else if metadata.is_file() && files.contains(&portable) {
                found.insert(portable);
            } else {
                return Err(invalid(
                    "The installed package contains an unexpected entry",
                ));
            }
        }
    }
    if found != files {
        return Err(invalid("The installed package is missing files"));
    }
    let bytes = read_bounded(open_regular(&root.join(MANIFEST))?, MANIFEST_LIMIT)?;
    let actual: PluginManifest =
        serde_json::from_slice(&bytes).map_err(|_| invalid("The installed manifest is invalid"))?;
    if &actual != expected {
        return Err(invalid("The installed manifest has changed"));
    }
    let mut expanded = bytes.len() as u64;
    for payload in &expected.files {
        let digest = copy_hash(
            open_regular(&root.join(&payload.path))?,
            std::io::sink(),
            FILE_LIMIT,
            &mut expanded,
        )?;
        if digest != payload.sha256 {
            return Err(invalid("The installed payload checksum has changed"));
        }
    }
    let executable = root.join(&expected.entrypoints[current_target()]);
    validate_executable(&executable)?;
    executable.canonicalize().map_err(io_error)
}

fn inventory(manifest: &PluginManifest) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
    manifest
        .validate()
        .map_err(|_| invalid("The package manifest is invalid or incompatible"))?;
    if current_target() == "unsupported" || !manifest.entrypoints.contains_key(current_target()) {
        return Err(Error {
            code: "incompatible_plugin",
            message: "The package does not support this host target",
        });
    }
    let mut files = BTreeSet::from([MANIFEST.to_owned()]);
    let mut paths = BTreeMap::new();
    register_path(MANIFEST, &mut paths)?;
    for payload in &manifest.files {
        register_path(&payload.path, &mut paths)?;
        files.insert(payload.path.clone());
    }
    let directories = paths
        .into_values()
        .filter_map(|(path, file)| (!file).then_some(path))
        .collect();
    Ok((files, directories))
}

fn validate_path(path: &str) -> Result<(), Error> {
    validate_package_path(path).map_err(|_| invalid("The package contains an unsafe path"))
}

fn register_path(path: &str, paths: &mut BTreeMap<String, (String, bool)>) -> Result<(), Error> {
    validate_path(path)?;
    let parts: Vec<_> = path.split('/').collect();
    for end in 1..=parts.len() {
        let prefix = parts[..end].join("/");
        let file = end == parts.len();
        let key = prefix.to_lowercase();
        if let Some((existing, existing_file)) = paths.get(&key) {
            if existing != &prefix || *existing_file || file {
                return Err(invalid(
                    "The package contains duplicate or conflicting paths",
                ));
            }
        } else {
            paths.insert(key, (prefix, file));
        }
    }
    Ok(())
}

fn central_directory_names(bytes: &[u8]) -> Result<BTreeSet<String>, Error> {
    let end = (0..bytes.len().saturating_sub(21))
        .rev()
        .take(65536)
        .find(|&offset| {
            bytes.get(offset..offset + 4) == Some(b"PK\x05\x06")
                && offset
                    + 22
                    + u16::from_le_bytes([bytes[offset + 20], bytes[offset + 21]]) as usize
                    == bytes.len()
        })
        .ok_or_else(|| invalid("The archive directory is invalid"))?;
    let read16 = |offset| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
    let read32 =
        |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let count = read16(end + 10);
    let mut offset = read32(end + 16);
    if read16(end + 4) != 0
        || read16(end + 6) != 0
        || read16(end + 8) != count
        || count == 0
        || count > MEMBER_LIMIT
        || offset.checked_add(read32(end + 12)) != Some(end)
    {
        return Err(invalid(
            "The archive is oversized or uses unsupported ZIP metadata",
        ));
    }
    let mut names = BTreeSet::new();
    let mut paths = BTreeMap::new();
    for _ in 0..count {
        if offset.checked_add(46).is_none_or(|next| next > end)
            || bytes.get(offset..offset + 4) != Some(b"PK\x01\x02")
        {
            return Err(invalid("The archive directory is malformed"));
        }
        let name_end = offset + 46 + read16(offset + 28);
        let next = name_end + read16(offset + 30) + read16(offset + 32);
        if next > end || read16(offset + 34) != 0 {
            return Err(invalid("The archive directory is malformed"));
        }
        let name = std::str::from_utf8(&bytes[offset + 46..name_end])
            .map_err(|_| invalid("The archive path is not UTF-8"))?;
        register_path(name, &mut paths)?;
        names.insert(name.to_owned());
        offset = next;
    }
    if offset != end {
        return Err(invalid("The archive directory has unexpected data"));
    }
    Ok(names)
}

fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("The package exceeds its size limit"));
    }
    Ok(bytes)
}

fn copy_hash(
    mut reader: impl Read,
    mut writer: impl Write,
    limit: u64,
    expanded: &mut u64,
) -> Result<String, Error> {
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let remaining = (limit - size).min(EXPANDED_LIMIT - *expanded);
        let capacity = buffer.len().min((remaining + 1) as usize);
        let count = reader.read(&mut buffer[..capacity]).map_err(io_error)?;
        if count == 0 {
            break;
        }
        if count as u64 > remaining {
            return Err(invalid("The expanded package exceeds its size limit"));
        }
        size += count as u64;
        *expanded += count as u64;
        hash.update(&buffer[..count]);
        writer.write_all(&buffer[..count]).map_err(io_error)?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn open_regular(path: &Path) -> Result<File, Error> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(invalid(
            "The package must contain regular files, not links or devices",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(invalid("The package file changed during inspection"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("The package contains a hard link"));
        }
    }
    Ok(file)
}

fn create_private(path: &Path) -> Result<File, Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(io_error)
}

fn set_mode(path: &Path, mode: u32) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(io_error)?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn create_parents(root: &Path, relative: &str) -> Result<(), Error> {
    let mut parent = root.to_path_buf();
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            break;
        }
        parent.push(part);
        let created = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().mode(0o700).create(&parent)
            }
            #[cfg(not(unix))]
            {
                fs::create_dir(&parent)
            }
        };
        match created {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&parent).map_err(io_error)?;
                if !metadata.is_dir() || is_link(&metadata) {
                    return Err(invalid("The payload parent is not a directory"));
                }
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn validate_executable(path: &Path) -> Result<(), Error> {
    let mut file = open_regular(path)?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| invalid("The entrypoint is not a native executable"))?;
    let valid = match std::env::consts::OS {
        "linux" => {
            let machine = if std::env::consts::ARCH == "x86_64" {
                62
            } else {
                183
            };
            header[..7] == *b"\x7fELF\x02\x01\x01"
                && matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
                && u16::from_le_bytes([header[18], header[19]]) == machine
        }
        "windows" => {
            use std::io::{Seek, SeekFrom};
            let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
            let mut pe = [0u8; 24];
            let machine = if std::env::consts::ARCH == "x86_64" {
                0x8664
            } else {
                0xaa64
            };
            header[..2] == *b"MZ"
                && (64..=FILE_LIMIT - 24).contains(&offset)
                && file.seek(SeekFrom::Start(offset)).is_ok()
                && file.read_exact(&mut pe).is_ok()
                && pe[..4] == *b"PE\0\0"
                && u16::from_le_bytes([pe[4], pe[5]]) == machine
                && u16::from_le_bytes([pe[22], pe[23]]) & 0x2002 == 0x0002
        }
        "macos" => {
            let cpu = if std::env::consts::ARCH == "x86_64" {
                0x01000007
            } else {
                0x0100000c
            };
            header[..4] == [0xcf, 0xfa, 0xed, 0xfe]
                && u32::from_le_bytes(header[4..8].try_into().unwrap()) == cpu
                && u32::from_le_bytes(header[12..16].try_into().unwrap()) == 2
        }
        _ => false,
    };
    if !valid {
        return Err(invalid(
            "The entrypoint is not a native executable for this host",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use zip::write::SimpleFileOptions;

    fn native_binary() -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        match std::env::consts::OS {
            "linux" => {
                bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
                bytes[16] = 2;
                bytes[18..20].copy_from_slice(
                    &(if std::env::consts::ARCH == "x86_64" {
                        62u16
                    } else {
                        183
                    })
                    .to_le_bytes(),
                );
            }
            "windows" => {
                bytes[..2].copy_from_slice(b"MZ");
                bytes[60] = 64;
                bytes[64..68].copy_from_slice(b"PE\0\0");
                bytes[68..70].copy_from_slice(
                    &(if std::env::consts::ARCH == "x86_64" {
                        0x8664u16
                    } else {
                        0xaa64
                    })
                    .to_le_bytes(),
                );
                bytes[86] = 2;
            }
            "macos" => {
                bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
                bytes[4..8].copy_from_slice(
                    &(if std::env::consts::ARCH == "x86_64" {
                        0x01000007u32
                    } else {
                        0x0100000c
                    })
                    .to_le_bytes(),
                );
                bytes[12] = 2;
            }
            _ => panic!("unsupported test host"),
        }
        bytes
    }

    fn manifest_for(payloads: &[(&str, Vec<u8>)]) -> Value {
        json!({
            "schemaVersion": 1,
            "id": "org.example.catalog",
            "name": "Example catalog",
            "version": "1.0.0",
            "publisher": "Example author",
            "description": "Local package fixture",
            "protocolVersion": 1,
            "capabilities": ["catalog.v1"],
            "entrypoints": {current_target(): "bin/catalog"},
            "files": payloads.iter().map(|(path, bytes)| json!({"path":path,"sha256":format!("{:x}",Sha256::digest(bytes))})).collect::<Vec<_>>()
        })
    }

    fn archive_bytes(manifest: &Value, payloads: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);
        writer.start_file(MANIFEST, options).unwrap();
        writer
            .write_all(&serde_json::to_vec(manifest).unwrap())
            .unwrap();
        for (path, bytes) in payloads {
            writer.start_file(*path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn inspect_bytes(bytes: &[u8]) -> (tempfile::TempDir, Result<StagedPackage, Error>) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.opennow-plugin");
        let staging = directory.path().join("stage");
        fs::write(&source, bytes).unwrap();
        fs::create_dir(&staging).unwrap();
        let result = inspect(&source, &staging);
        (directory, result)
    }

    fn valid_archive() -> Vec<u8> {
        let payloads = vec![
            ("bin/catalog", native_binary()),
            ("data/catalog.json", b"[]".to_vec()),
        ];
        archive_bytes(&manifest_for(&payloads), &payloads)
    }

    #[test]
    fn inspection_binds_snapshot_and_verifies_exact_installed_tree() {
        let bytes = valid_archive();
        let (directory, staged) = inspect_bytes(&bytes);
        let staged = staged.unwrap();
        assert_eq!(
            staged.package_sha256,
            format!("{:x}", Sha256::digest(&bytes))
        );
        fs::write(directory.path().join("source.opennow-plugin"), b"replaced").unwrap();
        assert_eq!(
            verify(&staged.root, &staged.manifest).unwrap(),
            staged.root.join("bin/catalog").canonicalize().unwrap()
        );
        assert_eq!(fs::read_dir(&staged.root).unwrap().count(), 3);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for (path, mode) in [
                ("", 0o700),
                ("bin", 0o700),
                (MANIFEST, 0o600),
                ("bin/catalog", 0o755),
                ("data/catalog.json", 0o600),
            ] {
                assert_eq!(
                    fs::metadata(staged.root.join(path))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    mode
                );
            }
        }
    }

    #[test]
    fn rejects_traversal_platform_aliases_and_absolute_paths() {
        for path in [
            "../escape",
            "/absolute",
            "//server/share",
            "C:/drive",
            "C:drive",
            "a\\b",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "a\0b",
            "a\nb",
            "NUL.txt",
            "aux",
            "bin/COM1",
            "a.",
            "a ",
            "bin/a:stream",
        ] {
            let payloads = vec![("bin/catalog", native_binary()), (path, vec![1])];
            let (_, result) = inspect_bytes(&archive_bytes(&manifest_for(&payloads), &payloads));
            assert!(result.is_err(), "accepted {path:?}");
        }
        assert!(validate_path(&"a".repeat(241)).is_err());
    }

    #[test]
    fn rejects_member_and_parent_case_collisions() {
        for path in [
            "BIN/catalog",
            "Bin/other",
            "MANIFEST.JSON",
            "bin",
            "manifest.json/other",
        ] {
            let payloads = vec![("bin/catalog", native_binary()), (path, vec![1])];
            let (_, result) = inspect_bytes(&archive_bytes(&manifest_for(&payloads), &payloads));
            assert!(result.is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn rejects_duplicate_members_even_when_zip_reader_collapses_them() {
        let payloads = vec![
            ("bin/catalog", native_binary()),
            ("bin/cataloz", native_binary()),
        ];
        let mut bytes = archive_bytes(&manifest_for(&payloads), &payloads);
        for offset in 0..bytes.len() - 11 {
            if &bytes[offset..offset + 11] == b"bin/cataloz" {
                bytes[offset + 10] = b'g';
            }
        }
        assert_eq!(zip::ZipArchive::new(Cursor::new(&bytes)).unwrap().len(), 2);
        let (_, result) = inspect_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_symlink_directory_and_device_member_modes() {
        for mode in [0o120777u32, 0o040700, 0o020600, 0o060600, 0o010600] {
            let mut bytes = valid_archive();
            let mut directory_headers = 0;
            for offset in 0..bytes.len() - 46 {
                if &bytes[offset..offset + 4] == b"PK\x01\x02" {
                    directory_headers += 1;
                    if directory_headers == 2 {
                        bytes[offset + 5] = 3;
                        bytes[offset + 38..offset + 42]
                            .copy_from_slice(&(mode << 16).to_le_bytes());
                        break;
                    }
                }
            }
            let (_, result) = inspect_bytes(&bytes);
            assert!(result.is_err(), "accepted mode {mode:o}");
        }
    }

    #[test]
    fn rejects_missing_unlisted_and_hash_mismatched_payloads() {
        let payloads = vec![("bin/catalog", native_binary()), ("data/extra", vec![1])];
        let mut manifest = manifest_for(&payloads);
        let (_, missing) = inspect_bytes(&archive_bytes(&manifest, &payloads[..1]));
        assert!(missing.is_err());
        manifest["files"].as_array_mut().unwrap().pop();
        let (_, unlisted) = inspect_bytes(&archive_bytes(&manifest, &payloads));
        assert!(unlisted.is_err());
        manifest["files"][0]["sha256"] = json!("0".repeat(64));
        let (_, mismatch) = inspect_bytes(&archive_bytes(&manifest, &payloads[..1]));
        assert!(mismatch.is_err());
    }

    #[test]
    fn rejects_malformed_manifest_unsupported_target_and_scripts() {
        let payloads = vec![("bin/catalog", native_binary())];
        let manifest = manifest_for(&payloads);
        for (key, value) in [
            ("schemaVersion", json!(2)),
            ("protocolVersion", json!(2)),
            ("engines", json!({})),
            ("builtIn", json!(true)),
            ("capabilities", json!(["session.v1"])),
            ("entrypoints", json!({"unrecognized-target": "bin/catalog"})),
            ("version", json!("not-semver")),
        ] {
            let mut invalid_manifest = manifest.clone();
            invalid_manifest[key] = value;
            let (_, result) = inspect_bytes(&archive_bytes(&invalid_manifest, &payloads));
            assert!(result.is_err(), "accepted {key}");
        }
        let scripts = vec![("bin/catalog", b"#!/bin/sh\necho plugin\n".to_vec())];
        let (_, result) = inspect_bytes(&archive_bytes(&manifest_for(&scripts), &scripts));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_malformed_hashes_and_duplicate_inventory() {
        let payloads = vec![("bin/catalog", native_binary())];
        for digest in [
            "A".repeat(64),
            "g".repeat(64),
            "0".repeat(63),
            "0".repeat(65),
        ] {
            let mut manifest = manifest_for(&payloads);
            manifest["files"][0]["sha256"] = json!(digest);
            assert!(
                inspect_bytes(&archive_bytes(&manifest, &payloads))
                    .1
                    .is_err()
            );
        }
        let mut manifest = manifest_for(&payloads);
        let repeated = manifest["files"][0].clone();
        manifest["files"].as_array_mut().unwrap().push(repeated);
        assert!(
            inspect_bytes(&archive_bytes(&manifest, &payloads))
                .1
                .is_err()
        );
    }

    #[test]
    fn rejects_corrupted_installed_payload_manifest_and_inventory() {
        for corruption in 0..5 {
            let (_directory, staged) = inspect_bytes(&valid_archive());
            let staged = staged.unwrap();
            match corruption {
                0 => fs::write(staged.root.join("bin/catalog"), b"changed").unwrap(),
                1 => fs::write(staged.root.join("extra"), b"extra").unwrap(),
                2 => fs::remove_file(staged.root.join("data/catalog.json")).unwrap(),
                3 => fs::create_dir(staged.root.join("empty-extra")).unwrap(),
                _ => {
                    let mut manifest = serde_json::to_value(&staged.manifest).unwrap();
                    manifest["publisher"] = json!("Changed publisher");
                    fs::write(
                        staged.root.join(MANIFEST),
                        serde_json::to_vec(&manifest).unwrap(),
                    )
                    .unwrap();
                }
            }
            assert!(
                verify(&staged.root, &staged.manifest).is_err(),
                "accepted corruption {corruption}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlink_source_and_installed_links() {
        use std::os::unix::fs::symlink;
        let (directory, staged) = inspect_bytes(&valid_archive());
        let staged = staged.unwrap();
        let source_link = directory.path().join("linked.opennow-plugin");
        symlink(directory.path().join("source.opennow-plugin"), &source_link).unwrap();
        let stage = directory.path().join("stage2");
        fs::create_dir(&stage).unwrap();
        assert!(inspect(&source_link, &stage).is_err());
        let payload = staged.root.join("data/catalog.json");
        fs::remove_file(&payload).unwrap();
        let external = directory.path().join("external");
        fs::write(&external, b"[]").unwrap();
        symlink(&external, &payload).unwrap();
        assert!(verify(&staged.root, &staged.manifest).is_err());
        fs::remove_file(&payload).unwrap();
        fs::hard_link(&external, &payload).unwrap();
        assert!(verify(&staged.root, &staged.manifest).is_err());
        fs::remove_file(&payload).unwrap();
        fs::remove_dir(staged.root.join("data")).unwrap();
        symlink(directory.path(), staged.root.join("data")).unwrap();
        assert!(verify(&staged.root, &staged.manifest).is_err());
    }

    #[test]
    fn reads_stop_at_hard_limits_without_trusting_declared_lengths() {
        struct Endless {
            read: usize,
        }
        impl Read for Endless {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes.fill(0);
                self.read += bytes.len();
                Ok(bytes.len())
            }
        }
        let mut reader = Endless { read: 0 };
        assert!(read_bounded(&mut reader, 1024).is_err());
        assert_eq!(reader.read, 1025);
        let mut reader = Endless { read: 0 };
        let mut expanded = 0;
        assert!(copy_hash(&mut reader, std::io::sink(), 1024, &mut expanded).is_err());
        assert_eq!(reader.read, 1025);
        let mut reader = Endless { read: 0 };
        let mut expanded = EXPANDED_LIMIT - 10;
        assert!(copy_hash(&mut reader, std::io::sink(), FILE_LIMIT, &mut expanded).is_err());
        assert_eq!(reader.read, 11);
    }

    #[test]
    fn rejects_archive_and_manifest_size_limits_and_nonempty_staging() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("large");
        File::create(&source)
            .unwrap()
            .set_len(ARCHIVE_LIMIT + 1)
            .unwrap();
        let stage = directory.path().join("stage");
        fs::create_dir(&stage).unwrap();
        assert!(inspect(&source, &stage).is_err());
        fs::write(stage.join("existing"), b"keep").unwrap();
        assert!(inspect(&source, &stage).is_err());
        assert_eq!(fs::read(stage.join("existing")).unwrap(), b"keep");
        let payloads = vec![("bin/catalog", native_binary())];
        let mut manifest = manifest_for(&payloads);
        manifest["description"] = json!("x".repeat(MANIFEST_LIMIT as usize));
        assert!(
            inspect_bytes(&archive_bytes(&manifest, &payloads))
                .1
                .is_err()
        );
    }

    #[test]
    fn rejects_excess_members_and_forged_expanded_lengths() {
        let names: Vec<_> = (0..MEMBER_LIMIT)
            .map(|index| format!("data/{index}"))
            .collect();
        let mut payloads = vec![("bin/catalog", native_binary())];
        payloads.extend(names.iter().map(|name| (name.as_str(), vec![1])));
        assert!(
            inspect_bytes(&archive_bytes(&manifest_for(&payloads), &payloads))
                .1
                .is_err()
        );
        let mut bytes = valid_archive();
        let mut headers = 0;
        for offset in 0..bytes.len() - 46 {
            if &bytes[offset..offset + 4] == b"PK\x01\x02" {
                headers += 1;
                if headers == 2 {
                    bytes[offset + 24..offset + 28]
                        .copy_from_slice(&((FILE_LIMIT + 1) as u32).to_le_bytes());
                    break;
                }
            }
        }
        assert!(inspect_bytes(&bytes).1.is_err());
        let payloads = vec![
            ("bin/catalog", native_binary()),
            ("data/large", vec![0; 1024]),
        ];
        let bytes = archive_bytes(&manifest_for(&payloads), &payloads);
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let member = archive.by_name("data/large").unwrap();
        assert!(copy_hash(member, std::io::sink(), 16, &mut 0).is_err());
    }

    #[test]
    fn rejects_other_native_architectures_and_other_valid_host_targets() {
        let mut binary = native_binary();
        let offset = match std::env::consts::OS {
            "linux" => 18,
            "windows" => 68,
            "macos" => 4,
            _ => unreachable!(),
        };
        binary[offset] ^= 0xff;
        let payloads = vec![("bin/catalog", binary)];
        assert!(
            inspect_bytes(&archive_bytes(&manifest_for(&payloads), &payloads))
                .1
                .is_err()
        );
        let payloads = vec![("bin/catalog", native_binary())];
        let mut manifest = manifest_for(&payloads);
        let other_target = if current_target() == "x86_64-unknown-linux-gnu" {
            "aarch64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        manifest["entrypoints"] = json!({other_target: "bin/catalog"});
        let failure = inspect_bytes(&archive_bytes(&manifest, &payloads))
            .1
            .unwrap_err();
        assert_eq!(failure.code, "incompatible_plugin");
    }
}
