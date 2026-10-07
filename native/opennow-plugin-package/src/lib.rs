use opennow_plugin_api::provider::ProviderManifest;
use opennow_plugin_api::{PackageFile, PluginId, PluginManifest, validate_package_path};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const EXPANDED_LIMIT: u64 = 128 * 1024 * 1024;
pub const FILE_LIMIT: u64 = 64 * 1024 * 1024;
pub const MANIFEST_LIMIT: u64 = 64 * 1024;
pub const MANIFEST: &str = "manifest.json";

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: &'static str,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}
impl std::error::Error for Error {}

pub fn invalid(message: &'static str) -> Error {
    Error {
        code: "invalid_plugin_package",
        message,
    }
}

pub fn io_error(_: std::io::Error) -> Error {
    Error {
        code: "plugin_storage_error",
        message: "The plugin package could not be read or stored",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InstalledManifest {
    Catalog(PluginManifest),
    Provider(ProviderManifest),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Role {
    Catalog,
    Control,
    Media,
}

impl InstalledManifest {
    pub fn validate(&self) -> Result<(), opennow_plugin_api::ValidationError> {
        match self {
            Self::Catalog(manifest) => manifest.validate(),
            Self::Provider(manifest) => manifest.validate(),
        }
    }
    pub fn id(&self) -> &PluginId {
        match self {
            Self::Catalog(manifest) => &manifest.id,
            Self::Provider(manifest) => &manifest.id,
        }
    }
    pub fn files(&self) -> &[PackageFile] {
        match self {
            Self::Catalog(manifest) => &manifest.files,
            Self::Provider(manifest) => &manifest.files,
        }
    }
    pub fn entrypoint(&self, role: Role) -> Option<&str> {
        match self {
            Self::Catalog(manifest) if role != Role::Media => manifest
                .entrypoints
                .get(current_target())
                .map(String::as_str),
            Self::Provider(manifest) => {
                manifest
                    .entrypoints
                    .get(current_target())
                    .and_then(|roles| match role {
                        Role::Control => Some(roles.control.as_str()),
                        Role::Media => Some(roles.media.as_str()),
                        Role::Catalog => None,
                    })
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedPackage {
    root: PathBuf,
    manifest: InstalledManifest,
    entrypoints: BTreeMap<Role, PathBuf>,
    pin: Arc<PackagePin>,
}

impl VerifiedPackage {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest(&self) -> &InstalledManifest {
        &self.manifest
    }
    pub fn entrypoint(&self, role: Role) -> Option<&Path> {
        self.entrypoints.get(&role).map(PathBuf::as_path)
    }
    pub fn pin(&self) -> &Arc<PackagePin> {
        &self.pin
    }
}

#[derive(Debug)]
pub struct PackagePin {
    file: File,
}

impl PackagePin {
    pub fn shared(root: &Path) -> Result<Self, Error> {
        Self::open(root, false)
    }
    pub fn try_exclusive(root: &Path) -> Result<Self, Error> {
        Self::open(root, true)
    }

    fn open(root: &Path, exclusive: bool) -> Result<Self, Error> {
        let parent = root
            .parent()
            .ok_or_else(|| invalid("The package root has no parent"))?
            .canonicalize()
            .map_err(io_error)?;
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("The package root name is invalid"))?;
        validate_path(name)?;
        let lock_directory = parent
            .parent()
            .unwrap_or(&parent)
            .join(".opennow-package-pins");
        let directory = fs::DirBuilder::new();
        #[cfg(unix)]
        let directory = {
            use std::os::unix::fs::DirBuilderExt;
            let mut directory = directory;
            directory.mode(0o700);
            directory
        };
        match directory.create(&lock_directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(error)),
        }
        let metadata = fs::symlink_metadata(&lock_directory).map_err(io_error)?;
        if !metadata.is_dir() || is_link(&metadata) {
            return Err(invalid("The package pin namespace is not a directory"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(invalid("The package pin namespace has another owner"));
            }
        }
        let namespace = format!(
            "{:x}",
            Sha256::digest(parent.as_os_str().as_encoded_bytes())
        );
        let path = lock_directory.join(format!("{namespace}.{name}.pin"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        let file = options.open(&path).map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() || is_link(&metadata) {
            return Err(invalid("The package pin is not a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 {
                return Err(invalid("The package pin is a hard link"));
            }
        }
        let result = if exclusive {
            fs2::FileExt::try_lock_exclusive(&file)
        } else {
            fs2::FileExt::try_lock_shared(&file)
        };
        result.map_err(|error| {
            if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
                Error {
                    code: "plugin_in_use",
                    message: "The installed package version is in use",
                }
            } else {
                io_error(error)
            }
        })?;
        Ok(Self { file })
    }
}

impl Drop for PackagePin {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub fn current_target() -> &'static str {
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

pub fn verify(root: &Path, expected: &InstalledManifest) -> Result<VerifiedPackage, Error> {
    let pin = PackagePin::shared(root)?;
    let entrypoints = verify_files(root, expected)?;
    Ok(VerifiedPackage {
        root: root.canonicalize().map_err(io_error)?,
        manifest: expected.clone(),
        entrypoints,
        pin: Arc::new(pin),
    })
}

pub fn validate_staged(root: &Path, expected: &InstalledManifest) -> Result<(), Error> {
    verify_files(root, expected).map(|_| ())
}

fn verify_files(
    root: &Path,
    expected: &InstalledManifest,
) -> Result<BTreeMap<Role, PathBuf>, Error> {
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
    let actual: InstalledManifest =
        serde_json::from_slice(&bytes).map_err(|_| invalid("The installed manifest is invalid"))?;
    if &actual != expected {
        return Err(invalid("The installed manifest has changed"));
    }
    let mut expanded = bytes.len() as u64;
    for payload in expected.files() {
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
    let mut entrypoints = BTreeMap::new();
    for role in [Role::Catalog, Role::Control, Role::Media] {
        if let Some(relative) = expected.entrypoint(role) {
            let executable = root.join(relative);
            validate_executable(&executable)?;
            entrypoints.insert(role, executable.canonicalize().map_err(io_error)?);
        }
    }
    Ok(entrypoints)
}

pub fn inventory(
    manifest: &InstalledManifest,
) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
    manifest
        .validate()
        .map_err(|_| invalid("The package manifest is invalid or incompatible"))?;
    if current_target() == "unsupported" || manifest.entrypoint(Role::Control).is_none() {
        return Err(Error {
            code: "incompatible_plugin",
            message: "The package does not support this host target",
        });
    }
    let mut files = BTreeSet::from([MANIFEST.to_owned()]);
    let mut paths = BTreeMap::new();
    register_path(MANIFEST, &mut paths)?;
    for payload in manifest.files() {
        register_path(&payload.path, &mut paths)?;
        files.insert(payload.path.clone());
    }
    let directories = paths
        .into_values()
        .filter_map(|(path, file)| (!file).then_some(path))
        .collect();
    Ok((files, directories))
}

pub fn validate_path(path: &str) -> Result<(), Error> {
    validate_package_path(path).map_err(|_| invalid("The package contains an unsafe path"))
}

pub fn register_path(
    path: &str,
    paths: &mut BTreeMap<String, (String, bool)>,
) -> Result<(), Error> {
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

pub fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, Error> {
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

pub fn copy_hash(
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

pub fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

pub fn open_regular(path: &Path) -> Result<File, Error> {
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

pub fn validate_executable(path: &Path) -> Result<(), Error> {
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
