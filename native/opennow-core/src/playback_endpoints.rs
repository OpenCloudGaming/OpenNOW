use serde_json::{Value, json};
use std::net::IpAddr;
use url::Url;

fn valid_webrtc_endpoint(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|raw| Url::parse(raw).ok())
        .is_some_and(|url| {
            url.scheme() == "wss"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
}

pub(crate) fn has_webrtc_endpoint(session: &Value) -> bool {
    session["connectionInfo"]
        .as_array()
        .is_some_and(|connections| {
            connections
                .iter()
                .filter_map(webrtc_signaling_endpoint)
                .any(|url| session["signalingUrl"] == url.as_str())
        })
}

pub(crate) fn webrtc_signaling_endpoint(connection: &Value) -> Option<Url> {
    if !matches!(value_i64(&connection["usage"]), Some(14 | 16)) {
        return None;
    }
    let resource = connection["resourcePath"]
        .as_str()
        .filter(|path| !path.is_empty())
        .unwrap_or("/nvst/");
    let rtsp = resource.starts_with("rtsp://") || resource.starts_with("rtsps://");
    let native_descriptor = value_i64(&connection["usage"]) == Some(16)
        || matches!(value_i64(&connection["appLevelProtocol"]), Some(1 | 6));
    let absolute = resource.starts_with("wss://") || resource.starts_with("https://") || rtsp;
    let mut url = if absolute {
        let mut url = if rtsp {
            Url::parse(&format!("wss://{}", resource.split_once("://")?.1)).ok()?
        } else {
            Url::parse(resource).ok()?
        };
        url.set_scheme("wss").ok()?;
        if rtsp {
            url.set_port(None).ok()?;
            url.set_path("/nvst/");
            url.set_query(None);
        }
        url
    } else {
        if resource.contains("://") || !resource.starts_with('/') {
            return None;
        }
        let host = connection["ip"].as_str()?;
        let mut url = Url::parse("wss://endpoint.invalid/").ok()?;
        if let Ok(ip) = host.parse::<IpAddr>() {
            url.set_ip_host(ip).ok()?;
        } else {
            url.set_host(Some(host)).ok()?;
        }
        url.set_path(resource);
        url
    };
    if !absolute && let Some(host) = connection["ip"].as_str().filter(|host| !host.is_empty()) {
        if let Ok(ip) = host.parse::<IpAddr>() {
            if ip.is_unspecified() {
                return None;
            }
            url.set_ip_host(ip).ok()?;
        } else {
            url.set_host(Some(host)).ok()?;
        }
    }
    if !absolute
        && !native_descriptor
        && let Some(port) = value_i64(&connection["port"])
    {
        if !(1..=65535).contains(&port) {
            return None;
        }
        url.set_port(Some(port as u16)).ok()?;
    }
    valid_webrtc_endpoint(&json!(url.as_str())).then_some(url)
}

fn value_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str()?.parse().ok())
}
