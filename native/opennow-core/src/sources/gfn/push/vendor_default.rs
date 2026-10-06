use crate::sources::gfn::push::owner::PushOwnerConfig;
use crate::sources::gfn::push::registration::{PushEndpoints, PushIdentity};

const PROJECT_ID: &str = "nv-pushnotificationss-20220823";
const API_KEY: &str = "AIzaSyCyGIQv16BXoVfMOSXesGclbZnC6krMBmU";
const SENDER_ID: &str = "950535643107";
const APP_ID: &str = "nv-pushnotificationss-20220823";
const FIREBASE_APP_ID: &str = "1:950535643107:web:a2e08529da36c9456e0ad6";
const VAPID_KEY: &str =
    "BKXL_5beeXnhyvgZV-1BzEB6DH-bYL9gsql510ilbB8gYBxJavlpdHPF6VtjyrmowsskhD3qn72uvIMhiffB6UY";
const PNS_SERVER: &str = "https://pns.geforcenow.com";
const PNS_VERSION: &str = "v1";
const PNS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";

pub fn config_for_provider(provider_idp_id: &str, device_id: String) -> PushOwnerConfig {
    PushOwnerConfig::bounded(
        PushEndpoints {
            pns: format!(
                "{}/{}",
                PNS_SERVER.trim_end_matches('/'),
                PNS_VERSION.trim_matches('/')
            ),
            pns_client_id: PNS_CLIENT_ID.to_owned(),
            ..PushEndpoints::default()
        },
        PushIdentity {
            project_id: PROJECT_ID.to_owned(),
            api_key: API_KEY.to_owned(),
            sender_id: SENDER_ID.to_owned(),
            app_id: APP_ID.to_owned(),
            firebase_app_id: FIREBASE_APP_ID.to_owned(),
            vapid_key: Some(VAPID_KEY.to_owned()),
        },
        provider_idp_id.to_owned(),
        device_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_default_is_complete_and_public_only() {
        let config = config_for_provider("nvidia", "device".into());
        assert_eq!(config.provider_id, "nvidia");
        assert_eq!(config.device_id, "device");
        assert_eq!(config.identity.project_id, PROJECT_ID);
        assert!(!config.identity.api_key.is_empty());
        assert!(!config.identity.sender_id.is_empty());
        assert!(!config.identity.app_id.is_empty());
        assert!(!config.identity.firebase_app_id.is_empty());
        assert!(
            config
                .identity
                .vapid_key
                .as_deref()
                .is_some_and(|key| !key.is_empty())
        );
        assert!(!config.endpoints.pns.is_empty());
        assert!(!config.endpoints.pns.ends_with('/'));
        assert!(!config.endpoints.pns_client_id.is_empty());
        assert_eq!(
            config.endpoints.pns,
            format!(
                "{}/{}",
                PNS_SERVER.trim_end_matches('/'),
                PNS_VERSION.trim_matches('/')
            )
        );
        assert_eq!(config.endpoints.mcs_host, PushEndpoints::default().mcs_host);
    }

    #[test]
    fn the_bundled_default_is_bound_to_the_authenticated_provider() {
        assert_eq!(
            config_for_provider("nvidia", "device".into()).provider_id,
            "nvidia"
        );
        assert_eq!(
            config_for_provider("alliance", "device".into()).provider_id,
            "alliance"
        );
    }

    #[test]
    fn the_bundled_default_uses_a_usable_https_pns_base() {
        let config = config_for_provider("nvidia", "device".into());
        assert!(
            crate::sources::gfn::push::config::pns_base_is_usable(&config.endpoints.pns),
            "the bundled default must keep the evidenced https pns base"
        );
    }

    #[test]
    fn the_bundled_default_carries_no_device_or_user_secret() {
        let first = config_for_provider("nvidia", "device-a".into());
        let second = config_for_provider("nvidia", "device-b".into());
        assert_eq!(first.identity, second.identity);
        assert_eq!(first.endpoints, second.endpoints);
        assert_ne!(first, second);
    }
}
