use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;
const REQUIRED_ATTESTATIONS: &[&str] = &[
    "deviceLogin",
    "accountSwitching",
    "subscriptionRefresh",
    "regionRefresh",
    "guideVideoOrdering",
    "keyboardInput",
    "relativeMouseInput",
    "controllerInput",
    "controllerNeutralState",
    "windowResize",
    "fullscreen",
    "displayMigration",
    "highestRefreshRate",
    "screenshotVisible",
    "recordingPlayback",
    "thumbnailGenerated",
    "mediaReveal",
    "networkRecovery",
    "streamerRecovery",
    "terminalErrorUsable",
    "installUpgradeUninstall",
    "desktopAndProtocolAssociations",
];

#[derive(Debug)]
struct Arguments {
    live: PathBuf,
    performance_1080p: PathBuf,
    performance_4k: PathBuf,
    attestations: PathBuf,
    packages: Vec<PathBuf>,
    output: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatrixOs {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Copy, Debug)]
struct MatrixRow {
    os: MatrixOs,
    architecture: &'static str,
    window_system: &'static [&'static str],
}

fn main() -> ExitCode {
    let raw_arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if raw_arguments
        .iter()
        .any(|value| value == "--help" || value == "-h")
    {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }
    let arguments = match parse_arguments(raw_arguments.into_iter()) {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}\n\n{}", usage());
            return ExitCode::from(2);
        }
    };
    match verify(&arguments) {
        Ok(passed) => ExitCode::from(if passed { 0 } else { 1 }),
        Err(message) => {
            eprintln!("Acceptance verification could not run: {message}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> &'static str {
    concat!(
        "Usage: opennow-acceptance-verify \\\n",
        "  --live <live.json> --performance-1080p <1080p.json> \\\n",
        "  --performance-4k <4k.json> --attestations <attestations.json> \\\n",
        "  --package <artifact> [--package <artifact> ...] --output <result.json>"
    )
}

fn parse_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Arguments, String> {
    let mut values: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut arguments = arguments.peekable();
    while let Some(flag) = arguments.next() {
        let flag = flag
            .into_string()
            .map_err(|_| "Arguments must be valid Unicode".to_owned())?;
        if !matches!(
            flag.as_str(),
            "--live"
                | "--performance-1080p"
                | "--performance-4k"
                | "--attestations"
                | "--package"
                | "--output"
        ) {
            return Err(format!("Unknown argument: {flag}"));
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        values.entry(flag).or_default().push(PathBuf::from(value));
    }
    let one = |flag: &str| -> Result<PathBuf, String> {
        let found = values.get(flag).cloned().unwrap_or_default();
        if found.len() != 1 {
            return Err(format!("{flag} must be supplied exactly once"));
        }
        Ok(found[0].clone())
    };
    let packages = values.get("--package").cloned().unwrap_or_default();
    if packages.is_empty() {
        return Err("At least one --package is required".to_owned());
    }
    Ok(Arguments {
        live: one("--live")?,
        performance_1080p: one("--performance-1080p")?,
        performance_4k: one("--performance-4k")?,
        attestations: one("--attestations")?,
        packages,
        output: one("--output")?,
    })
}

fn verify(arguments: &Arguments) -> Result<bool, String> {
    if !arguments.output.is_absolute() {
        return Err("--output must be an absolute path".to_owned());
    }
    let live = read_json(&arguments.live)?;
    let performance_1080p = read_json(&arguments.performance_1080p)?;
    let performance_4k = read_json(&arguments.performance_4k)?;
    let attestations = read_json(&arguments.attestations)?;

    let mut failures = Vec::new();
    validate_live(&live, &mut failures);
    validate_performance(&performance_1080p, 1920, 1080, &mut failures);
    validate_performance(&performance_4k, 3840, 2160, &mut failures);

    let matrix_name = attestations["matrixRow"].as_str().unwrap_or_default();
    let matrix = matrix_row(matrix_name);
    if matrix.is_none() {
        failures.push("unsupported matrixRow".to_owned());
    }
    validate_attestations(&attestations, matrix, &mut failures);
    validate_cross_report(
        &live,
        &performance_1080p,
        &performance_4k,
        &attestations,
        matrix,
        &mut failures,
    );

    let package_evidence = hash_packages(&arguments.packages)?;
    validate_packages(&attestations, matrix, &package_evidence, &mut failures);
    failures.sort();
    failures.dedup();
    let passed = failures.is_empty();
    let report = json!({
        "schemaVersion": 1,
        "kind": "opennow.acceptance-verification",
        "pass": passed,
        "matrixRow": matrix.map(|_| matrix_name),
        "releaseCandidate": safe_output_label(&attestations["releaseCandidate"]),
        "applicationVersion": safe_output_label(&live["applicationVersion"]),
        "inputs": {
            "live": file_evidence(&arguments.live)?,
            "performance1080p": file_evidence(&arguments.performance_1080p)?,
            "performance4k": file_evidence(&arguments.performance_4k)?,
            "attestations": file_evidence(&arguments.attestations)?,
            "packages": package_evidence.values().cloned().collect::<Vec<_>>()
        },
        "failures": failures,
        "policy": "all-machine-observations-performance-reports-manual-attestations-packages"
    });
    write_new_json(&arguments.output, &report)?;
    println!(
        "Acceptance verification {}: {}",
        if passed { "passed" } else { "failed" },
        arguments.output.display()
    );
    Ok(passed)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "JSON input must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_JSON_BYTES {
        return Err(format!("JSON input size is invalid: {}", path.display()));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))
}

fn validate_live(value: &Value, failures: &mut Vec<String>) {
    require(
        value["schemaVersion"] == 1,
        "live schemaVersion must be 1",
        failures,
    );
    require(
        value["kind"] == "opennow.live-acceptance",
        "live kind is invalid",
        failures,
    );
    require(
        value["observedPass"] == true,
        "live observedPass is false",
        failures,
    );
    require(
        value["scope"] == "machine-observed-live-runtime",
        "live scope is invalid",
        failures,
    );
    require(
        safe_label(value["applicationVersion"].as_str().unwrap_or_default()),
        "live applicationVersion is missing or invalid",
        failures,
    );
    require(
        !contains_sensitive(value),
        "live evidence contains a sensitive key or value",
        failures,
    );
    let checks = value["checks"].as_object();
    require(checks.is_some(), "live checks object is missing", failures);
    if let Some(checks) = checks {
        for name in [
            "streamingTenMinutes",
            "firstFramePresented",
            "nvstTransportActive",
            "nativeInputReady",
            "inputOwnershipExercised",
            "allGuidePagesVisited",
            "surfaceReconfigured",
            "fullscreenControlExercised",
            "statsControlExercised",
            "recordingRoundTrip",
            "mediaArtifactsComplete",
            "networkRecoveryExercised",
            "streamerRecoveryExercised",
            "noTerminalMediaError",
            "deviceRecoveryBalanced",
        ] {
            require(
                checks.get(name) == Some(&Value::Bool(true)),
                &format!("live check did not pass: {name}"),
                failures,
            );
        }
    }
    require(
        value["stream"]["status"] == "streaming",
        "live stream was not active",
        failures,
    );
    require(
        value["stream"]["transport"] == "nvst",
        "live stream transport is not NVST",
        failures,
    );
    require(
        value["stream"]["sessionUptimeMs"]
            .as_u64()
            .unwrap_or_default()
            >= 600_000,
        "live stream duration is under ten minutes",
        failures,
    );
    require_media_item(&value["media"]["screenshot"], false, failures);
    require_media_item(&value["media"]["recording"], true, failures);
}

fn require_media_item(value: &Value, thumbnail: bool, failures: &mut Vec<String>) {
    require(
        value.is_object(),
        "live media artifact is missing",
        failures,
    );
    require(
        safe_file_name(value["fileName"].as_str().unwrap_or_default()),
        "live media filename is invalid",
        failures,
    );
    require(
        value["sizeBytes"].as_u64().unwrap_or_default() > 0,
        "live media artifact is empty",
        failures,
    );
    require(
        valid_sha(value["sha256"].as_str().unwrap_or_default()),
        "live media SHA-256 is invalid",
        failures,
    );
    if thumbnail {
        require_media_item(&value["thumbnail"], false, failures);
    }
}

fn validate_performance(value: &Value, width: u64, height: u64, failures: &mut Vec<String>) {
    let prefix = format!("{width}x{height} performance");
    require(
        value["schemaVersion"] == 1,
        &format!("{prefix} schemaVersion must be 1"),
        failures,
    );
    require(
        value["kind"] == "opennow.qt.performance",
        &format!("{prefix} kind is invalid"),
        failures,
    );
    require(
        value["pass"] == true,
        &format!("{prefix} report did not pass"),
        failures,
    );
    require(
        value["failures"].as_array().is_some_and(Vec::is_empty),
        &format!("{prefix} contains failures"),
        failures,
    );
    let environment = &value["environment"];
    for field in ["requestedPhysicalWidth", "actualPhysicalWidth"] {
        require(
            environment[field].as_u64() == Some(width),
            &format!("{prefix} {field} is wrong"),
            failures,
        );
    }
    for field in ["requestedPhysicalHeight", "actualPhysicalHeight"] {
        require(
            environment[field].as_u64() == Some(height),
            &format!("{prefix} {field} is wrong"),
            failures,
        );
    }
    require(
        environment["refreshRateOverrideHz"].is_null(),
        &format!("{prefix} used a refresh override"),
        failures,
    );
    require(
        safe_label(environment["machineLabel"].as_str().unwrap_or_default()),
        &format!("{prefix} machineLabel is missing or invalid"),
        failures,
    );
    require(
        !environment["screen"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        &format!("{prefix} screen is missing"),
        failures,
    );
    let platform = environment["qtPlatform"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    require(
        !matches!(platform.as_str(), "" | "offscreen" | "minimal"),
        &format!("{prefix} used a non-representative Qt platform"),
        failures,
    );
    let graphics = environment["graphicsApi"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    require(
        !matches!(graphics.as_str(), "" | "software" | "null" | "unknown"),
        &format!("{prefix} used a non-hardware renderer"),
        failures,
    );
    require(
        value["workload"]["cycles"].as_u64().unwrap_or_default() >= 3,
        &format!("{prefix} used fewer than three cycles"),
        failures,
    );
    let steps = value["steps"].as_array();
    require(
        steps.is_some_and(|steps| steps.len() >= 27),
        &format!("{prefix} workload is incomplete"),
        failures,
    );
    if let Some(steps) = steps {
        for step in steps {
            let raw_name = step["name"].as_str().unwrap_or_default();
            let name = if safe_label(raw_name) {
                raw_name
            } else {
                "<invalid>"
            };
            require(
                safe_label(raw_name),
                &format!("{prefix} contains an invalid step name"),
                failures,
            );
            require(
                step["accepted"] == true,
                &format!("{prefix} rejected step {name}"),
                failures,
            );
            require(
                step["focusValid"] == true,
                &format!("{prefix} invalid focus at {name}"),
                failures,
            );
            require(
                step["firstFrameMs"]
                    .as_f64()
                    .is_some_and(|number| number.is_finite() && number >= 0.0),
                &format!("{prefix} missing frame at {name}"),
                failures,
            );
        }
    }
}

fn validate_attestations(value: &Value, matrix: Option<MatrixRow>, failures: &mut Vec<String>) {
    require(
        value["schemaVersion"] == 1,
        "attestations schemaVersion must be 1",
        failures,
    );
    require(
        value["kind"] == "opennow.acceptance-attestations",
        "attestations kind is invalid",
        failures,
    );
    require(
        safe_label(value["releaseCandidate"].as_str().unwrap_or_default()),
        "releaseCandidate is missing or invalid",
        failures,
    );
    require(
        safe_label(value["machineLabel"].as_str().unwrap_or_default()),
        "attestation machineLabel is missing or invalid",
        failures,
    );
    require(
        safe_label(value["applicationVersion"].as_str().unwrap_or_default()),
        "attestation applicationVersion is missing or invalid",
        failures,
    );
    require(
        !contains_sensitive(value),
        "attestations contain a sensitive key or value",
        failures,
    );
    let checks = value["checks"].as_object();
    require(
        checks.is_some(),
        "attestation checks object is missing",
        failures,
    );
    if let Some(checks) = checks {
        for name in REQUIRED_ATTESTATIONS {
            require(
                checks.get(*name) == Some(&Value::Bool(true)),
                &format!("manual attestation did not pass: {name}"),
                failures,
            );
        }
    }
    for name in ["hdr", "vrr"] {
        let capability = &value["displayCapabilities"][name];
        let status = capability["status"].as_str().unwrap_or_default();
        require(
            matches!(status, "passed" | "not-supported"),
            &format!("display capability {name} has no valid status"),
            failures,
        );
        require(
            !capability["evidence"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            &format!("display capability {name} evidence is missing"),
            failures,
        );
    }
    if matrix.is_some() {
        require(
            value["matrixRow"]
                .as_str()
                .is_some_and(|name| matrix_row(name).is_some()),
            "matrix row is invalid",
            failures,
        );
    }
}

fn validate_cross_report(
    live: &Value,
    first: &Value,
    second: &Value,
    attestations: &Value,
    matrix: Option<MatrixRow>,
    failures: &mut Vec<String>,
) {
    for value in [first, second] {
        require(
            value["environment"]["machineLabel"] == attestations["machineLabel"],
            "performance machineLabel differs from attestations",
            failures,
        );
        require(
            value["environment"]["applicationVersion"] == live["applicationVersion"],
            "application versions differ between live and performance evidence",
            failures,
        );
    }
    require(
        attestations["applicationVersion"] == live["applicationVersion"],
        "attestation applicationVersion differs from live evidence",
        failures,
    );
    require(
        first["environment"]["cpuArchitecture"] == second["environment"]["cpuArchitecture"],
        "performance CPU architectures differ",
        failures,
    );
    require(
        first["environment"]["qtPlatform"] == second["environment"]["qtPlatform"],
        "performance Qt platforms differ",
        failures,
    );
    if let Some(matrix) = matrix {
        require(
            normalize_arch(
                live["platform"]["cpuArchitecture"]
                    .as_str()
                    .unwrap_or_default(),
            ) == matrix.architecture,
            "live architecture does not match matrix row",
            failures,
        );
        require(
            normalize_arch(
                first["environment"]["cpuArchitecture"]
                    .as_str()
                    .unwrap_or_default(),
            ) == matrix.architecture,
            "performance architecture does not match matrix row",
            failures,
        );
        let live_os = live["platform"]["os"].as_str().unwrap_or_default();
        require(
            os_matches(matrix.os, live_os),
            "live OS does not match matrix row",
            failures,
        );
        let qt_platform = first["environment"]["qtPlatform"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        require(
            matrix
                .window_system
                .iter()
                .any(|allowed| qt_platform.contains(allowed)),
            "Qt platform does not match matrix row",
            failures,
        );
        let live_window = live["platform"]["windowSystem"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        require(
            matrix
                .window_system
                .iter()
                .any(|allowed| live_window.contains(allowed)),
            "live window system does not match matrix row",
            failures,
        );
    }
}

fn validate_packages(
    value: &Value,
    matrix: Option<MatrixRow>,
    actual: &BTreeMap<String, Value>,
    failures: &mut Vec<String>,
) {
    let declared = value["packages"].as_array();
    require(
        declared.is_some(),
        "attestation packages array is missing",
        failures,
    );
    let mut declared_names = BTreeSet::new();
    if let Some(declared) = declared {
        for package in declared {
            let name = package["fileName"].as_str().unwrap_or_default();
            let display_name = if safe_file_name(name) {
                name
            } else {
                "<invalid>"
            };
            declared_names.insert(name.to_owned());
            require(
                safe_file_name(name),
                "declared package filename is invalid",
                failures,
            );
            require(
                actual.get(name).is_some(),
                &format!("declared package was not supplied: {display_name}"),
                failures,
            );
            if let Some(actual) = actual.get(name) {
                require(
                    package["sha256"] == actual["sha256"],
                    &format!("package SHA-256 mismatch: {display_name}"),
                    failures,
                );
                require(
                    package["sizeBytes"] == actual["sizeBytes"],
                    &format!("package size mismatch: {display_name}"),
                    failures,
                );
            }
            require(
                package["signingVerified"] == true,
                &format!("package signing was not verified: {display_name}"),
                failures,
            );
            require(
                package["updateManifestVerified"] == true,
                &format!("update manifest was not verified: {display_name}"),
                failures,
            );
        }
    }
    require(
        declared_names == actual.keys().cloned().collect(),
        "supplied and declared package sets differ",
        failures,
    );
    if let Some(matrix) = matrix {
        let lower = declared_names
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let has = |suffix: &str| lower.iter().any(|name| name.ends_with(suffix));
        match matrix.os {
            MatrixOs::Windows => {
                require(
                    has(".msi") || has(".exe"),
                    "Windows installer package is missing",
                    failures,
                );
                require(has(".zip"), "Windows portable ZIP is missing", failures);
            }
            MatrixOs::Macos => {
                require(has(".dmg"), "macOS DMG is missing", failures);
                require(has(".zip"), "macOS ZIP is missing", failures);
            }
            MatrixOs::Linux => {
                require(has(".appimage"), "Linux AppImage is missing", failures);
                require(has(".deb"), "Linux DEB is missing", failures);
            }
        }
    }
}

fn hash_packages(paths: &[PathBuf]) -> Result<BTreeMap<String, Value>, String> {
    let mut output = BTreeMap::new();
    for path in paths {
        let evidence = file_evidence(path)?;
        let name = evidence["fileName"].as_str().unwrap_or_default().to_owned();
        if output.insert(name.clone(), evidence).is_some() {
            return Err(format!("Duplicate package filename: {name}"));
        }
    }
    Ok(output)
}

fn file_evidence(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "Evidence must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !safe_file_name(name) {
        return Err(format!("Unsafe evidence filename: {}", path.display()));
    }
    let file = fs::File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(
        json!({"fileName": name, "sizeBytes": metadata.len(), "sha256": format!("{:x}", hasher.finalize())}),
    )
}

fn write_new_json(path: &Path, value: &Value) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "Output already exists; choose a new path: {}",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Output has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create output directory: {error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("acceptance"),
        std::process::id(),
        nonce
    ));
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create temporary output: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not write output: {error}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not commit output: {error}"));
    }
    Ok(())
}

fn matrix_row(name: &str) -> Option<MatrixRow> {
    match name {
        "windows-x64" => Some(MatrixRow {
            os: MatrixOs::Windows,
            architecture: "x64",
            window_system: &["windows"],
        }),
        "windows-arm64" => Some(MatrixRow {
            os: MatrixOs::Windows,
            architecture: "arm64",
            window_system: &["windows"],
        }),
        "macos-apple-silicon" => Some(MatrixRow {
            os: MatrixOs::Macos,
            architecture: "arm64",
            window_system: &["cocoa"],
        }),
        "macos-intel" => Some(MatrixRow {
            os: MatrixOs::Macos,
            architecture: "x64",
            window_system: &["cocoa"],
        }),
        "linux-x64-x11" => Some(MatrixRow {
            os: MatrixOs::Linux,
            architecture: "x64",
            window_system: &["xcb"],
        }),
        "linux-x64-wayland" => Some(MatrixRow {
            os: MatrixOs::Linux,
            architecture: "x64",
            window_system: &["wayland"],
        }),
        "linux-arm64-native" => Some(MatrixRow {
            os: MatrixOs::Linux,
            architecture: "arm64",
            window_system: &["xcb", "wayland"],
        }),
        _ => None,
    }
}

fn normalize_arch(value: &str) -> &'static str {
    match value.to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" | "x64" => "x64",
        "aarch64" | "arm64" => "arm64",
        _ => "unknown",
    }
}

fn os_matches(expected: MatrixOs, value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    match expected {
        MatrixOs::Windows => value.contains("windows"),
        MatrixOs::Macos => value.contains("macos") || value.contains("darwin"),
        MatrixOs::Linux => value.contains("linux"),
    }
}

fn contains_sensitive(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            [
                "token",
                "authorization",
                "password",
                "secret",
                "sessionid",
                "processid",
                "executable",
                "filepath",
                "url",
            ]
            .iter()
            .any(|needle| key.contains(needle))
                || contains_sensitive(value)
        }),
        Value::Array(values) => values.iter().any(contains_sensitive),
        Value::String(value) => {
            let lower = value.to_ascii_lowercase();
            value.contains('@')
                || lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("wss://")
                || lower.starts_with("/home/")
                || lower.starts_with("/users/")
                || lower.starts_with("c:\\users\\")
        }
        _ => false,
    }
}

fn safe_file_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

fn safe_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn safe_output_label(value: &Value) -> Value {
    value
        .as_str()
        .filter(|value| safe_label(value))
        .map(|value| Value::String(value.to_owned()))
        .unwrap_or(Value::Null)
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn require(condition: bool, message: &str, failures: &mut Vec<String>) {
    if !condition {
        failures.push(message.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn write_value(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn performance_report(width: u64, height: u64) -> Value {
        let steps = (0..27)
            .map(|index| {
                json!({"name":format!("step-{index}"),"accepted":true,
                       "focusValid":true,"firstFrameMs":1.0})
            })
            .collect::<Vec<_>>();
        json!({
            "schemaVersion":1,"kind":"opennow.qt.performance","pass":true,"failures":[],
            "environment":{"applicationVersion":"1.0.0","machineLabel":"machine-a",
                "cpuArchitecture":"x86_64","qtPlatform":"wayland","graphicsApi":"vulkan",
                "screen":"Display-1","refreshRateOverrideHz":null,
                "requestedPhysicalWidth":width,"actualPhysicalWidth":width,
                "requestedPhysicalHeight":height,"actualPhysicalHeight":height},
            "workload":{"cycles":3},"steps":steps
        })
    }

    fn live_report() -> Value {
        let mut checks = Map::new();
        for name in [
            "streamingTenMinutes",
            "firstFramePresented",
            "nvstTransportActive",
            "nativeInputReady",
            "inputOwnershipExercised",
            "allGuidePagesVisited",
            "surfaceReconfigured",
            "fullscreenControlExercised",
            "statsControlExercised",
            "recordingRoundTrip",
            "mediaArtifactsComplete",
            "networkRecoveryExercised",
            "streamerRecoveryExercised",
            "noTerminalMediaError",
            "deviceRecoveryBalanced",
        ] {
            checks.insert(name.to_owned(), Value::Bool(true));
        }
        let media = |name: &str| json!({"fileName":name,"sizeBytes":1,"sha256":"0".repeat(64)});
        json!({
            "schemaVersion":1,"kind":"opennow.live-acceptance","observedPass":true,
            "scope":"machine-observed-live-runtime","applicationVersion":"1.0.0",
            "platform":{"os":"linux","cpuArchitecture":"x86_64","windowSystem":"wayland"},
            "checks":checks,"stream":{"status":"streaming","transport":"nvst","sessionUptimeMs":600000},
            "media":{"screenshot":media("shot.png"),
                "recording":{"fileName":"clip.mkv","sizeBytes":1,"sha256":"0".repeat(64),
                             "thumbnail":media("thumb.jpg")}}
        })
    }

    #[test]
    fn matrix_rows_are_explicit_and_architectures_normalize() {
        assert_eq!(matrix_row("linux-x64-wayland").unwrap().os, MatrixOs::Linux);
        assert!(matrix_row("linux-auto").is_none());
        assert_eq!(normalize_arch("x86_64"), "x64");
        assert_eq!(normalize_arch("aarch64"), "arm64");
    }

    #[test]
    fn live_validation_rejects_false_checks_sensitive_values_and_bad_hashes() {
        let mut value = json!({
            "schemaVersion":1, "kind":"opennow.live-acceptance", "observedPass":true,
            "scope":"machine-observed-live-runtime", "applicationVersion":"1.0.0",
            "checks":{}, "stream":{"status":"streaming","transport":"nvst","sessionUptimeMs":600000},
            "media":{"screenshot":{"fileName":"shot.png","sizeBytes":1,"sha256":"0".repeat(64)},
                     "recording":{"fileName":"clip.mkv","sizeBytes":1,"sha256":"0".repeat(64),
                                  "thumbnail":{"fileName":"thumb.jpg","sizeBytes":1,"sha256":"0".repeat(64)}}}
        });
        for name in [
            "streamingTenMinutes",
            "firstFramePresented",
            "nvstTransportActive",
            "nativeInputReady",
            "inputOwnershipExercised",
            "allGuidePagesVisited",
            "surfaceReconfigured",
            "fullscreenControlExercised",
            "statsControlExercised",
            "recordingRoundTrip",
            "mediaArtifactsComplete",
            "networkRecoveryExercised",
            "streamerRecoveryExercised",
            "noTerminalMediaError",
            "deviceRecoveryBalanced",
        ] {
            value["checks"][name] = Value::Bool(true);
        }
        let mut failures = Vec::new();
        validate_live(&value, &mut failures);
        assert!(failures.is_empty(), "{failures:?}");
        value["checks"]["nativeInputReady"] = Value::Bool(false);
        value["stream"]["transport"] = Value::String("webrtc".to_owned());
        value["unsafeUrl"] = Value::String("https://example.invalid".to_owned());
        value["media"]["screenshot"]["sha256"] = Value::String("BAD".to_owned());
        validate_live(&value, &mut failures);
        assert!(
            failures
                .iter()
                .any(|value| value.contains("nativeInputReady"))
        );
        assert!(failures.iter().any(|value| value.contains("sensitive")));
        assert!(failures.iter().any(|value| value.contains("SHA-256")));
        assert!(failures.iter().any(|value| value.contains("not NVST")));
    }

    #[test]
    fn required_manual_attestation_names_are_unique() {
        let unique = REQUIRED_ATTESTATIONS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), REQUIRED_ATTESTATIONS.len());
    }

    #[test]
    fn complete_bundle_passes_and_package_tampering_fails_closed() {
        let root = env::temp_dir().join(format!(
            "opennow-acceptance-verifier-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let live = root.join("live.json");
        let perf_1080p = root.join("1080p.json");
        let perf_4k = root.join("4k.json");
        let attestations = root.join("attestations.json");
        let appimage = root.join("OpenNOW.AppImage");
        let deb = root.join("opennow.deb");
        fs::write(&appimage, b"appimage").unwrap();
        fs::write(&deb, b"deb").unwrap();
        write_value(&live, &live_report());
        write_value(&perf_1080p, &performance_report(1920, 1080));
        write_value(&perf_4k, &performance_report(3840, 2160));
        let mut checks = Map::new();
        for name in REQUIRED_ATTESTATIONS {
            checks.insert((*name).to_owned(), Value::Bool(true));
        }
        let package = |path: &Path| {
            let mut evidence = file_evidence(path).unwrap();
            evidence["signingVerified"] = Value::Bool(true);
            evidence["updateManifestVerified"] = Value::Bool(true);
            evidence
        };
        write_value(
            &attestations,
            &json!({
                "schemaVersion":1,"kind":"opennow.acceptance-attestations",
                "releaseCandidate":"v1.0.0-rc1","applicationVersion":"1.0.0",
                "matrixRow":"linux-x64-wayland","machineLabel":"machine-a","checks":checks,
                "displayCapabilities":{"hdr":{"status":"not-supported","evidence":"display probe"},
                                       "vrr":{"status":"passed","evidence":"capture hash"}},
                "packages":[package(&appimage),package(&deb)]
            }),
        );
        let arguments = Arguments {
            live,
            performance_1080p: perf_1080p,
            performance_4k: perf_4k,
            attestations,
            packages: vec![appimage.clone(), deb],
            output: root.join("result.json"),
        };
        assert!(verify(&arguments).unwrap());
        assert_eq!(read_json(&arguments.output).unwrap()["pass"], true);
        fs::write(&appimage, b"tampered").unwrap();
        let failed_arguments = Arguments {
            output: root.join("failed.json"),
            ..arguments
        };
        assert!(!verify(&failed_arguments).unwrap());
        let failure = read_json(&failed_arguments.output).unwrap();
        assert_eq!(failure["pass"], false);
        assert!(failure["failures"].as_array().unwrap().iter().any(|value| {
            value
                .as_str()
                .is_some_and(|value| value.contains("SHA-256 mismatch"))
        }));
        let _ = fs::remove_dir_all(root);
    }
}
