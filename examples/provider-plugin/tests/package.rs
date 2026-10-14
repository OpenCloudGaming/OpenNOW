use opennow_plugin_api::provider::ProviderManifest;
use opennow_sdk_demo::{PLUGIN_ID, TARGET};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

#[test]
fn archive_contains_both_roles_and_exact_native_file_hashes() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("demo.opennow-plugin");
    let native = Path::new(env!("CARGO_BIN_EXE_opennow-sdk-demo"));
    opennow_sdk_demo::package::create(native, native, &output).unwrap();
    let mut archive = zip::ZipArchive::new(File::open(output).unwrap()).unwrap();
    assert_eq!(archive.len(), 5);
    let manifest: ProviderManifest =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    assert_eq!(manifest.id.as_str(), PLUGIN_ID);
    let roles = &manifest.entrypoints[TARGET];
    assert_ne!(roles.control, roles.media);
    for file in manifest.files {
        let mut bytes = Vec::new();
        archive
            .by_name(&file.path)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(file.sha256, format!("{:x}", Sha256::digest(&bytes)));
        if file.path.starts_with("bin/") {
            assert_eq!(bytes, fs::read(native).unwrap());
        }
    }
}

#[test]
fn package_generator_rejects_scripts_instead_of_executing_them() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("worker");
    fs::write(&script, b"#!/bin/sh\necho unsafe\n").unwrap();
    let output = directory.path().join("invalid.opennow-plugin");
    assert!(
        opennow_sdk_demo::package::create(
            Path::new(env!("CARGO_BIN_EXE_opennow-sdk-demo")),
            &script,
            &output
        )
        .is_err()
    );
    assert!(!output.exists());
}
