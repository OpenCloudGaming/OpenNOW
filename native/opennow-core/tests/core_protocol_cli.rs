use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn hello(version: u32) -> Value {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-protocol-{version}-{unique}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .arg("--data-dir")
        .arg(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        json!({
            "type":"request","id":"hello","method":"core.hello",
            "params":{"protocolVersion":version,"shell":"qt"}
        })
    )
    .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        sender.send(result).unwrap();
    });
    let response = receiver.recv_timeout(Duration::from_secs(10));
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    if directory.exists() {
        std::fs::remove_dir_all(directory).unwrap();
    }
    serde_json::from_str(&response.expect("core handshake timed out").unwrap()).unwrap()
}

#[test]
fn protocol_three_shells_are_rejected_before_the_paged_library_contract() {
    let response = hello(3);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "incompatible_protocol");
    assert!(response.get("result").is_none());
}

#[test]
fn protocol_four_shells_receive_the_paged_library_capabilities() {
    let response = hello(4);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "incompatible_protocol");
}

#[test]
fn protocol_five_shells_receive_the_paged_library_capabilities() {
    let response = hello(5);
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["protocolVersion"], 5);
    let capabilities = response["result"]["capabilities"].as_array().unwrap();
    for capability in [
        "catalog.libraryPages.v1",
        "catalog.metadata.v1",
        "account.syncObservation.v1",
        "account.pushInvalidation.v1",
        "catalog.languages.v1",
        "queue.servers.v1",
        "auth.accountPicker.v1",
        "session.terminalCleanup.v1",
    ] {
        assert!(capabilities.contains(&json!(capability)));
    }
}

#[test]
fn writable_cores_exclusively_own_the_resolved_profile_until_exit() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-profile-lock-{unique}"));
    let other_directory = directory.join("other-profile");
    let mut first = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .args(["--data-dir", directory.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(first.stdin.as_mut().unwrap(), "{}", json!({
        "type":"request","id":"owner","method":"core.hello","params":{"protocolVersion":5,"shell":"qt"}
    })).unwrap();
    let stdout = first.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let _ = sender.send(BufReader::new(stdout).read_line(&mut line).map(|_| line));
    });
    let ready = receiver.recv_timeout(Duration::from_secs(10));
    let launch = |path: &std::path::Path, graphics: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
        command.arg("--data-dir").arg(path).stdin(Stdio::null());
        if graphics {
            command.arg("--graphics-preferences");
        }
        command.output().unwrap()
    };
    let second = launch(&directory, false);
    let independent = launch(&other_directory, false);
    let read_only = launch(&directory, true);
    first.kill().unwrap();
    first.wait().unwrap();
    reader.join().unwrap();
    let recovered = launch(&directory, false);
    std::fs::remove_dir_all(&directory).unwrap();
    let hello: Value =
        serde_json::from_str(&ready.expect("owner handshake timed out").unwrap()).unwrap();
    assert_eq!(hello["ok"], true);
    assert!(
        !second.status.success(),
        "a second writable core admitted the same profile"
    );
    assert!(String::from_utf8_lossy(&second.stderr).contains("data directory is already in use"));
    assert!(
        independent.status.success(),
        "a distinct profile was blocked"
    );
    assert!(
        read_only.status.success(),
        "read-only graphics preferences were blocked"
    );
    assert!(
        recovered.status.success(),
        "process exit left a stale profile lock"
    );
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod pending_profile_owner {
    use super::*;
    use fs2::FileExt;
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::process::Child;
    use std::thread::{self, JoinHandle};
    use std::time::Instant;

    struct ReapedChild {
        child: Child,
        reader: Option<JoinHandle<()>>,
    }

    impl Drop for ReapedChild {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }

    fn pipe() -> (File, File) {
        let mut descriptors = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        unsafe {
            (
                File::from_raw_fd(descriptors[0]),
                File::from_raw_fd(descriptors[1]),
            )
        }
    }

    fn signal(reader: &mut File, expected: u8) {
        let mut descriptor = libc::pollfd {
            fd: reader.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { libc::poll(&mut descriptor, 1, 10_000) },
            1,
            "fixture signal timed out"
        );
        let mut byte = [0];
        reader.read_exact(&mut byte).unwrap();
        assert_eq!(byte[0], expected);
    }

    fn response(receiver: &mpsc::Receiver<io::Result<String>>, id: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let line = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("core response timed out")
                .unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["id"] == id {
                return value;
            }
        }
    }

    #[test]
    fn pending_mutable_worker_keeps_profile_locked_after_main_loop_error() {
        let directory = tempfile::tempdir().unwrap();
        let fixture = directory.path().join("profile-fsync-gate.so");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/profile_fsync_gate.c");
        let compiled = Command::new("cc")
            .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"])
            .arg(source)
            .args(["-o"])
            .arg(&fixture)
            .arg("-ldl")
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "fsync fixture compilation failed"
        );
        let armed = directory.path().join("armed");
        let (mut ready_read, ready_write) = pipe();
        let (mut error_read, error_write) = pipe();
        let (release_read, mut release_write) = pipe();
        let (_stderr_read, mut stderr_write) = pipe();
        let inherited = [
            ready_write.as_raw_fd(),
            error_write.as_raw_fd(),
            release_read.as_raw_fd(),
        ];
        let executable = std::env::var_os("OPENNOW_PROFILE_TEST_CORE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_opennow-core").into());
        let mut command = Command::new(&executable);
        command
            .arg("--data-dir")
            .arg(directory.path())
            .env("LD_PRELOAD", fixture)
            .env("PROFILE_FSYNC_ARMED", &armed)
            .env("PROFILE_FSYNC_READY_FD", inherited[0].to_string())
            .env("PROFILE_ERROR_FD", inherited[1].to_string())
            .env("PROFILE_FSYNC_RELEASE_FD", inherited[2].to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_write.try_clone().unwrap()));
        unsafe {
            command.pre_exec(move || {
                for fd in inherited {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let mut first = ReapedChild {
            child: command.spawn().unwrap(),
            reader: None,
        };
        drop((ready_write, error_write, release_read));
        let stdout = first.child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        first.reader = Some(thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        }));
        writeln!(
            first.child.stdin.as_mut().unwrap(),
            "{}",
            json!({
                "type":"request", "id":"owner", "method":"core.hello",
                "params":{"protocolVersion":5,"shell":"qt"}
            })
        )
        .unwrap();
        assert_eq!(response(&receiver, "owner")["ok"], true);

        let fd = stderr_write.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        loop {
            match stderr_write.write(&[b'x'; 4096]) {
                Ok(count) => assert!(count > 0),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("could not fill fixture stderr pipe: {error}"),
            }
        }
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFL, flags) }, 0);
        File::create(&armed).unwrap();
        writeln!(
            first.child.stdin.as_mut().unwrap(),
            "{}",
            json!({
                "type":"request", "id":"width", "method":"settings.set",
                "params":{"key":"windowWidth","value":1234}
            })
        )
        .unwrap();
        signal(&mut ready_read, b's');
        writeln!(first.child.stdin.as_mut().unwrap(), "invalid-json").unwrap();
        signal(&mut error_read, b'e');
        assert!(first.child.try_wait().unwrap().is_none());

        let mut second = ReapedChild {
            child: Command::new(&executable)
                .arg("--data-dir")
                .arg(directory.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
            reader: None,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = second.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "second core did not exit");
            thread::sleep(Duration::from_millis(5));
        };
        let mut error = String::new();
        second
            .child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)
            .unwrap();
        assert!(
            !status.success(),
            "a second writable core admitted a profile while a pending worker survived main-loop failure"
        );
        assert!(error.contains("data directory is already in use"));

        release_write.write_all(b"r").unwrap();
        let saved = response(&receiver, "width");
        assert_eq!(saved["ok"], true);
        assert_eq!(saved["result"]["value"], 1234);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let tasks = std::fs::read_dir(format!("/proc/{}/task", first.child.id())).unwrap();
            let rpc_running = tasks.filter_map(Result::ok).any(|entry| {
                std::fs::read_to_string(entry.path().join("comm"))
                    .is_ok_and(|name| name.starts_with("opennow-rpc-"))
            });
            if !rpc_running {
                break;
            }
            assert!(Instant::now() < deadline, "RPC worker did not finish");
            thread::sleep(Duration::from_millis(5));
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.path().join("core.lock"))
            .unwrap();
        assert!(first.child.try_wait().unwrap().is_none());
        let error = lock
            .try_lock_exclusive()
            .expect_err("live core process released profile ownership after RPC teardown");
        assert_eq!(
            error.raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
        drop(first);
        lock.try_lock_exclusive()
            .expect("terminated core retained profile ownership");
    }
}

#[test]
fn social_v1_answers_through_the_protocol_without_a_linked_account() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-social-{unique}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .arg("--data-dir")
        .arg(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel::<Value>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                break;
            }
        }
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut request = |id: &str, method: &str, params: Value| {
        writeln!(
            stdin,
            "{}",
            json!({"type":"request", "id":id, "method":method, "params":params})
        )
        .unwrap();
        loop {
            let message = lines.recv_timeout(Duration::from_secs(10)).unwrap();
            if message["type"] == "response" && message["id"] == id {
                return message;
            }
        }
    };
    let hello = request(
        "1",
        "core.hello",
        json!({"protocolVersion":5, "shell":"qt"}),
    );
    assert!(
        hello["result"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("social.v1"))
    );
    let state = request("2", "social.state.get", json!({}));
    assert_eq!(
        state["result"],
        json!({"connection":"unlinked", "account":null, "guest":null})
    );
    let friends = request("3", "social.friends.get", json!({}));
    assert_eq!(friends["error"]["code"], "social_not_linked");
    let presence = request(
        "4",
        "social.presence.set",
        json!({"activity":"browsing_store", "game":null, "since":null, "queuePosition":null, "lobbyId":null}),
    );
    assert_eq!(presence["result"], json!({}));
    let insecure = request(
        "5",
        "settings.set",
        json!({"key":"socialBackendUrl", "value":"http://social.example.com"}),
    );
    assert_eq!(insecure["error"]["code"], "invalid_setting");
    let local = request(
        "6",
        "settings.set",
        json!({"key":"socialBackendUrl", "value":"http://LOCALHOST:8787/"}),
    );
    assert_eq!(local["result"]["value"], "http://localhost:8787");
    let settings = request("7", "settings.get", json!({}));
    assert_eq!(
        settings["result"]["settings"]["socialBackendUrl"],
        "http://localhost:8787"
    );
    let reset = request("8", "settings.reset", json!({}));
    assert_eq!(reset["result"]["settings"]["socialBackendUrl"], "");
    drop(stdin);
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn session_stream_reconfigure_is_refused_without_touching_the_session() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-reconfigure-{unique}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .arg("--data-dir")
        .arg(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel::<Value>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                break;
            }
        }
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut request = |id: &str, method: &str, params: Value| {
        writeln!(
            stdin,
            "{}",
            json!({"type":"request", "id":id, "method":method, "params":params})
        )
        .unwrap();
        loop {
            let message = lines.recv_timeout(Duration::from_secs(10)).unwrap();
            if message["type"] == "response" && message["id"] == id {
                return message;
            }
        }
    };
    request(
        "1",
        "core.hello",
        json!({"protocolVersion":5, "shell":"qt"}),
    );
    for codec in ["h264", "h265", "av1"] {
        let response = request(codec, "session.stream.reconfigure", json!({"codec":codec}));
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unsupported");
    }
    let invalid = request("bad", "session.stream.reconfigure", json!({"codec":"vp9"}));
    assert_eq!(invalid["error"]["code"], "invalid_params");
    drop(stdin);
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    let _ = std::fs::remove_dir_all(directory);
}
