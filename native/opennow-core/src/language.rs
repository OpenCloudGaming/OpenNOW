use serde_json::{Value, json};

const KEYBOARDS: &[(&str, &str, &[&str])] = &[
    ("en-US", "English (US)", &[]),
    ("en-GB", "English (UK)", &[]),
    ("tr-TR", "Turkish Q", &[]),
    ("de-DE", "German", &[]),
    ("fr-FR", "French", &[]),
    ("es-ES_tradnl", "Spanish (traditional)", &["es-ES"]),
    ("es-MX", "Spanish (Latin America)", &[]),
    ("it-IT", "Italian", &[]),
    ("pt-PT", "Portuguese (Portugal)", &[]),
    ("pt-BR", "Portuguese (Brazil)", &[]),
    ("pl-PL", "Polish", &[]),
    ("da-DK", "Danish", &[]),
    ("nb-NO", "Norwegian", &[]),
    ("sv-SE", "Swedish", &[]),
    ("fi-FI", "Finnish", &[]),
    ("ru-RU", "Russian", &[]),
    ("uk-UA", "Ukrainian", &[]),
    ("ja-106", "Japanese 106", &["ja-JP", "Japanese106"]),
    ("ko-KR", "Korean", &[]),
    ("zh-CN", "Chinese (Simplified)", &[]),
    ("zh-TW", "Chinese (Traditional)", &[]),
];

pub fn valid_game_language(value: &str) -> bool {
    if value.len() > 64 || matches!(value.to_ascii_lowercase().as_str(), "auto" | "system") {
        return false;
    }
    let mut parts = value.split(['-', '_']);
    let first = parts.next().unwrap_or_default();
    (2..=8).contains(&first.len())
        && first.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn keyboard_wire_id(value: &str) -> Option<&'static str> {
    KEYBOARDS
        .iter()
        .find_map(|(id, _, aliases)| (*id == value || aliases.contains(&value)).then_some(*id))
}

pub fn validate_setting(key: &str, value: &Value) -> Result<(), String> {
    let valid = value.as_str().is_some_and(|value| match key {
        "gameLanguage" => valid_game_language(value),
        "keyboardLayout" => keyboard_wire_id(value).is_some(),
        _ => true,
    });
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid {key} identifier"))
    }
}

pub fn keyboard_choices() -> Value {
    json!(
        KEYBOARDS
            .iter()
            .map(|(id, label, aliases)| { json!({"value":id, "label":label, "aliases":aliases}) })
            .collect::<Vec<_>>()
    )
}

pub fn session_keyboard_layout(settings: &Value) -> &'static str {
    settings["keyboardLayout"]
        .as_str()
        .and_then(keyboard_wire_id)
        .unwrap_or("en-US")
}

pub fn append_session_preferences(url: &mut url::Url, settings: &Value) {
    let language = settings["gameLanguage"]
        .as_str()
        .filter(|value| valid_game_language(value))
        .unwrap_or("en_US");
    url.query_pairs_mut()
        .append_pair("keyboardLayout", session_keyboard_layout(settings))
        .append_pair("languageCode", language);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_identifiers_preserve_wire_spelling_and_allow_safe_future_values() {
        for value in [
            "en_US",
            "es_419",
            "zh_Hant_TW",
            "sr-Latn-RS",
            "future_001",
            "EN-us",
        ] {
            assert!(valid_game_language(value), "{value}");
        }
        for value in [
            "", "auto", "AUTO", "system", "en US", "en\nUS", "en__US", "en/US", "1en", "en_", "éé",
        ] {
            assert!(!valid_game_language(value), "{value:?}");
        }
        assert!(!valid_game_language(&format!(
            "en{}",
            "_abcdefgh".repeat(8)
        )));
    }

    #[test]
    fn keyboard_table_and_legacy_aliases_have_one_wire_identity() {
        for (id, _, aliases) in KEYBOARDS {
            assert_eq!(keyboard_wire_id(id), Some(*id));
            for alias in *aliases {
                assert_eq!(keyboard_wire_id(alias), Some(*id));
            }
        }
        assert_eq!(keyboard_wire_id("ja-JP"), Some("ja-106"));
        assert_eq!(keyboard_wire_id("Japanese106"), Some("ja-106"));
        assert_eq!(keyboard_wire_id("es-ES"), Some("es-ES_tradnl"));
        for value in ["auto", "system", "en_US", "m-us", "unknown"] {
            assert!(keyboard_wire_id(value).is_none());
        }
    }

    #[test]
    fn request_preferences_are_independent_and_corrupt_restore_is_safe() {
        for (settings, language, keyboard) in [
            (
                json!({"appLanguage":"ja", "gameLanguage":"es_419", "keyboardLayout":"de-DE"}),
                "es_419",
                "de-DE",
            ),
            (
                json!({"gameLanguage":"zh_Hant_TW", "keyboardLayout":"ja-JP"}),
                "zh_Hant_TW",
                "ja-106",
            ),
            (
                json!({"gameLanguage":"system", "keyboardLayout":"m-us"}),
                "en_US",
                "en-US",
            ),
            (
                json!({"gameLanguage":"auto", "keyboardLayout":"es-ES"}),
                "en_US",
                "es-ES_tradnl",
            ),
        ] {
            let mut url = url::Url::parse("https://fixture.invalid/session?existing=1").unwrap();
            append_session_preferences(&mut url, &settings);
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
            assert_eq!(pairs["languageCode"], language);
            assert_eq!(pairs["keyboardLayout"], keyboard);
            assert_eq!(pairs["existing"], "1");
            assert_eq!(pairs.len(), 3);
        }
    }
}
