use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const ID: &str = "org.opennow.example.catalog";

struct Core {
    child: Child,
    replies: mpsc::Receiver<Value>,
    serial: usize,
}

impl Core {
    fn start(data: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
            .arg("--data-dir")
            .arg(data)
            .env("OPENNOW_TEST_PARENT_SECRET", "synthetic-parent-secret")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    return;
                }
            }
        });
        let mut core = Self {
            child,
            replies,
            serial: 0,
        };
        let start = Instant::now();
        let hello = core.call("core.hello", json!({"protocolVersion":5,"shell":"qt"}));
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(hello["ok"], true, "{hello}");
        assert!(
            hello["result"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("plugins.v1"))
        );
        core
    }

    fn send(&mut self, method: &str, params: Value) -> String {
        self.serial += 1;
        let id = self.serial.to_string();
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"type":"request","id":id,"method":method,"params":params})
        )
        .unwrap();
        id
    }

    fn response(&self, id: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let reply = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("core response timed out");
            if reply["type"] == "response" && reply["id"] == id {
                return reply;
            }
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.response(&id)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let reply = self.call(method, params);
        assert_eq!(reply["ok"], true, "{method}: {reply}");
        reply["result"].clone()
    }

    fn generation(&mut self) -> u64 {
        self.ok("plugins.list", json!({}))["generation"]
            .as_u64()
            .unwrap()
    }

    fn set_enabled(&mut self, enabled: bool) -> Value {
        self.set_enabled_for(ID, enabled)
    }

    fn set_enabled_for(&mut self, id: &str, enabled: bool) -> Value {
        let generation = self.generation();
        self.call(
            "plugins.setEnabled",
            json!({"id":id,"enabled":enabled,"expectedGeneration":generation}),
        )
    }

    fn install(&mut self, path: &Path) {
        let inspected = self.ok(
            "plugins.install.inspect",
            json!({"path":path.to_str().unwrap()}),
        );
        self.ok("plugins.install.commit", json!({"token":inspected["inspection"]["token"],"expectedGeneration":inspected["generation"],"consent":true}));
    }

    fn wait_state(&mut self, state: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let snapshot = self.ok("plugins.list", json!({}));
            if let Some(plugin) = snapshot["plugins"]
                .as_array()
                .unwrap()
                .iter()
                .find(|plugin| plugin["id"] == ID && plugin["state"] == state)
            {
                return plugin.clone();
            }
            assert!(Instant::now() < deadline, "missing {state}: {snapshot}");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn close(self) {
        self.close_within(Duration::from_secs(10));
    }

    fn close_within(mut self, budget: Duration) {
        self.child.stdin.take();
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("core did not stop after EOF");
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn example_package() -> &'static Path {
    static PACKAGE: OnceLock<PathBuf> = OnceLock::new();
    PACKAGE.get_or_init(|| {
        let target =
            std::env::temp_dir().join(format!("opennow-plugin-example-{}", std::process::id()));
        let manifest =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/catalog-plugin/Cargo.toml");
        assert!(
            Command::new(env!("CARGO"))
                .args(["build", "--locked", "--manifest-path"])
                .arg(&manifest)
                .env("CARGO_TARGET_DIR", &target)
                .status()
                .unwrap()
                .success()
        );
        let executable = target.join("debug").join(format!(
            "opennow-example-catalog{}",
            std::env::consts::EXE_SUFFIX
        ));
        let package = target.join("example.opennow-plugin");
        assert!(
            Command::new(executable)
                .arg("--package")
                .arg(&package)
                .status()
                .unwrap()
                .success()
        );
        package
    })
}

fn fixture(directory: &Path, mode: &str) -> PathBuf {
    fixture_for(directory, mode, ID)
}

fn fixture_for(directory: &Path, mode: &str, id: &str) -> PathBuf {
    let executable = directory.join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog_plugin_process.rs");
    let target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let mut compiler = Command::new("rustc");
    compiler
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .env("PLUGIN_FIXTURE_MODE", mode)
        .env("PLUGIN_FIXTURE_ID", id);
    let linker_key = format!(
        "CARGO_TARGET_{}_LINKER",
        target.replace('-', "_").to_ascii_uppercase()
    );
    if let Some(linker) = std::env::var_os(linker_key) {
        let mut argument = std::ffi::OsString::from("linker=");
        argument.push(linker);
        compiler.arg("-C").arg(argument);
    }
    assert!(compiler.status().unwrap().success());
    let package = directory.join("fixture.opennow-plugin");
    let bytes = fs::read(executable).unwrap();
    let manifest = json!({"schemaVersion":1,"id":id,"name":"Process test fixture","description":"Synthetic protocol fixture","version":"1.0.0","publisher":"Test fixture","protocolVersion":1,"capabilities":["catalog.v1"],"entrypoints":{target:"plugin"},"files":[{"path":"plugin","sha256":format!("{:x}",Sha256::digest(&bytes))}]});
    let mut zip = zip::ZipWriter::new(fs::File::create(&package).unwrap());
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.start_file(
        "plugin",
        zip::write::SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(&bytes).unwrap();
    zip.finish().unwrap();
    package
}

fn pid(data: &Path) -> u32 {
    fs::read_to_string(data.join("plugins/data").join(ID).join("last-process-id"))
        .unwrap()
        .parse()
        .unwrap()
}

#[cfg(unix)]
fn assert_stopped(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if unsafe { libc::kill(pid as i32, 0) } != 0 {
            return;
        }
        assert!(Instant::now() < deadline, "plugin {pid} remains alive");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(windows)]
fn assert_stopped(pid: u32) {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};
    let handle = unsafe { OpenProcess(0x00100000, 0, pid) };
    if handle.is_null() {
        assert_eq!(unsafe { GetLastError() }, 87);
        return;
    }
    let result = unsafe { WaitForSingleObject(handle, 3000) };
    unsafe {
        CloseHandle(handle);
    }
    assert_eq!(result, WAIT_OBJECT_0);
}

#[test]
fn example_install_consent_catalog_restart_disable_and_uninstall() {
    let data = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    let inspected = core.ok(
        "plugins.install.inspect",
        json!({"path":example_package().to_str().unwrap()}),
    );
    assert!(!data.path().join("plugins/data").join(ID).exists());
    let commit = json!({"token":inspected["inspection"]["token"],"expectedGeneration":inspected["generation"],"consent":false});
    assert_eq!(
        core.call("plugins.install.commit", commit)["error"]["code"],
        "consent_required"
    );
    core.ok("plugins.install.commit", json!({"token":inspected["inspection"]["token"],"expectedGeneration":inspected["generation"],"consent":true}));
    core.wait_state("disabled");
    assert!(!data.path().join("plugins/data").join(ID).exists());
    assert_eq!(
        core.call("sources.catalog.page", json!({"sourceId":ID}))["error"]["code"],
        "plugin_not_ready"
    );
    assert_eq!(core.set_enabled(true)["ok"], true);
    let first_pid = pid(data.path());
    let page = core.ok(
        "sources.catalog.page",
        json!({"sourceId":ID,"query":"game","limit":2}),
    );
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][0]["id"]["sourceId"], ID);
    assert_eq!(page["items"][0]["title"], "Example game 01");
    assert_eq!(page["nextCursor"], "2");
    core.close();
    assert_stopped(first_pid);
    let mut core = Core::start(data.path());
    core.wait_state("ready");
    let second_pid = pid(data.path());
    assert_ne!(first_pid, second_pid);
    assert_eq!(core.set_enabled(false)["ok"], true);
    assert_stopped(second_pid);
    let generation = core.generation();
    assert_eq!(
        core.call(
            "plugins.uninstall",
            json!({"id":ID,"expectedGeneration":generation,"confirmed":false})
        )["error"]["code"],
        "consent_required"
    );
    core.ok(
        "plugins.uninstall",
        json!({"id":ID,"expectedGeneration":generation,"confirmed":true}),
    );
    assert!(!data.path().join("plugins/data").join(ID).exists());
    assert!(!data.path().join("plugins/installed").join(ID).exists());
    core.close();
}

#[test]
fn malformed_flooded_spoofed_or_oversized_child_fails_closed() {
    for mode in [
        "malformed",
        "oversized",
        "wrong_epoch",
        "wrong_id",
        "event",
        "flood",
    ] {
        let data = TempDir::new().unwrap();
        let package_dir = TempDir::new().unwrap();
        let package = fixture(package_dir.path(), mode);
        let mut core = Core::start(data.path());
        core.install(&package);
        assert_eq!(core.set_enabled(true)["ok"], true, "{mode}");
        let process = pid(data.path());
        let response = core.call("sources.catalog.page", json!({"sourceId":ID}));
        if mode != "flood" {
            assert_eq!(response["ok"], false, "{mode}: {response}");
        }
        core.wait_state("failed");
        assert_stopped(process);
        core.close();
    }
}

#[test]
fn parent_environment_is_not_inherited_and_stderr_is_discarded() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let package = fixture(package_dir.path(), "stderr");
    let mut core = Core::start(data.path());
    core.install(&package);
    assert_eq!(core.set_enabled(true)["ok"], true);
    let page = core.ok("sources.catalog.page", json!({"sourceId":ID}));
    assert_eq!(page["items"][0]["title"], "scrubbed");
    assert_eq!(core.set_enabled(false)["ok"], true);
    let registry = fs::read_to_string(data.path().join("plugins/registry.json")).unwrap();
    assert!(!registry.contains("synthetic-parent-secret"));
    assert!(!registry.contains("SSSSSSSS"));
    core.close();
}

#[test]
fn cancellation_retires_uncooperative_child_once_and_next_preview_works() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "cancel_restart"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    let id = core.send(
        "sources.catalog.page",
        json!({"sourceId":ID,"query":"wait"}),
    );
    wait_file(
        &data
            .path()
            .join("plugins/data")
            .join(ID)
            .join("request-waiting"),
    );
    writeln!(
        core.child.stdin.as_mut().unwrap(),
        "{}",
        json!({"type":"cancel","id":id})
    )
    .unwrap();
    assert_stopped(process);
    assert_eq!(core.wait_state("ready")["enabled"], true);
    assert_ne!(pid(data.path()), process);
    let page = core.ok("sources.catalog.page", json!({"sourceId":ID}));
    assert_eq!(page["items"][0]["title"], "scrubbed");
    core.close();
}

fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "fixture did not reach the request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cooperative_preview_cancellation_keeps_enabled_process_and_next_query_works() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "cooperative_cancel"));
    let enabled = core.set_enabled(true);
    assert_eq!(enabled["ok"], true, "{enabled}");
    let process = pid(data.path());
    let id = core.send(
        "sources.catalog.page",
        json!({"sourceId":ID,"query":"wait"}),
    );
    wait_file(
        &data
            .path()
            .join("plugins/data")
            .join(ID)
            .join("request-waiting"),
    );
    writeln!(
        core.child.stdin.as_mut().unwrap(),
        "{}",
        json!({"type":"cancel","id":id})
    )
    .unwrap();
    let page = core.ok(
        "sources.catalog.page",
        json!({"sourceId":ID,"query":"replacement"}),
    );
    assert_eq!(page["items"][0]["title"], "scrubbed");
    assert_eq!(pid(data.path()), process);
    assert_eq!(core.wait_state("ready")["enabled"], true);
    core.close();
}

#[test]
fn a_valid_request_error_does_not_disable_or_restart_the_plugin() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "request_error"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    assert_eq!(
        core.call(
            "sources.catalog.page",
            json!({"sourceId":ID,"query":"invalid"})
        )["error"]["code"],
        "plugin_request_failed"
    );
    let page = core.ok(
        "sources.catalog.page",
        json!({"sourceId":ID,"query":"valid"}),
    );
    assert_eq!(page["items"][0]["title"], "scrubbed");
    assert_eq!(pid(data.path()), process);
    assert_eq!(core.wait_state("ready")["enabled"], true);
    core.close();
}

#[test]
fn core_shutdown_kills_all_uncooperative_children_within_qt_budget() {
    let data = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    let mut processes = Vec::new();
    let mut packages = Vec::new();
    for id in [
        "org.example.catalogone",
        "org.example.catalogtwo",
        "org.example.catalogthree",
        "org.example.catalogfour",
    ] {
        let package_dir = TempDir::new().unwrap();
        core.install(&fixture_for(package_dir.path(), "ignore_shutdown", id));
        assert_eq!(core.set_enabled_for(id, true)["ok"], true);
        processes.push(
            fs::read_to_string(
                data.path()
                    .join("plugins/data")
                    .join(id)
                    .join("last-process-id"),
            )
            .unwrap()
            .parse()
            .unwrap(),
        );
        packages.push(package_dir);
    }
    core.close_within(Duration::from_millis(1500));
    for process in processes {
        assert_stopped(process);
    }
}

#[cfg(unix)]
#[test]
fn core_eof_during_graceful_disable_still_owns_and_kills_the_child() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "ignore_shutdown"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    let generation = core.generation();
    core.send(
        "plugins.setEnabled",
        json!({"id":ID,"enabled":false,"expectedGeneration":generation}),
    );
    wait_file(
        &data
            .path()
            .join("plugins/data")
            .join(ID)
            .join("shutdown-waiting"),
    );
    core.wait_state("disabled");
    core.close_within(Duration::from_millis(1500));
    let orphaned = unsafe { libc::kill(process as i32, 0) } == 0;
    if orphaned {
        unsafe {
            libc::kill(process as i32, libc::SIGKILL);
        }
    }
    assert!(!orphaned, "core lost ownership during graceful disable");
}

#[test]
fn malformed_core_input_still_kills_the_live_plugin_before_exit() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "ignore_shutdown"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    writeln!(core.child.stdin.as_mut().unwrap(), "malformed JSON").unwrap();
    let deadline = Instant::now() + Duration::from_millis(1500);
    while core.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_stopped(process);
}

#[test]
fn restored_hanging_plugin_does_not_delay_core_hello() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "hang_hello"));
    core.close();
    let path = data.path().join("plugins/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    registry["plugins"][0]["enabled"] = json!(true);
    fs::write(path, serde_json::to_vec(&registry).unwrap()).unwrap();
    let mut core = Core::start(data.path());
    core.wait_state("failed");
    assert_stopped(pid(data.path()));
    core.close();
}

#[test]
fn corrupt_optional_registry_does_not_prevent_core_handshake() {
    let data = TempDir::new().unwrap();
    let root = data.path().join("plugins");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("registry.json"), "invalid registry bytes").unwrap();
    let mut core = Core::start(data.path());
    assert_eq!(
        core.call("plugins.list", json!({}))["error"]["code"],
        "plugin_storage_error"
    );
    core.close();
    assert_eq!(
        fs::read_to_string(root.join("registry.json")).unwrap(),
        "invalid registry bytes"
    );
}

#[test]
fn oversized_registry_generation_is_isolated_without_overwrite_or_overflow() {
    for generation in [u64::MAX, 1u64 << 53, i32::MAX as u64] {
        let data = TempDir::new().unwrap();
        let root = data.path().join("plugins");
        fs::create_dir_all(&root).unwrap();
        let original =
            serde_json::to_vec(&json!({"version":1,"generation":generation,"plugins":[]})).unwrap();
        fs::write(root.join("registry.json"), &original).unwrap();
        let mut core = Core::start(data.path());
        assert_eq!(
            core.call("plugins.list", json!({}))["error"]["code"],
            "plugin_storage_error"
        );
        core.close();
        assert_eq!(fs::read(root.join("registry.json")).unwrap(), original);
    }
}

#[test]
fn shutdown_during_restore_terminates_the_new_child() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "hang_hello"));
    core.close();
    let path = data.path().join("plugins/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    registry["plugins"][0]["enabled"] = json!(true);
    fs::write(path, serde_json::to_vec(&registry).unwrap()).unwrap();
    let core = Core::start(data.path());
    let marker = data
        .path()
        .join("plugins/data")
        .join(ID)
        .join("last-process-id");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let process = pid(data.path());
    core.close();
    assert_stopped(process);
}

#[test]
fn inspection_tokens_stale_actions_and_modified_install_fail_closed() {
    let data = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    let inspected = core.ok(
        "plugins.install.inspect",
        json!({"path":url::Url::from_file_path(example_package()).unwrap().as_str()}),
    );
    let cancel = json!({"token":inspected["inspection"]["token"]});
    core.ok("plugins.install.cancel", cancel.clone());
    core.ok("plugins.install.cancel", cancel);
    assert_eq!(core.call("plugins.install.commit",json!({"token":inspected["inspection"]["token"],"expectedGeneration":inspected["generation"],"consent":true}))["error"]["code"],"inspection_expired");
    core.install(example_package());
    assert_eq!(
        core.call(
            "plugins.install.inspect",
            json!({"path":example_package().to_str().unwrap()})
        )["error"]["code"],
        "plugin_already_installed"
    );
    assert_eq!(
        core.call(
            "plugins.setEnabled",
            json!({"id":ID,"expectedGeneration":0,"enabled":true})
        )["error"]["code"],
        "stale_registry"
    );
    let registry: Value =
        serde_json::from_slice(&fs::read(data.path().join("plugins/registry.json")).unwrap())
            .unwrap();
    let record = &registry["plugins"][0];
    let member = record["manifest"]["files"][0]["path"].as_str().unwrap();
    let installed = data
        .path()
        .join("plugins/installed")
        .join(ID)
        .join(record["packageSha256"].as_str().unwrap())
        .join(member);
    fs::OpenOptions::new()
        .append(true)
        .open(installed)
        .unwrap()
        .write_all(b"tampered")
        .unwrap();
    assert_eq!(core.set_enabled(true)["error"]["code"], "package_changed");
    assert!(!data.path().join("plugins/data").join(ID).exists());
    core.close();
}

#[test]
fn ignored_shutdown_is_bounded_and_wrong_hello_version_never_enables() {
    for mode in ["ignore_shutdown", "bad_version"] {
        let data = TempDir::new().unwrap();
        let package_dir = TempDir::new().unwrap();
        let mut core = Core::start(data.path());
        core.install(&fixture(package_dir.path(), mode));
        let enabled = core.set_enabled(true);
        if mode == "bad_version" {
            assert_eq!(enabled["ok"], false);
            core.wait_state("failed");
        } else {
            assert_eq!(enabled["ok"], true);
            let start = Instant::now();
            assert_eq!(core.set_enabled(false)["ok"], true);
            assert!(start.elapsed() < Duration::from_secs(4));
        }
        assert_stopped(pid(data.path()));
        core.close();
    }
}

#[test]
fn unanswered_catalog_request_hits_deadline_and_reaps_child() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "hang_catalog"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    let start = Instant::now();
    let reply = core.call("sources.catalog.page", json!({"sourceId":ID}));
    assert_eq!(reply["error"]["code"], "plugin_timeout");
    assert!(start.elapsed() < Duration::from_secs(13));
    core.wait_state("failed");
    assert_stopped(process);
    core.close();
}

#[test]
fn an_unregistered_partial_install_is_replaced_without_inheriting_data_or_execution() {
    fn copy_tree(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let data = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    let inspected = core.ok(
        "plugins.install.inspect",
        json!({"path":example_package().to_str().unwrap()}),
    );
    let token = inspected["inspection"]["token"].as_str().unwrap();
    let digest = inspected["inspection"]["packageSha256"].as_str().unwrap();
    copy_tree(
        &data.path().join("plugins/staging").join(token),
        &data.path().join("plugins/installed").join(ID).join(digest),
    );
    let plugin_data = data.path().join("plugins/data").join(ID);
    fs::create_dir_all(&plugin_data).unwrap();
    fs::write(plugin_data.join("untrusted-old-state"), "must not survive").unwrap();
    core.ok(
        "plugins.install.commit",
        json!({"token":token,"expectedGeneration":inspected["generation"],"consent":true}),
    );
    core.wait_state("disabled");
    assert!(!plugin_data.exists());
    assert_eq!(core.set_enabled(true)["ok"], true);
    assert!(!plugin_data.join("untrusted-old-state").exists());
    core.close();
}

#[cfg(unix)]
#[test]
fn disable_terminates_ordinary_descendants_holding_the_pipes() {
    let data = TempDir::new().unwrap();
    let package_dir = TempDir::new().unwrap();
    let mut core = Core::start(data.path());
    core.install(&fixture(package_dir.path(), "descendant"));
    assert_eq!(core.set_enabled(true)["ok"], true);
    let process = pid(data.path());
    let descendant: u32 = fs::read_to_string(
        data.path()
            .join("plugins/data")
            .join(ID)
            .join("descendant-id"),
    )
    .unwrap()
    .parse()
    .unwrap();
    let start = Instant::now();
    assert_eq!(core.set_enabled(false)["ok"], true);
    assert!(start.elapsed() < Duration::from_secs(4));
    assert_stopped(process);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if unsafe { libc::kill(descendant as i32, 0) } != 0 {
            break;
        }
        #[cfg(target_os = "linux")]
        if fs::read_to_string(format!("/proc/{descendant}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, tail)| tail.starts_with('Z'))
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "descendant remains running");
        std::thread::sleep(Duration::from_millis(20));
    }
    core.close();
}
