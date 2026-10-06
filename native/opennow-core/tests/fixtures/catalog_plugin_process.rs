use std::io::{self, BufRead, Write};
use std::time::Duration;

fn main() {
    if std::env::args().any(|arg| arg == "--descendant") {
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    let mode = option_env!("PLUGIN_FIXTURE_MODE").unwrap_or("valid");
    let plugin_id = option_env!("PLUGIN_FIXTURE_ID").unwrap_or("org.opennow.example.catalog");
    let data = std::path::PathBuf::from(std::env::var_os("OPENNOW_PLUGIN_DATA_DIR").unwrap());
    std::fs::write(data.join("last-process-id"), std::process::id().to_string()).unwrap();
    if mode == "descendant" {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--descendant")
            .spawn()
            .unwrap();
        std::fs::write(data.join("descendant-id"), child.id().to_string()).unwrap();
    }
    let mut pending: Option<(u64, String)> = None;
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let id = line
            .split("\"id\":\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let epoch: u64 = line
            .split("\"epoch\":")
            .nth(1)
            .unwrap()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap();
        if line.contains("\"type\":\"cancel\"") {
            if pending
                .as_ref()
                .is_some_and(|(e, request)| *e == epoch && request == id)
            {
                pending = None;
                println!(
                    "{{\"v\":1,\"type\":\"response\",\"epoch\":{epoch},\"id\":\"{id}\",\"ok\":false,\"error\":{{\"code\":\"cancelled\"}}}}"
                );
            }
        } else if line.contains("plugin.hello") {
            if mode == "hang_hello" {
                std::thread::sleep(Duration::from_secs(60));
            }
            let version = if mode == "bad_version" {
                "2.0.0"
            } else {
                "1.0.0"
            };
            println!(
                "{{\"v\":1,\"type\":\"response\",\"epoch\":{epoch},\"id\":\"{id}\",\"ok\":true,\"result\":{{\"pluginId\":\"{plugin_id}\",\"version\":\"{version}\",\"protocolVersion\":1,\"capabilities\":[\"catalog.v1\"]}}}}"
            );
        } else if line.contains("catalog.page") {
            if mode == "cooperative_cancel" && line.contains("\"query\":\"wait\"") {
                pending = Some((epoch, id.to_owned()));
                std::fs::write(data.join("request-waiting"), "true").unwrap();
                continue;
            }
            if mode == "cancel_restart" && line.contains("\"query\":\"wait\"") {
                std::fs::write(data.join("request-waiting"), "true").unwrap();
                std::thread::sleep(Duration::from_secs(60));
            }
            if mode == "request_error" && line.contains("\"query\":\"invalid\"") {
                println!(
                    "{{\"v\":1,\"type\":\"response\",\"epoch\":{epoch},\"id\":\"{id}\",\"ok\":false,\"error\":{{\"code\":\"invalid_request\"}}}}"
                );
                io::stdout().flush().unwrap();
                continue;
            }
            match mode {
                "hang_catalog" => std::thread::sleep(Duration::from_secs(60)),
                "malformed" => {
                    println!("not JSON");
                    continue;
                }
                "oversized" => {
                    io::stdout()
                        .write_all(&vec![b'x'; 1024 * 1024 + 1])
                        .unwrap();
                    io::stdout().flush().unwrap();
                    continue;
                }
                "event" => {
                    println!(
                        "{{\"type\":\"event\",\"name\":\"auth.session.changed\",\"payload\":{{}}}}"
                    );
                    continue;
                }
                "stderr" => {
                    for _ in 0..256 {
                        io::stderr().write_all(&[b'S'; 4096]).unwrap();
                    }
                }
                _ => {}
            }
            let reply_epoch = if mode == "wrong_epoch" {
                epoch + 1
            } else {
                epoch
            };
            let reply_id = if mode == "wrong_id" {
                "unsolicited"
            } else {
                id
            };
            let title = if std::env::var_os("OPENNOW_TEST_PARENT_SECRET").is_some() {
                "leaked"
            } else {
                "scrubbed"
            };
            let reply = format!(
                "{{\"v\":1,\"type\":\"response\",\"epoch\":{reply_epoch},\"id\":\"{reply_id}\",\"ok\":true,\"result\":{{\"items\":[{{\"id\":\"fixture\",\"title\":\"{title}\"}}],\"nextCursor\":null,\"coverage\":\"complete\"}}}}"
            );
            for _ in 0..if mode == "flood" { 256 } else { 1 } {
                println!("{reply}");
            }
        } else if line.contains("plugin.shutdown") {
            if mode == "ignore_shutdown" {
                std::fs::write(data.join("shutdown-waiting"), "true").unwrap();
                std::thread::sleep(Duration::from_secs(60));
            }
            return;
        }
        io::stdout().flush().unwrap();
    }
}
