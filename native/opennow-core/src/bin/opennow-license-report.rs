use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_LICENSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Package {
    name: String,
    version: String,
    license: String,
    source: String,
    directory: PathBuf,
    license_file: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("opennow-license-report: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let base = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage("missing base notice path"))?;
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage("missing output path"))?;
    let manifests = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if manifests.is_empty() {
        return Err(usage("at least one Cargo manifest is required"));
    }

    let mut packages = BTreeSet::new();
    for manifest in manifests {
        packages.extend(packages_for_manifest(&manifest)?);
    }

    let mut texts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut metadata_only = Vec::new();
    for package in packages {
        let label = format!(
            "{} {} — {} — {}",
            package.name, package.version, package.license, package.source
        );
        let license_files = find_license_files(&package)?;
        if license_files.is_empty() {
            metadata_only.push(label);
            continue;
        }
        for path in license_files {
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            texts.entry(contents).or_default().insert(label.clone());
        }
    }

    let mut notice = fs::read_to_string(&base)
        .map_err(|error| format!("could not read {}: {error}", base.display()))?;
    notice.push_str("\n\nGenerated Rust dependency notices\n=================================\n\n");
    notice.push_str(
        "This section is generated from the exact Cargo dependency graphs used by the OpenNOW application core and native streamer. Packages sharing identical license text are grouped together.\n",
    );
    for (index, (license_text, labels)) in texts.into_iter().enumerate() {
        notice.push_str(&format!(
            "\nLicense text group {}\n---------------------\n",
            index + 1
        ));
        for label in labels {
            notice.push_str("- ");
            notice.push_str(&label);
            notice.push('\n');
        }
        notice.push('\n');
        notice.push_str(license_text.trim());
        notice.push('\n');
    }
    if !metadata_only.is_empty() {
        notice.push_str("\nSPDX metadata-only packages\n---------------------------\n");
        notice.push_str("These packages declared an SPDX expression but shipped no standalone license file in the resolved source archive:\n");
        for label in metadata_only {
            notice.push_str("- ");
            notice.push_str(&label);
            notice.push('\n');
        }
    }

    let parent = output
        .parent()
        .ok_or_else(|| "output path has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let temporary = output.with_extension("tmp");
    fs::write(&temporary, notice)
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    if output.exists() {
        fs::remove_file(&output)
            .map_err(|error| format!("could not replace {}: {error}", output.display()))?;
    }
    fs::rename(&temporary, &output)
        .map_err(|error| format!("could not publish {}: {error}", output.display()))?;
    Ok(())
}

fn packages_for_manifest(manifest: &Path) -> Result<BTreeSet<Package>, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .output()
        .map_err(|error| format!("could not execute cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed for {}: {}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata returned invalid JSON: {error}"))?;
    let mut packages = BTreeSet::new();
    for value in metadata["packages"].as_array().into_iter().flatten() {
        let name = value["name"].as_str().unwrap_or_default();
        if name == "opennow-core" || name.starts_with("opennow-streamer") {
            continue;
        }
        let manifest_path = value["manifest_path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| format!("package {name} has no manifest path"))?;
        let directory = manifest_path
            .parent()
            .ok_or_else(|| format!("package {name} has an invalid manifest path"))?
            .to_path_buf();
        let source = value["repository"]
            .as_str()
            .or_else(|| value["homepage"].as_str())
            .or_else(|| value["source"].as_str())
            .unwrap_or("resolved local source")
            .to_owned();
        packages.insert(Package {
            name: name.to_owned(),
            version: value["version"].as_str().unwrap_or("unknown").to_owned(),
            license: value["license"]
                .as_str()
                .unwrap_or("NOASSERTION")
                .to_owned(),
            source,
            directory,
            license_file: value["license_file"].as_str().map(PathBuf::from),
        });
    }
    Ok(packages)
}

fn find_license_files(package: &Package) -> Result<Vec<PathBuf>, String> {
    if let Some(path) = package.license_file.as_ref() {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            package.directory.join(path)
        };
        validate_license_file(&path)?;
        return Ok(vec![path]);
    }

    let mut paths = fs::read_dir(&package.directory)
        .map_err(|error| format!("could not inspect {}: {error}", package.directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_ascii_lowercase)
                    .is_some_and(|name| {
                        name.starts_with("license")
                            || name.starts_with("copying")
                            || name.starts_with("notice")
                            || name.starts_with("copyright")
                    })
        })
        .collect::<Vec<_>>();
    paths.sort();
    for path in &paths {
        validate_license_file(path)?;
    }
    Ok(paths)
}

fn validate_license_file(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_LICENSE_BYTES {
        return Err(format!("unusable license file: {}", path.display()));
    }
    Ok(())
}

fn usage(message: &str) -> String {
    format!("{message}; usage: opennow-license-report BASE_NOTICE OUTPUT MANIFEST [MANIFEST ...]")
}
