use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    _directory: TempDir,
}

impl Session {
    fn start(environment: &[(&str, &str)]) -> Self {
        let directory = TempDir::new().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
        command
            .arg("--data-dir")
            .arg(directory.path())
            .env_remove("OPENNOW_PICTURES_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            _directory: directory,
        }
    }

    fn request(&mut self, id: &str, method: &str, params: Value) -> Value {
        writeln!(
            self.stdin,
            "{}",
            json!({"type":"request","id":id,"method":method,"params":params})
        )
        .unwrap();
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(10))
                .expect("core response timed out");
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["type"] == "response" && value["id"] == id {
                return value;
            }
        }
    }

    fn handshake(&mut self) {
        let response = self.request(
            "hello",
            "core.hello",
            json!({"protocolVersion":5,"shell":"qt"}),
        );
        assert_eq!(response["ok"], true);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn empty_pictures_override_keeps_unrelated_startup_working_while_media_is_unavailable() {
    let mut session = Session::start(&[("OPENNOW_PICTURES_DIR", "")]);
    session.handshake();
    let media = session.request("media", "media.root.get", json!({}));
    assert_eq!(media["ok"], false);
    assert_eq!(media["error"]["code"], "media_unavailable");
    let unrelated = session.request("thanks", "thanks.data.get", json!({}));
    assert_eq!(unrelated["ok"], true);
}

#[test]
fn explicit_pictures_override_is_published_by_the_running_core() {
    let directory = TempDir::new().unwrap();
    {
        let mut session =
            Session::start(&[("OPENNOW_PICTURES_DIR", directory.path().to_str().unwrap())]);
        session.handshake();
        let media = session.request("media", "media.root.get", json!({}));
        assert_eq!(media["ok"], true);
        assert_eq!(
            media["result"]["path"],
            directory.path().join("OpenNOW").to_string_lossy().as_ref()
        );
    }
}

#[test]
fn absent_pictures_override_keeps_the_platform_fallback() {
    let home = TempDir::new().unwrap();
    {
        let mut session = Session::start(&[
            ("HOME", home.path().to_str().unwrap()),
            ("USERPROFILE", home.path().to_str().unwrap()),
        ]);
        session.handshake();
        let media = session.request("media", "media.root.get", json!({}));
        assert_eq!(media["ok"], true);
        assert_eq!(
            media["result"]["path"],
            home.path()
                .join("Pictures")
                .join("OpenNOW")
                .to_string_lossy()
                .as_ref()
        );
    }
}
