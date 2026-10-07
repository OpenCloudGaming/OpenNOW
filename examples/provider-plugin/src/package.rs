use crate::{CAPABILITIES, PLUGIN_ID, TARGET, VERSION};
use opennow_plugin_api::provider::{AuthKind, List, ProviderManifest, RoleEntrypoints};
use opennow_plugin_api::{PackageFile, PluginId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

const FILE_LIMIT: u64 = 64 * 1024 * 1024;

pub fn create(control: &Path, worker: &Path, destination: &Path) -> io::Result<()> {
    let control = read_executable(control)?;
    let worker = read_executable(worker)?;
    let control_path = if cfg!(windows) {
        "bin/control.exe"
    } else {
        "bin/control"
    };
    let media_path = if cfg!(windows) {
        "bin/media.exe"
    } else {
        "bin/media"
    };
    let payloads = [
        (control_path, control),
        (media_path, worker),
        ("LICENSE.txt", include_bytes!("../../../LICENSE").to_vec()),
        (
            "MEDIA_NOTICES.txt",
            include_bytes!("../MEDIA_NOTICES.txt").to_vec(),
        ),
    ];
    let manifest=ProviderManifest{schema_version:2,protocol_version:2,id:PluginId::new(PLUGIN_ID).unwrap(),
        name:"OpenNOW SDK demo".into(),version:VERSION.into(),publisher:"OpenNOW SDK example authors".into(),
        description:"Playable local H.264/Opus fixture with deterministic demonstration pairing. Not Xbox or a cloud service.".into(),
        capabilities:List::new(CAPABILITIES.to_vec()).unwrap(),auth_kinds:List::new(vec![AuthKind::Pairing]).unwrap(),
        entrypoints:BTreeMap::from([(TARGET.to_owned(),RoleEntrypoints{control:control_path.into(),media:media_path.into()})]),
        files:payloads.iter().map(|(path,bytes)|PackageFile{path:(*path).into(),sha256:format!("{:x}",Sha256::digest(bytes))}).collect()};
    manifest
        .validate()
        .map_err(|_| io::Error::other("Invalid demo package inventory"))?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let mut archive = zip::ZipWriter::new(output.as_file_mut());
    archive.start_file(
        "manifest.json",
        zip::write::SimpleFileOptions::default().unix_permissions(0o600),
    )?;
    archive.write_all(
        &serde_json::to_vec_pretty(&manifest)
            .map_err(|_| io::Error::other("Cannot encode demo manifest"))?,
    )?;
    for (path, bytes) in payloads {
        archive.start_file(
            path,
            zip::write::SimpleFileOptions::default().unix_permissions(
                if path.starts_with("bin/") {
                    0o755
                } else {
                    0o600
                },
            ),
        )?;
        archive.write_all(&bytes)?;
    }
    let file = archive.finish()?;
    if file.metadata()?.len() > FILE_LIMIT {
        return Err(io::Error::other("Compressed demo package exceeds 64 MiB"));
    }
    file.sync_all()?;
    output.persist(destination).map_err(|error| error.error)?;
    Ok(())
}

fn read_executable(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || opennow_plugin_package::is_link(&metadata) {
        return Err(io::Error::other(
            "Expected a native build artifact, not a link or device",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > FILE_LIMIT {
        return Err(io::Error::other("Executable exceeds demo package bounds"));
    }
    let mut snapshot = tempfile::NamedTempFile::new()?;
    snapshot.write_all(&bytes)?;
    snapshot.flush()?;
    opennow_plugin_package::validate_executable(snapshot.path())
        .map_err(|_| io::Error::other("The captured executable does not match this host"))?;
    Ok(bytes)
}
