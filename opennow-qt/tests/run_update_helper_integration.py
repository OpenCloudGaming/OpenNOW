import base64
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "native/opennow-core/Cargo.toml"


def run(command, environment):
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=1200)


def build_fixtures(directory, environment):
    manifest = directory / "Cargo.toml"
    manifest.write_text('''[package]
name = "opennow-update-test-fixtures"
version = "0.0.0"
edition = "2024"
publish = false

[[bin]]
name = "previous"
path = "previous.rs"

[[bin]]
name = "candidate"
path = "candidate.rs"

[dependencies]
opennow-core = { path = ''' + json.dumps(str(MANIFEST.parent)) + ''' }
''', encoding="utf-8")
    shutil.copyfile(MANIFEST.with_name("Cargo.lock"), directory / "Cargo.lock")
    run(["cargo", "build", "--offline", "--manifest-path", str(manifest), "--bins"], environment)


def main():
    openssl = shutil.which("openssl")
    if openssl is None and os.name == "nt":
        candidate = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/usr/bin/openssl.exe"
        if candidate.is_file():
            openssl = str(candidate)
    if openssl is None:
        raise RuntimeError("OpenSSL is required to derive the ephemeral Ed25519 public key")
    with tempfile.TemporaryDirectory(prefix="opennow-signed-helper-") as temporary:
        directory = Path(temporary).resolve()
        seed = secrets.token_bytes(32)
        seed_file = directory / "test-only-signing-seed"
        seed_file.write_bytes(seed)
        seed_file.chmod(0o600)
        public_der = subprocess.run(
            [openssl, "pkey", "-inform", "DER", "-pubout", "-outform", "DER"],
            input=bytes.fromhex("302e020100300506032b657004220420") + seed,
            capture_output=True, check=True, timeout=30,
        ).stdout
        if len(public_der) != 44 or public_der[:12] != bytes.fromhex("302a300506032b6570032100"):
            raise RuntimeError("OpenSSL returned an unexpected Ed25519 public key encoding")
        environment = os.environ.copy()
        environment["CARGO_TARGET_DIR"] = str(directory / "target")
        environment["OPENNOW_UPDATE_ED25519_PUBLIC_KEY"] = base64.b64encode(public_der[12:]).decode("ascii")
        environment["OPENNOW_TEST_UPDATE_SEED_FILE"] = str(seed_file)
        for name in ("OPENNOW_UPDATE_PLAN", "OPENNOW_UPDATE_NONCE", "APPIMAGE", "APPDIR", "ARGV0"):
            environment.pop(name, None)
        run(["cargo", "build", "--locked", "--manifest-path", str(MANIFEST), "--lib", "--bin", "opennow-update-helper"], environment)
        suffix = ".exe" if os.name == "nt" else ""
        debug = directory / "target/debug"
        previous = debug / ("previous" + suffix)
        candidate = debug / ("candidate" + suffix)
        previous_source = directory / "previous.rs"
        previous_source.write_text('''
fn main() {
    if std::env::args().any(|argument| argument == "--hold") {
        std::thread::sleep(std::time::Duration::from_secs(180));
    } else if let Some(directory) = std::env::var_os("OPENNOW_DATA_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("previous-restarted"), b"1.0.0").unwrap();
    }
}
''', encoding="utf-8")
        candidate_source = directory / "candidate.rs"
        candidate_source.write_text('''
fn main() {
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    let plan = std::path::PathBuf::from(std::env::var_os("OPENNOW_UPDATE_PLAN").unwrap());
    let directory = plan.parent().unwrap();
    std::fs::write(directory.join("candidate-started"), std::process::id().to_string()).unwrap();
    if directory.join("reject-startup").exists() { std::process::exit(9); }
    unsafe {
        std::env::set_var("OPENNOW_APP_PID", std::process::id().to_string());
        std::env::set_var("OPENNOW_APP_EXECUTABLE", &executable);
        #[cfg(target_os = "linux")]
        std::env::set_var("APPIMAGE", &executable);
    }
    assert!(opennow_core::update_apply::acknowledge_startup_from_env("1.2.3").unwrap());
    let start = std::time::Instant::now();
    while !directory.join("exit-fixture").exists() && start.elapsed().as_secs() < 30 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
''', encoding="utf-8")
        build_fixtures(directory, environment)
        environment["OPENNOW_TEST_UPDATE_PREVIOUS"] = str(previous)
        environment["OPENNOW_TEST_UPDATE_CANDIDATE"] = str(candidate)
        environment["OPENNOW_TEST_UPDATE_HELPER"] = str(debug / ("opennow-update-helper" + suffix))
        listed = subprocess.run(
            ["cargo", "test", "--locked", "--manifest-path", str(MANIFEST), "--lib",
             "update_apply::integration_tests", "--", "--ignored", "--list"],
            cwd=ROOT, env=environment, check=True, capture_output=True, text=True, timeout=1200,
        )
        if listed.stdout.count(": test") != 4:
            raise RuntimeError("Expected all four signed helper integration tests to be registered")
        run(["cargo", "test", "--locked", "--manifest-path", str(MANIFEST), "--lib",
             "update_apply::integration_tests", "--", "--ignored", "--test-threads=1", "--nocapture"], environment)


if __name__ == "__main__":
    main()
