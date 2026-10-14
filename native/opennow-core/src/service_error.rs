#[derive(Clone, Debug)]
pub struct ServiceError {
    pub code: &'static str,
    pub message: String,
}

impl ServiceError {
    pub(crate) fn network(context: &str, error: reqwest::Error) -> Self {
        let error = error.without_url();
        Self {
            code: "network_error",
            message: match transport_failure(&error) {
                Some(stage) => format!("{context}: {error} ({stage})"),
                None => format!("{context}: {error}"),
            },
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_params",
            message: message.into(),
        }
    }
}

pub(crate) fn transport_failure(error: &reqwest::Error) -> Option<&'static str> {
    if error.is_timeout() {
        return Some("timeout");
    }
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if cause.to_string() == "dns error" {
            return Some("dns");
        }
        if cause.is::<rustls::Error>() {
            return Some("tls");
        }
        source = match cause.downcast_ref::<std::io::Error>() {
            Some(io) => io.get_ref().map(|inner| inner as _),
            None => cause.source(),
        };
    }
    error.is_connect().then_some("connect")
}
