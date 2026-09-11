use reqwest::blocking::{Client, multipart};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const POSTHOG_TOKEN: &str = "phc_nLL3oLahaD3KbFGokZ6CZgMWjLano5nJfnXM4ghCVRVw";
const POSTHOG_CAPTURE: &str = "https://eu.i.posthog.com/capture/";
const BUG_REPORT_ENDPOINT: &str =
    "https://api.printedwaste.com/releases/opennow-desktop/bug-reports";
const MAX_REPORT_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub struct TelemetryService {
    client: Client,
    app_opened_sent: AtomicBool,
}

impl TelemetryService {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())?,
            app_opened_sent: AtomicBool::new(false),
        })
    }

    pub fn sync(&self, consent: &str, install_id: &str) -> Result<Value, String> {
        if consent != "granted" {
            return Ok(json!({"enabled":false,"sent":false}));
        }
        if !valid_install_id(install_id) {
            return Err("Anonymous installation identity is invalid".to_owned());
        }
        if self.app_opened_sent.swap(true, Ordering::AcqRel) {
            return Ok(json!({"enabled":true,"sent":false,"alreadySent":true}));
        }
        if let Err(error) = self.capture("app_opened", install_id, json!({"process":"core"})) {
            self.app_opened_sent.store(false, Ordering::Release);
            return Err(error);
        }
        Ok(json!({"enabled":true,"sent":true}))
    }

    pub fn feedback(&self, install_id: &str, params: &Value) -> Result<Value, String> {
        if !valid_install_id(install_id) {
            return Err("Anonymous installation identity is invalid".to_owned());
        }
        let category = params["category"].as_str().unwrap_or("other");
        if !matches!(category, "bug" | "idea" | "other") {
            return Err("Unsupported feedback category".to_owned());
        }
        let message = params["message"]
            .as_str()
            .map(str::trim)
            .filter(|value| (8..=4_000).contains(&value.chars().count()))
            .ok_or_else(|| "Feedback must be between 8 and 4,000 characters".to_owned())?;
        self.capture(
            "feedback_submitted",
            install_id,
            json!({
                "category": category,
                "message": message,
                "include_system_info": params["includeSystemInfo"].as_bool().unwrap_or(false),
                "source": "qt-shell"
            }),
        )?;
        Ok(json!({"submitted":true,"message":"Thanks — your feedback was sent."}))
    }

    pub fn bug_report(
        &self,
        install_id: &str,
        params: &Value,
        diagnostic_path: Option<&Path>,
    ) -> Result<Value, String> {
        if !valid_install_id(install_id) {
            return Err("Anonymous installation identity is invalid".to_owned());
        }
        let title = bounded_required(&params["title"], 8, 120, "title")?;
        let description = bounded_required(&params["description"], 40, 12_000, "description")?;
        let reporter_id = reporter_id(install_id);
        let metadata = json!({
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "coreVersion": crate::version::APPLICATION_VERSION,
            "source": "qt-shell"
        });
        let mut form = multipart::Form::new()
            .text("title", title)
            .text("description", description)
            .text(
                "versionName",
                crate::version::APPLICATION_VERSION.to_owned(),
            )
            .text(
                "versionCode",
                crate::version::APPLICATION_VERSION.to_owned(),
            )
            .text("platform", "desktop")
            .text("reporterId", reporter_id)
            .text("metadata", metadata.to_string());
        if params["includeDiagnostics"].as_bool() == Some(true)
            && let Some(path) = diagnostic_path
            && path
                .metadata()
                .is_ok_and(|value| value.len() <= MAX_REPORT_FILE_BYTES)
        {
            let bytes =
                fs::read(path).map_err(|error| format!("Could not read diagnostics: {error}"))?;
            form = form.part(
                "files",
                multipart::Part::bytes(bytes)
                    .file_name("opennow-diagnostics.txt")
                    .mime_str("text/plain")
                    .map_err(|error| error.to_string())?,
            );
        }
        let response = self
            .client
            .post(BUG_REPORT_ENDPOINT)
            .multipart(form)
            .send()
            .map_err(|error| network_error(&error.to_string(), "Bug report upload failed"))?;
        let status = response.status();
        let body = response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(server_error(&body, status.as_u16()));
        }
        let reference = serde_json::from_str::<Value>(&body).ok().and_then(|value| {
            ["id", "reportId", "bugReportId"]
                .iter()
                .find_map(|key| value[*key].as_str().map(ToOwned::to_owned))
        });
        Ok(json!({"submitted":true,"reference":reference}))
    }

    fn capture(&self, event: &str, install_id: &str, properties: Value) -> Result<(), String> {
        let response = self
            .client
            .post(POSTHOG_CAPTURE)
            .json(&json!({
                "api_key": POSTHOG_TOKEN,
                "event": event,
                "properties": {
                    "distinct_id": install_id,
                    "app_version": crate::version::APPLICATION_VERSION,
                    "platform": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                    "payload": properties
                }
            }))
            .send()
            .map_err(|error| network_error(&error.to_string(), "Feedback could not be sent"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Feedback service returned HTTP {}",
                response.status()
            ));
        }
        Ok(())
    }
}

pub fn valid_install_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|value| value.is_ascii_hexdigit())
}

fn reporter_id(install_id: &str) -> String {
    let digest = Sha256::digest(format!("opennow-desktop-bug-report-v1:{install_id}"));
    format!("br1_{digest:x}")
}

fn bounded_required(
    value: &Value,
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<String, String> {
    let value = value.as_str().map(str::trim).unwrap_or_default();
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(format!(
            "Bug report {label} must be between {minimum} and {maximum} characters"
        ));
    }
    Ok(value.to_owned())
}

fn network_error(value: &str, fallback: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("connect") || lower.contains("dns") {
        "The reporting service is unavailable right now.".to_owned()
    } else {
        fallback.to_owned()
    }
}

fn server_error(body: &str, status: u16) -> String {
    let custom = serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value["error"]["message"]
            .as_str()
            .or_else(|| value["message"].as_str())
            .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
    });
    custom
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(320).collect())
        .unwrap_or_else(|| match status {
            403 => "Bug reporting is unavailable for this installation.".to_owned(),
            429 => "Too many bug reports were sent. Try again later.".to_owned(),
            _ => format!("Bug report upload failed (HTTP {status})."),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporter_identity_is_stable_and_not_the_install_id() {
        let id = "0123456789abcdef0123456789abcdef";
        assert!(valid_install_id(id));
        let reporter = reporter_id(id);
        assert!(reporter.starts_with("br1_"));
        assert_eq!(reporter.len(), 68);
        assert!(!reporter.contains(id));
    }

    #[test]
    fn feedback_and_reports_are_bounded() {
        assert!(bounded_required(&json!("short"), 8, 120, "title").is_err());
        assert!(bounded_required(&json!("A valid title"), 8, 120, "title").is_ok());
        assert_eq!(
            server_error("{\"error\":{\"message\":\"  Try later  \"}}", 500),
            "Try later"
        );
    }
}
