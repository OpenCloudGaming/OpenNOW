use flate2::{Compression, write::GzEncoder};
use reqwest::blocking::{Client, multipart};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_REPORTS_API: &str = "https://opennow-reports-production.up.railway.app";
const MAX_REPORT_JSON_BYTES: usize = 64 * 1024;
const MAX_LOG_TEXT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LOG_GZIP_BYTES: usize = 5 * 1024 * 1024;
const MAX_PENDING_REPORTS: usize = 5;
const MAX_PENDING_AGE_MS: u128 = 7 * 24 * 60 * 60 * 1000;

pub fn api_base_from_env() -> String {
    std::env::var("OPENNOW_REPORTS_API")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| value.starts_with("https://") || value.starts_with("http://"))
        .unwrap_or_else(|| DEFAULT_REPORTS_API.to_owned())
}

pub fn valid_install_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|value| value.is_ascii_hexdigit())
}

pub fn required_text(
    value: &Value,
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<String, String> {
    let value = value.as_str().map(str::trim).unwrap_or_default();
    if !(minimum..=maximum).contains(&value.chars().count()) {
        return Err(format!(
            "{label} must be between {minimum} and {maximum} characters"
        ));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub report_id: String,
    pub issue_id: Option<String>,
    pub issue_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub message: String,
    pub retryable: bool,
}

pub struct Document<'a> {
    pub install_id: &'a str,
    pub account: &'a Value,
    pub activity: Value,
    pub app: Value,
}

impl Document<'_> {
    pub fn automatic(self, trigger: Value) -> Value {
        self.compose(true, trigger)
    }

    pub fn manual(self, title: &str, description: &str) -> Value {
        self.compose(false, json!({"title":title, "description":description}))
    }

    fn compose(self, automatic: bool, detail: Value) -> Value {
        let mut report = json!({
            "schemaVersion": 1,
            "automatic": automatic,
            "installId": self.install_id,
            "account": if self.account.is_object() { self.account.clone() } else { Value::Null },
            "activity": self.activity,
            "app": self.app
        });
        report[if automatic { "trigger" } else { "manual" }] = detail;
        report
    }
}

pub fn gzip_log(path: &Path) -> Option<Vec<u8>> {
    if path
        .metadata()
        .map_or(true, |metadata| metadata.len() > MAX_LOG_TEXT_BYTES)
    {
        return None;
    }
    let text = fs::read(path).ok()?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&text).ok()?;
    encoder
        .finish()
        .ok()
        .filter(|bytes| bytes.len() <= MAX_LOG_GZIP_BYTES)
}

pub struct ReportsClient {
    client: Client,
    endpoint: String,
    pending: PathBuf,
}

impl ReportsClient {
    pub fn new(base: &str, data_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .build()
                .map_err(|error| error.to_string())?,
            endpoint: format!("{base}/v1/reports"),
            pending: data_dir.join("reports").join("pending"),
        })
    }

    pub fn submit(&self, report: &Value, log: Option<&[u8]>) -> Result<Accepted, Failure> {
        let body = report.to_string();
        if body.len() > MAX_REPORT_JSON_BYTES {
            return Err(Failure {
                message: "The bug report is too large to send.".to_owned(),
                retryable: false,
            });
        }
        let mut form = multipart::Form::new().part(
            "report",
            multipart::Part::text(body)
                .mime_str("application/json")
                .map_err(|error| Failure {
                    message: error.to_string(),
                    retryable: false,
                })?,
        );
        if let Some(log) = log {
            form = form.part(
                "log",
                multipart::Part::bytes(log.to_vec())
                    .file_name("opennow-diagnostics.log.gz")
                    .mime_str("application/gzip")
                    .map_err(|error| Failure {
                        message: error.to_string(),
                        retryable: false,
                    })?,
            );
        }
        let response = self
            .client
            .post(&self.endpoint)
            .multipart(form)
            .send()
            .map_err(|_| Failure {
                message: "The reporting service is unavailable right now.".to_owned(),
                retryable: true,
            })?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .ok()
            .and_then(|body| serde_json::from_str::<Value>(&body).ok())
            .unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            let text = |key: &str| body[key].as_str().map(|value| bounded(value, 64));
            return Ok(Accepted {
                report_id: text("reportId").unwrap_or_default(),
                issue_id: text("issueId"),
                issue_status: text("issueStatus"),
            });
        }
        Err(Failure {
            message: body["error"]["message"]
                .as_str()
                .map(|message| bounded(message, 320))
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| match status {
                    429 => "Too many bug reports were sent. Try again later.".to_owned(),
                    _ => format!("Bug report upload failed (HTTP {status})."),
                }),
            retryable: body["error"]["retryable"]
                .as_bool()
                .unwrap_or(status == 408 || status == 429 || status >= 500),
        })
    }

    pub fn spool(&self, report: &Value, log: Option<&[u8]>, now_ms: u128) -> std::io::Result<()> {
        fs::create_dir_all(&self.pending)?;
        let name = format!("{now_ms:013}-{:08x}", rand::random::<u32>());
        if let Some(log) = log {
            fs::write(self.pending.join(format!("{name}.log.gz")), log)?;
        }
        let path = self.pending.join(format!("{name}.json"));
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, report.to_string())?;
        fs::rename(&temporary, &path)?;
        self.prune(now_ms);
        Ok(())
    }

    pub fn clear_pending(&self) {
        let _ = fs::remove_dir_all(&self.pending);
    }

    pub fn retry_pending(&self, now_ms: u128) -> Vec<(Value, Accepted)> {
        let mut sent = Vec::new();
        for name in self.prune(now_ms) {
            let path = self.pending.join(format!("{name}.json"));
            let Some(report) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            else {
                self.remove(&name);
                continue;
            };
            let log = fs::read(self.pending.join(format!("{name}.log.gz"))).ok();
            match self.submit(&report, log.as_deref()) {
                Ok(accepted) => {
                    self.remove(&name);
                    sent.push((report, accepted));
                }
                Err(failure) if failure.retryable => break,
                Err(_) => self.remove(&name),
            }
        }
        sent
    }

    fn prune(&self, now_ms: u128) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.pending) else {
            return Vec::new();
        };
        let files = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        let mut reports = files
            .iter()
            .filter_map(|file| file.strip_suffix(".json"))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        reports.sort();
        let fresh = |name: &String| {
            name.split('-')
                .next()
                .and_then(|stamp| stamp.parse::<u128>().ok())
                .is_some_and(|stamp| now_ms.saturating_sub(stamp) <= MAX_PENDING_AGE_MS)
        };
        let excess = reports.len().saturating_sub(MAX_PENDING_REPORTS);
        let (kept, dropped): (Vec<_>, Vec<_>) = reports
            .into_iter()
            .enumerate()
            .partition(|(index, name)| *index >= excess && fresh(name));
        for (_, name) in dropped {
            self.remove(&name);
        }
        let kept = kept.into_iter().map(|(_, name)| name).collect::<Vec<_>>();
        for file in &files {
            let orphan = file
                .strip_suffix(".log.gz")
                .or_else(|| file.strip_suffix(".json.tmp"))
                .is_some_and(|name| !kept.iter().any(|kept| kept == name));
            if orphan {
                let _ = fs::remove_file(self.pending.join(file));
            }
        }
        kept
    }

    fn remove(&self, name: &str) {
        let _ = fs::remove_file(self.pending.join(format!("{name}.json")));
        let _ = fs::remove_file(self.pending.join(format!("{name}.log.gz")));
    }
}

fn bounded(value: &str, limit: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::thread;

    const INSTALL: &str = "0123456789abcdef0123456789abcdef";

    fn server(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let mut bodies = Vec::new();
            for (status, reply) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let mut request = head.into_bytes();
                request.extend_from_slice(b"\r\n");
                request.extend_from_slice(&body);
                bodies.push(request);
                let reply = reply.to_string();
                write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
            }
            bodies
        });
        (base, worker)
    }

    fn report() -> Value {
        Document {
            install_id: INSTALL,
            account: &json!({"userId":"sub-1", "providerIdpId":"idp", "providerCode":"NVIDIA",
                "providerName":"NVIDIA", "alliancePartner":false, "reporter":"Zortos",
                "reporterBasis":"username", "membershipTier":"ULTIMATE"}),
            activity: json!({"currentGame":{"title":"Portal 2","appId":"100"}, "recentGames":[]}),
            app: json!({"version":"1.0.3", "os":"linux", "arch":"x86_64", "gpu":null,
                "decoderBackend":"vaapi", "channel":"stable"}),
        }
        .automatic(
            json!({"kind":"frame_drops", "code":"sustained_frame_drops", "message":"",
            "metrics":{"droppedFrames":412.0}}),
        )
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    #[test]
    fn manual_text_is_trimmed_and_bounded() {
        assert!(required_text(&json!("short"), 8, 120, "Bug report title").is_err());
        assert_eq!(
            required_text(&json!("  A valid title "), 8, 120, "Bug report title").unwrap(),
            "A valid title"
        );
    }

    #[test]
    fn documents_follow_schema_v1_and_carry_only_one_detail_section() {
        let automatic = report();
        assert_eq!(automatic["schemaVersion"], 1);
        assert_eq!(automatic["automatic"], true);
        assert!(automatic.get("manual").is_none());
        assert_eq!(automatic["trigger"]["code"], "sustained_frame_drops");
        let manual = Document {
            install_id: INSTALL,
            account: &Value::Null,
            activity: json!({"currentGame":null, "recentGames":[]}),
            app: json!({}),
        }
        .manual("Feedback: idea", "Add a dark mode please");
        assert_eq!(manual["automatic"], false);
        assert_eq!(manual["account"], Value::Null);
        assert!(manual.get("trigger").is_none());
        assert_eq!(manual["manual"]["title"], "Feedback: idea");
    }

    #[test]
    fn reports_upload_as_multipart_with_json_and_a_gzip_log() {
        let (base, worker) = server(vec![(
            201,
            json!({"reportId":"br-7F3A21", "issueId":"iss_1", "issueStatus":"investigating"}),
        )]);
        let directory = tempfile::tempdir().unwrap();
        let client = ReportsClient::new(&base, directory.path()).unwrap();
        let log_path = directory.path().join("log.txt");
        fs::write(&log_path, "diagnostics line\n".repeat(64)).unwrap();
        let log = gzip_log(&log_path).unwrap();
        let accepted = client.submit(&report(), Some(&log)).unwrap();
        assert_eq!(
            accepted,
            Accepted {
                report_id: "br-7F3A21".to_owned(),
                issue_id: Some("iss_1".to_owned()),
                issue_status: Some("investigating".to_owned()),
            }
        );
        let request = worker.join().unwrap().pop().unwrap();
        let text = String::from_utf8_lossy(&request);
        assert!(text.starts_with("POST /v1/reports HTTP/1.1"));
        assert!(
            text.to_ascii_lowercase()
                .contains("content-type: multipart/form-data; boundary=")
        );
        assert!(text.contains("Content-Disposition: form-data; name=\"report\""));
        assert!(text.contains("Content-Type: application/json"));
        assert!(text.contains(&report().to_string()));
        assert!(text.contains(
            "Content-Disposition: form-data; name=\"log\"; filename=\"opennow-diagnostics.log.gz\""
        ));
        let start = find(&request, &[0x1f, 0x8b]).expect("gzip payload");
        let mut decoded = String::new();
        GzDecoder::new(&request[start..])
            .read_to_string(&mut decoded)
            .unwrap();
        assert!(decoded.starts_with("diagnostics line\n"));
    }

    #[test]
    fn server_errors_report_their_retryability() {
        let (base, worker) = server(vec![
            (
                400,
                json!({"error":{"code":"schema","message":"  bad   schema ","retryable":false}}),
            ),
            (503, Value::Null),
            (
                429,
                json!({"error":{"code":"rate_limited","message":"Slow down","retryable":true}}),
            ),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let client = ReportsClient::new(&base, directory.path()).unwrap();
        assert_eq!(
            client.submit(&report(), None).unwrap_err(),
            Failure {
                message: "bad schema".to_owned(),
                retryable: false
            }
        );
        assert!(client.submit(&report(), None).unwrap_err().retryable);
        assert!(client.submit(&report(), None).unwrap_err().retryable);
        worker.join().unwrap();
        let unreachable = ReportsClient::new("http://127.0.0.1:9", directory.path()).unwrap();
        assert!(unreachable.submit(&report(), None).unwrap_err().retryable);
    }

    #[test]
    fn pending_reports_keep_the_newest_five_within_seven_days() {
        let directory = tempfile::tempdir().unwrap();
        let client = ReportsClient::new("http://127.0.0.1:9", directory.path()).unwrap();
        let day = 24 * 60 * 60 * 1000;
        let now = 100 * day;
        client
            .spool(&json!({"n":0}), Some(b"old"), now - 8 * day)
            .unwrap();
        for index in 1..=6 {
            client
                .spool(&json!({"n":index}), None, now - 6 * day + index)
                .unwrap();
        }
        let kept = client.prune(now);
        assert_eq!(kept.len(), 5);
        let numbers = kept
            .iter()
            .map(|name| {
                let text = fs::read_to_string(client.pending.join(format!("{name}.json"))).unwrap();
                serde_json::from_str::<Value>(&text).unwrap()["n"]
                    .as_i64()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(numbers, vec![2, 3, 4, 5, 6]);
        let gz = fs::read_dir(&client.pending)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".log.gz")
            })
            .count();
        assert_eq!(gz, 0);
        assert!(client.prune(now + 8 * day).is_empty());
    }

    #[test]
    fn pending_reports_retry_until_the_service_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let (base, worker) = server(vec![
            (201, json!({"reportId":"br-000001"})),
            (
                400,
                json!({"error":{"code":"schema","message":"bad","retryable":false}}),
            ),
            (503, Value::Null),
        ]);
        let client = ReportsClient::new(&base, directory.path()).unwrap();
        let now = 10_000_000;
        for index in 0..4 {
            client
                .spool(&json!({"n":index}), Some(b"log"), now + index)
                .unwrap();
        }
        let sent = client.retry_pending(now + 10);
        worker.join().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0["n"], 0);
        assert_eq!(sent[0].1.report_id, "br-000001");
        let remaining = client.prune(now + 10);
        assert_eq!(remaining.len(), 2);
        assert!(
            client
                .pending
                .join(format!("{}.log.gz", remaining[0]))
                .is_file()
        );
    }
}
