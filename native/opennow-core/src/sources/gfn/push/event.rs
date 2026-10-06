use crate::sources::gfn::push::PushEvent;
use serde_json::{Value, json};

pub const EVENT_NAME: &str = "account.push.changed";

pub fn payload(event: &PushEvent, generation: u64) -> Value {
    match event {
        PushEvent::Library { changed_ids } => json!({
            "generation": generation,
            "kind": "library",
            "changedIds": changed_ids,
        }),
        PushEvent::Favorites { changed_ids } => json!({
            "generation": generation,
            "kind": "favorites",
            "changedIds": changed_ids,
        }),
        PushEvent::Subscription { changed_ids } => json!({
            "generation": generation,
            "kind": "subscription",
            "changedIds": changed_ids,
        }),
        PushEvent::LinkedAccount {
            account_type,
            linked,
        } => json!({
            "generation": generation,
            "kind": "linked-account",
            "accountType": account_type,
            "linked": linked,
        }),
        PushEvent::PlatformSync {
            platform_code,
            sync_state,
            sync_date,
            sync_game_count,
        } => json!({
            "generation": generation,
            "kind": "platform-sync",
            "platformCode": platform_code,
            "syncState": sync_state,
            "syncDate": sync_date,
            "syncGameCount": sync_game_count,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_events_carry_the_generation_and_changed_ids() {
        let value = payload(
            &PushEvent::Library {
                changed_ids: vec!["app-1".into()],
            },
            7,
        );
        assert_eq!(value["kind"], "library");
        assert_eq!(value["generation"], 7);
        assert_eq!(value["changedIds"][0], "app-1");
        assert_eq!(EVENT_NAME, "account.push.changed");
    }

    #[test]
    fn platform_sync_events_never_claim_completion() {
        let value = payload(
            &PushEvent::PlatformSync {
                platform_code: Some("STEAM".into()),
                sync_state: Some("SYNCING".into()),
                sync_date: Some("2026-01-01T00:00:00Z".into()),
                sync_game_count: Some(12),
            },
            9,
        );
        assert_eq!(value["kind"], "platform-sync");
        assert_eq!(value["syncState"], "SYNCING");
        assert!(value.get("completed").is_none());
    }

    #[test]
    fn linked_account_and_favorites_payloads_are_explicit() {
        let linked = payload(
            &PushEvent::LinkedAccount {
                account_type: Some("STEAM".into()),
                linked: Some(true),
            },
            1,
        );
        assert_eq!(linked["accountType"], "STEAM");
        assert_eq!(linked["linked"], true);
        let favorites = payload(
            &PushEvent::Favorites {
                changed_ids: vec![],
            },
            2,
        );
        assert_eq!(favorites["kind"], "favorites");
        assert_eq!(favorites["changedIds"].as_array().unwrap().len(), 0);
    }
}
