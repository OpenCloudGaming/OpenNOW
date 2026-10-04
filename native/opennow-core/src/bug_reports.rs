use serde_json::{Map, Value, json};
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ENABLED_WHEN_UNSET: bool = true;
const MAXIMUM_REPORTS_PER_RUN: usize = 5;
const MINIMUM_REPORT_INTERVAL: Duration = Duration::from_secs(30);
const RECENT_GAME_LIMIT: usize = 5;

const REPORTED_METHODS: &[(&str, Kind)] = &[
    ("session.create", Kind::SessionError),
    ("session.poll", Kind::SessionError),
    ("session.claim", Kind::SessionError),
    ("streamer.prepare", Kind::StreamError),
    ("streamer.start", Kind::StreamError),
    ("catalog.library.list", Kind::LibraryError),
];

const EXPECTED_CODES: &[&str] = &[
    "authentication_required",
    "busy",
    "cancelled",
    "catalog_changed",
    "catalog_mutation_busy",
    "confirmation_required",
    "launch_not_ready",
    "rate_limited",
    "routing_busy",
    "session_cleanup_pending",
    "session_conflict",
    "session_update_busy",
    "stale_account",
    "update_pending",
];

const SHELL_METRICS: &[&str] = &[
    "droppedFrames",
    "windowSeconds",
    "framesPerSecond",
    "packetLossPercent",
    "pingMs",
    "decodeTimeMs",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    SessionError,
    StreamError,
    FrameDrops,
    LibraryError,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionError => "session_error",
            Self::StreamError => "stream_error",
            Self::FrameDrops => "frame_drops",
            Self::LibraryError => "library_error",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SessionError => "Session error",
            Self::StreamError => "Stream error",
            Self::FrameDrops => "Repeated frame drops",
            Self::LibraryError => "Library failed to load",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Incident {
    pub kind: Kind,
    pub code: String,
    pub message: String,
    pub details: Value,
}

#[derive(Clone, Debug)]
struct Game {
    title: String,
    app_id: String,
    launched_at_ms: u128,
}

impl Game {
    fn value(&self) -> Value {
        json!({"title":self.title, "appId":self.app_id, "launchedAtMs":self.launched_at_ms.to_string()})
    }
}

#[derive(Default)]
struct State {
    current: Option<Game>,
    recent: VecDeque<Game>,
    reported: HashSet<String>,
    admitted: usize,
    last_admitted: Option<Instant>,
}

#[derive(Default)]
pub struct BugReporter {
    state: Mutex<State>,
}

pub fn enabled(settings: &Value) -> bool {
    match settings["automaticBugReports"].as_str() {
        Some("enabled") => true,
        Some("disabled") => false,
        _ => ENABLED_WHEN_UNSET,
    }
}

impl BugReporter {
    pub fn observe_launch(&self, params: &Value, now_ms: u128) -> String {
        let game = Game {
            title: bounded_text(params["title"].as_str().unwrap_or("Unknown game"), 120),
            app_id: bounded_text(params["appId"].as_str().unwrap_or_default(), 32),
            launched_at_ms: now_ms,
        };
        let detail = format!("title={} appId={}", game.title, game.app_id);
        let mut state = self.state.lock().expect("bug reporter poisoned");
        state.recent.retain(|recent| recent.app_id != game.app_id);
        state.recent.push_front(game.clone());
        state.recent.truncate(RECENT_GAME_LIMIT);
        state.current = Some(game);
        detail
    }

    pub fn observe_stop(&self) {
        self.state.lock().expect("bug reporter poisoned").current = None;
    }

    pub fn current_game_title(&self) -> Option<String> {
        let state = self.state.lock().expect("bug reporter poisoned");
        state.current.as_ref().map(|game| game.title.clone())
    }

    pub fn rpc_incident(method: &str, code: &str, message: &str) -> Option<Incident> {
        let kind = REPORTED_METHODS
            .iter()
            .find(|(reported, _)| *reported == method)
            .map(|(_, kind)| *kind)?;
        if EXPECTED_CODES.contains(&code) {
            return None;
        }
        Some(Incident {
            kind,
            code: bounded_code(code),
            message: crate::diagnostics::runtime_failure_reason(message),
            details: json!({"method":method}),
        })
    }

    pub fn shell_incident(params: &Value) -> Result<Incident, String> {
        let kind = match params["kind"].as_str() {
            Some("stream_error") => Kind::StreamError,
            Some("frame_drops") => Kind::FrameDrops,
            _ => return Err("Unsupported automatic report kind".to_owned()),
        };
        let code = params["code"]
            .as_str()
            .filter(|code| {
                (1..=64).contains(&code.len())
                    && code.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
            .ok_or_else(|| "Automatic report code is invalid".to_owned())?;
        let mut metrics = Map::new();
        for key in SHELL_METRICS {
            if let Some(value) = params["metrics"][*key]
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1_000_000.0).contains(value))
            {
                metrics.insert((*key).to_owned(), json!(value));
            }
        }
        Ok(Incident {
            kind,
            code: code.to_owned(),
            message: crate::diagnostics::runtime_failure_reason(
                params["message"].as_str().unwrap_or_default(),
            ),
            details: json!({"source":"shell", "metrics":metrics}),
        })
    }

    pub fn admit(&self, incident: &Incident, now: Instant) -> Option<Value> {
        let mut state = self.state.lock().expect("bug reporter poisoned");
        let signature = format!("{}:{}", incident.kind.as_str(), incident.code);
        if state.reported.contains(&signature)
            || state.admitted >= MAXIMUM_REPORTS_PER_RUN
            || state
                .last_admitted
                .is_some_and(|last| now.duration_since(last) < MINIMUM_REPORT_INTERVAL)
        {
            return None;
        }
        state.reported.insert(signature);
        state.admitted += 1;
        state.last_admitted = Some(now);
        Some(json!({
            "currentGame": state.current.as_ref().map(Game::value),
            "recentGames": state.recent.iter().map(Game::value).collect::<Vec<_>>()
        }))
    }
}

pub fn compose(incident: &Incident, activity: &Value, account: &Value) -> Value {
    let game = activity["currentGame"]["title"].as_str();
    let mut title = format!("[Auto] {}: {}", incident.kind.label(), incident.code);
    if let Some(game) = game {
        title.push_str(&format!(" · {game}"));
    }
    let reporter = account["reporter"].as_str().unwrap_or("signed-out user");
    let provider = account["provider"].as_str().unwrap_or("unknown provider");
    let recent = activity["recentGames"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|game| game["title"].as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = format!(
        "Automatic report generated by OpenNOW.\n\nTrigger: {}\nCode: {}\nMessage: {}\nReporter: {reporter} ({provider})\nGame: {}\nRecent games: {}\n\nRedacted diagnostics logs are attached.",
        incident.kind.label(),
        incident.code,
        if incident.message.is_empty() {
            "none"
        } else {
            &incident.message
        },
        game.unwrap_or("none"),
        if recent.is_empty() { "none" } else { &recent },
    );
    json!({
        "title": bounded_text(&title, 120),
        "description": bounded_text(&description, 12_000),
        "includeDiagnostics": true
    })
}

fn bounded_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' {
                ' '
            } else {
                character
            }
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn bounded_code(value: &str) -> String {
    let code = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .take(64)
        .collect::<String>();
    if code.is_empty() {
        "unknown".to_owned()
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_consent_is_opt_out_and_explicit_choices_win() {
        assert!(enabled(&json!({"automaticBugReports":"unset"})));
        assert!(enabled(&json!({})));
        assert!(enabled(&json!({"automaticBugReports":"enabled"})));
        assert!(!enabled(&json!({"automaticBugReports":"disabled"})));
    }

    #[test]
    fn only_unexpected_failures_of_reported_methods_become_incidents() {
        let incident = BugReporter::rpc_incident("catalog.library.list", "network_error", "boom")
            .expect("library failures are reported");
        assert_eq!(incident.kind, Kind::LibraryError);
        assert!(BugReporter::rpc_incident("session.create", "cancelled", "user").is_none());
        assert!(BugReporter::rpc_incident("settings.set", "invalid_setting", "x").is_none());
        let incident =
            BugReporter::rpc_incident("session.poll", "session_error", "Bearer abc.def failed")
                .unwrap();
        assert!(!incident.message.contains("abc.def"));
    }

    #[test]
    fn shell_incidents_accept_only_known_kinds_codes_and_metrics() {
        let incident = BugReporter::shell_incident(&json!({
            "kind":"frame_drops", "code":"sustained_frame_drops",
            "metrics":{"droppedFrames":412, "windowSeconds":60, "secret":"x", "pingMs":-1}
        }))
        .unwrap();
        assert_eq!(incident.kind, Kind::FrameDrops);
        assert_eq!(
            incident.details["metrics"],
            json!({"droppedFrames":412.0, "windowSeconds":60.0})
        );
        assert!(BugReporter::shell_incident(&json!({"kind":"library_error","code":"x"})).is_err());
        assert!(
            BugReporter::shell_incident(&json!({"kind":"stream_error","code":"Bad Code"})).is_err()
        );
    }

    #[test]
    fn reports_are_deduplicated_spaced_and_capped_per_run() {
        let reporter = BugReporter::default();
        let start = Instant::now();
        let incident = |code: &str| Incident {
            kind: Kind::StreamError,
            code: code.to_owned(),
            message: String::new(),
            details: Value::Null,
        };
        assert!(reporter.admit(&incident("a"), start).is_some());
        assert!(
            reporter
                .admit(&incident("a"), start + Duration::from_secs(60))
                .is_none()
        );
        assert!(
            reporter
                .admit(&incident("b"), start + Duration::from_secs(5))
                .is_none()
        );
        for (index, code) in ["b", "c", "d", "e"].iter().enumerate() {
            let at = start + MINIMUM_REPORT_INTERVAL * (index as u32 + 1);
            assert!(reporter.admit(&incident(code), at).is_some());
        }
        assert!(
            reporter
                .admit(&incident("f"), start + Duration::from_secs(3600))
                .is_none()
        );
    }

    #[test]
    fn reports_name_the_game_reporter_and_recent_games() {
        let reporter = BugReporter::default();
        reporter.observe_launch(&json!({"title":"Portal 2", "appId":"100"}), 1);
        reporter.observe_launch(&json!({"title":"Cyberpunk 2077", "appId":"200"}), 2);
        reporter.observe_launch(&json!({"title":"Portal 2", "appId":"100"}), 3);
        let incident =
            BugReporter::rpc_incident("session.create", "session_error", "seat lost").unwrap();
        let activity = reporter.admit(&incident, Instant::now()).unwrap();
        assert_eq!(activity["currentGame"]["title"], "Portal 2");
        assert_eq!(activity["recentGames"].as_array().unwrap().len(), 2);
        let report = compose(
            &incident,
            &activity,
            &json!({"reporter":"Zortos", "provider":"NVIDIA"}),
        );
        assert_eq!(
            report["title"],
            "[Auto] Session error: session_error · Portal 2"
        );
        let description = report["description"].as_str().unwrap();
        assert!(description.contains("Reporter: Zortos (NVIDIA)"));
        assert!(description.contains("Recent games: Portal 2, Cyberpunk 2077"));
        reporter.observe_stop();
        assert!(reporter.current_game_title().is_none());
    }
}
