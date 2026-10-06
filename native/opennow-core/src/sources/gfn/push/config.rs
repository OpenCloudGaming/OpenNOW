use crate::sources::gfn::push::PushOwnerConfig;
use crate::sources::gfn::push::registration::{PushEndpoints, PushIdentity};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

pub const FILE_NAME: &str = "push.json";
pub const MAXIMUM_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushConfig {
    Absent,
    Configured(Box<PushOwnerConfig>),
    Suppressed,
}

pub fn load_for_provider(data_dir: &Path, provider_idp_id: &str, device_id: String) -> PushConfig {
    let path = data_dir.join(FILE_NAME);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return PushConfig::Absent,
        Err(_) => return PushConfig::Suppressed,
        Ok(_) => {}
    }
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        _ => return PushConfig::Suppressed,
    }
    let Some(bytes) = read_bounded(&path) else {
        return PushConfig::Suppressed;
    };
    let Ok(raw) = serde_json::from_slice::<Value>(&bytes) else {
        return PushConfig::Suppressed;
    };
    let Some(entry) = select_entry(&raw, provider_idp_id) else {
        return PushConfig::Suppressed;
    };
    from_json(entry, provider_idp_id, device_id)
        .map(|config| PushConfig::Configured(Box::new(config)))
        .unwrap_or(PushConfig::Suppressed)
}

fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAXIMUM_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAXIMUM_CONFIG_BYTES).then_some(bytes)
}

fn select_entry<'a>(raw: &'a Value, provider_idp_id: &str) -> Option<&'a Value> {
    match raw {
        Value::Array(entries) => entries.iter().find(|entry| {
            entry
                .get("providerIdpId")
                .and_then(Value::as_str)
                .is_some_and(|provider| provider == provider_idp_id)
        }),
        Value::Object(_) => Some(raw),
        _ => None,
    }
}

pub(crate) fn pns_base_is_usable(base: &str) -> bool {
    let Ok(url) = url::Url::parse(base) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

pub fn from_json(raw: &Value, provider_idp_id: &str, device_id: String) -> Option<PushOwnerConfig> {
    let object = raw.as_object()?;
    if let Some(enabled) = object.get("enabled") {
        if enabled.as_bool() != Some(true) {
            return None;
        }
    }
    let string = |key: &str| -> Option<String> {
        object
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let configured_provider = string("providerIdpId")?;
    if configured_provider != provider_idp_id {
        return None;
    }
    let project_id = string("projectId")?;
    let api_key = string("apiKey")?;
    let sender_id = string("senderId")?;
    let app_id = string("appId")?;
    let firebase_app_id = string("firebaseAppId")?;
    let pns_server = string("pnsServer")?;
    let pns_version = string("pnsVersion").unwrap_or_else(|| "v1".to_owned());
    let pns_client_id = string("pnsClientId")?;
    let pns_base = format!(
        "{}/{}",
        pns_server.trim_end_matches('/'),
        pns_version.trim_matches('/')
    );
    if !pns_base_is_usable(&pns_server) || !pns_base_is_usable(&pns_base) {
        return None;
    }
    let endpoints = PushEndpoints {
        pns: pns_base,
        pns_client_id,
        ..PushEndpoints::default()
    };
    let identity = PushIdentity {
        project_id,
        api_key,
        sender_id,
        app_id,
        firebase_app_id,
        vapid_key: string("vapidKey"),
    };
    Some(PushOwnerConfig::bounded(
        endpoints,
        identity,
        configured_provider,
        device_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> Value {
        json!({
            "providerIdpId": "nvidia",
            "projectId": "project",
            "apiKey": "api-key",
            "senderId": "123456789012",
            "appId": "app",
            "firebaseAppId": "1:1:web:abc",
            "vapidKey": "vapid",
            "pnsServer": "https://pns.example/",
            "pnsVersion": "v1",
            "pnsClientId": "client"
        })
    }

    fn data_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("opennow-push-config-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn load(dir: &Path, provider: &str) -> PushConfig {
        load_for_provider(dir, provider, "device".into())
    }

    #[test]
    fn valid_config_produces_a_bounded_owner_config() {
        let config = from_json(&raw(), "nvidia", "device".into()).unwrap();
        assert_eq!(config.identity.project_id, "project");
        assert_eq!(config.identity.sender_id, "123456789012");
        assert_eq!(config.endpoints.pns, "https://pns.example/v1");
        assert_eq!(config.endpoints.pns_client_id, "client");
        assert_eq!(config.device_id, "device");
        assert_eq!(config.endpoints.mcs_host, "mtalk.google.com");
        assert!(config.maximum_frame_bytes <= 64 * 1024);
    }

    #[test]
    fn provider_mismatch_never_starts_the_vendor_config() {
        assert!(from_json(&raw(), "alliance", "device".into()).is_none());
    }

    #[test]
    fn incomplete_or_blank_config_is_rejected() {
        let mut value = raw();
        value["projectId"] = json!("   ");
        assert!(from_json(&value, "nvidia", "device".into()).is_none());
        let mut value = raw();
        value.as_object_mut().unwrap().remove("pnsServer");
        assert!(from_json(&value, "nvidia", "device".into()).is_none());
        assert!(from_json(&json!("not an object"), "nvidia", "device".into()).is_none());
    }

    #[test]
    fn missing_vapid_key_is_allowed_and_version_defaults() {
        let mut value = raw();
        value.as_object_mut().unwrap().remove("vapidKey");
        value.as_object_mut().unwrap().remove("pnsVersion");
        let config = from_json(&value, "nvidia", "device".into()).unwrap();
        assert!(config.identity.vapid_key.is_none());
        assert_eq!(config.endpoints.pns, "https://pns.example/v1");
    }

    #[test]
    fn an_explicit_disable_suppresses_the_override() {
        assert!(from_json(&json!({"enabled": false}), "nvidia", "device".into()).is_none());
        let mut value = raw();
        value["enabled"] = json!(false);
        assert!(from_json(&value, "nvidia", "device".into()).is_none());
        assert!(from_json(&json!({"enabled": "no"}), "nvidia", "device".into()).is_none());
        let mut value = raw();
        value["enabled"] = json!(true);
        assert!(from_json(&value, "nvidia", "device".into()).is_some());
    }

    #[test]
    fn the_config_file_is_absent_without_an_override() {
        let dir = data_dir("absent");
        assert_eq!(load(&dir, "nvidia"), PushConfig::Absent);
    }

    #[test]
    fn the_config_file_is_configured_for_a_complete_matching_entry() {
        let dir = data_dir("configured");
        std::fs::write(dir.join(FILE_NAME), raw().to_string()).unwrap();
        let PushConfig::Configured(config) = load(&dir, "nvidia") else {
            panic!("a complete entry must be configured");
        };
        assert_eq!(config.identity.project_id, "project");
        assert_eq!(config.provider_id, "nvidia");
    }

    #[test]
    fn an_existing_override_never_falls_back_to_a_default() {
        let dir = data_dir("suppressed");
        std::fs::write(dir.join(FILE_NAME), raw().to_string()).unwrap();
        assert_eq!(load(&dir, "alliance"), PushConfig::Suppressed);
        std::fs::write(dir.join(FILE_NAME), b"not json").unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Suppressed);
        std::fs::write(dir.join(FILE_NAME), json!({"enabled": false}).to_string()).unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Suppressed);
    }

    #[test]
    fn the_config_file_selects_the_matching_provider_entry() {
        let dir = data_dir("array");
        let entries = json!([
            {"providerIdpId": "alliance", "projectId": "other"},
            raw()
        ]);
        std::fs::write(dir.join(FILE_NAME), entries.to_string()).unwrap();
        let PushConfig::Configured(config) = load(&dir, "nvidia") else {
            panic!("the matching entry must be configured");
        };
        assert_eq!(config.identity.project_id, "project");
        assert_eq!(load(&dir, "alliance"), PushConfig::Suppressed);
    }

    #[test]
    fn oversized_config_files_are_suppressed() {
        let dir = data_dir("oversized");
        std::fs::write(
            dir.join(FILE_NAME),
            vec![b' '; MAXIMUM_CONFIG_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Suppressed);
    }

    #[test]
    fn an_empty_config_file_is_suppressed() {
        let dir = data_dir("empty");
        std::fs::write(dir.join(FILE_NAME), b"").unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Suppressed);
    }

    #[test]
    fn an_insecure_or_ambiguous_pns_base_is_rejected() {
        for server in [
            "http://pns.example",
            "https://",
            "https://user@pns.example",
            "https://user:pw@pns.example",
            "https://pns.example/?x=1",
            "https://pns.example/#f",
            "pns.example",
            "not a url",
        ] {
            let mut value = raw();
            value["pnsServer"] = json!(server);
            assert!(
                from_json(&value, "nvidia", "device".into()).is_none(),
                "the pns base {server} must be rejected"
            );
        }
    }

    #[test]
    fn a_valid_pns_base_keeps_the_documented_shape() {
        let mut value = raw();
        value["pnsServer"] = json!("https://pns.example/api");
        value["pnsVersion"] = json!("v2");
        let config = from_json(&value, "nvidia", "device".into()).unwrap();
        assert_eq!(config.endpoints.pns, "https://pns.example/api/v2");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_config_link_is_suppressed() {
        let dir = data_dir("dangling");
        let link = dir.join(FILE_NAME);
        std::os::unix::fs::symlink(dir.join("missing-push.json"), &link).unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Suppressed);
        std::fs::remove_file(&link).unwrap();
        assert_eq!(load(&dir, "nvidia"), PushConfig::Absent);
    }

    #[cfg(unix)]
    #[test]
    fn a_config_link_to_a_valid_file_is_configured() {
        let dir = data_dir("link-valid");
        let target = dir.join("target.json");
        std::fs::write(&target, raw().to_string()).unwrap();
        std::os::unix::fs::symlink(&target, dir.join(FILE_NAME)).unwrap();
        let PushConfig::Configured(config) = load(&dir, "nvidia") else {
            panic!("a valid link target must stay configured");
        };
        assert_eq!(config.identity.project_id, "project");
    }

    #[cfg(unix)]
    #[test]
    fn a_non_regular_override_entry_is_suppressed_without_blocking() {
        use std::os::unix::ffi::OsStrExt as _;
        let dir = data_dir("fifo");
        let path = dir.join(FILE_NAME);
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(load(&dir, "nvidia"));
        });
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("a non-regular override entry must not block the loader"),
            PushConfig::Suppressed
        );
    }
}
