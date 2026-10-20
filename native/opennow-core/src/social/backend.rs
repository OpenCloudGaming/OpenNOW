use super::{Config, SocialError, Token};
use reqwest::Method;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

const MAXIMUM_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAXIMUM_ERROR_MESSAGE_CHARS: usize = 512;
const WAKE_RETRY_DELAY: Duration = Duration::from_millis(750);

pub(super) struct Backend {
    client: Client,
}

/// A failed backend call. `status` is the HTTP status when the backend answered;
/// `transient` marks failures worth retrying (unreachable, 429, 5xx).
#[derive(Debug)]
pub(super) struct Failure {
    pub status: Option<u16>,
    pub transient: bool,
    pub error: SocialError,
}

impl Failure {
    pub(super) fn waking(&self, method: &Method) -> bool {
        self.error.code == "social_offline"
            && match self.status {
                Some(status) => matches!(status, 502 | 503),
                None => *method == Method::GET,
            }
    }
}

pub(super) struct Call<'a> {
    pub method: Method,
    pub path: &'a [&'a str],
    pub query: Option<(&'a str, &'a str)>,
    pub body: Option<Value>,
}

impl<'a> Call<'a> {
    pub fn get(path: &'a [&'a str]) -> Self {
        Self::new(Method::GET, path, None)
    }

    pub fn post(path: &'a [&'a str], body: Value) -> Self {
        Self::new(Method::POST, path, Some(body))
    }

    pub fn delete(path: &'a [&'a str]) -> Self {
        Self::new(Method::DELETE, path, None)
    }

    pub fn new(method: Method, path: &'a [&'a str], body: Option<Value>) -> Self {
        Self {
            method,
            path,
            query: None,
            body,
        }
    }
}

impl Backend {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client })
    }

    /// Retries once when the backend is waking from Railway's app sleep: the
    /// edge answers 502/503 without the backend's error envelope, so the app
    /// never saw the request. A failure with no response at all is retried
    /// only for GET, because a POST may already have been handled.
    pub fn send(
        &self,
        config: &Config,
        call: Call<'_>,
        token: Option<&Token>,
    ) -> Result<Value, Failure> {
        match self.send_once(config, &call, token) {
            Err(failure) if failure.waking(&call.method) => {
                std::thread::sleep(WAKE_RETRY_DELAY);
                self.send_once(config, &call, token)
            }
            result => result,
        }
    }

    fn send_once(
        &self,
        config: &Config,
        call: &Call<'_>,
        token: Option<&Token>,
    ) -> Result<Value, Failure> {
        let mut url = config.base.clone();
        url.path_segments_mut()
            .map_err(|()| invalid_response("The social service address is invalid"))?
            .clear()
            .extend(call.path);
        if let Some((key, value)) = call.query {
            url.query_pairs_mut().append_pair(key, value);
        }
        let client =
            crate::proxy::client_for_settings(&self.client, &config.proxy).map_err(|message| {
                Failure {
                    status: None,
                    transient: false,
                    error: SocialError::new("social_offline", message),
                }
            })?;
        let mut request = client
            .request(call.method.clone(), url)
            .header(
                "User-Agent",
                format!("opennow/{}", crate::version::APPLICATION_VERSION),
            )
            .header("Accept", "application/json");
        if let Some(token) = token {
            request = request.bearer_auth(token.expose());
        }
        if let Some(body) = &call.body {
            request = request.json(body);
        }
        let response = request.send().map_err(|_| offline(None))?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        response
            .take(MAXIMUM_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| offline(Some(status)))?;
        if bytes.len() as u64 > MAXIMUM_RESPONSE_BYTES {
            return Err(invalid_response(
                "The social service response exceeded the size limit",
            ));
        }
        if (200..300).contains(&status) {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                return Ok(json!({}));
            }
            return match serde_json::from_slice::<Value>(&bytes) {
                Ok(value @ Value::Object(_)) => Ok(value),
                _ => Err(invalid_response("The social service returned invalid JSON")),
            };
        }
        Err(error_from_status(status, &bytes))
    }
}

fn error_from_status(status: u16, bytes: &[u8]) -> Failure {
    let transient = status == 429 || status >= 500;
    let envelope = serde_json::from_slice::<Value>(bytes).ok();
    let code = envelope
        .as_ref()
        .and_then(|value| value["error"]["code"].as_str())
        .filter(|code| valid_code(code));
    let Some(code) = code else {
        if transient {
            return offline(Some(status));
        }
        return Failure {
            status: Some(status),
            transient: false,
            error: SocialError::new(
                "invalid_upstream_response",
                format!("The social service returned an unexpected response ({status})"),
            ),
        };
    };
    let message = envelope
        .as_ref()
        .and_then(|value| value["error"]["message"].as_str())
        .unwrap_or("The social service rejected the request")
        .chars()
        .take(MAXIMUM_ERROR_MESSAGE_CHARS)
        .collect::<String>();
    Failure {
        status: Some(status),
        transient,
        error: SocialError::new(code, format!("{message} ({status})")),
    }
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn offline(status: Option<u16>) -> Failure {
    let message = match status {
        Some(status) => format!("The OpenNOW social service is unavailable right now ({status})"),
        None => "The OpenNOW social service is unreachable right now".to_owned(),
    };
    Failure {
        status,
        transient: true,
        error: SocialError::new("social_offline", message),
    }
}

fn invalid_response(message: &str) -> Failure {
    Failure {
        status: None,
        transient: false,
        error: SocialError::new("invalid_upstream_response", message),
    }
}
