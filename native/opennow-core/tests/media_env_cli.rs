use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl Session {
    fn start(environment: &[(&str, &str)]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("opennow-media-env-{unique}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
        command
            .arg("--data-dir")
            .arg(&directory)
            .env_remove("OPENNOW_PICTURES_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
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
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-media-root-{unique}"));
    {
        let mut session = Session::start(&[("OPENNOW_PICTURES_DIR", directory.to_str().unwrap())]);
        session.handshake();
        let media = session.request("media", "media.root.get", json!({}));
        assert_eq!(media["ok"], true);
        assert_eq!(
            media["result"]["path"],
            directory.join("OpenNOW").to_string_lossy().as_ref()
        );
    }
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn absent_pictures_override_keeps_the_platform_fallback() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("opennow-media-home-{unique}"));
    {
        let mut session = Session::start(&[
            ("HOME", home.to_str().unwrap()),
            ("USERPROFILE", home.to_str().unwrap()),
        ]);
        session.handshake();
        let media = session.request("media", "media.root.get", json!({}));
        assert_eq!(media["ok"], true);
        assert_eq!(
            media["result"]["path"],
            home.join("Pictures")
                .join("OpenNOW")
                .to_string_lossy()
                .as_ref()
        );
    }
    let _ = std::fs::remove_dir_all(home);
}
