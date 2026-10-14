use serde_json::Value;

pub fn append_session_preferences(url: &mut url::Url, settings: &Value) {
    let language = settings["gameLanguage"]
        .as_str()
        .filter(|value| crate::language::valid_game_language(value))
        .unwrap_or("en_US");
    url.query_pairs_mut()
        .append_pair(
            "keyboardLayout",
            crate::language::session_keyboard_layout(settings),
        )
        .append_pair("languageCode", language);
}
