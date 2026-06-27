use crate::protocol::{NativeStreamerShortcutAction, NativeStreamerShortcutBindings};

const MODIFIER_SHIFT: u16 = 0x01;
const MODIFIER_CTRL: u16 = 0x02;
const MODIFIER_ALT: u16 = 0x04;
const MODIFIER_META: u16 = 0x08;
const SHORTCUT_MODIFIER_MASK: u16 = MODIFIER_SHIFT | MODIFIER_CTRL | MODIFIER_ALT | MODIFIER_META;

const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_PAUSE: u16 = 0x13;
const VK_CAPITAL: u16 = 0x14;
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_INSERT: u16 = 0x2D;
const VK_DELETE: u16 = 0x2E;
const VK_PRINT: u16 = 0x2C;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_APPS: u16 = 0x5D;
const VK_NUMPAD0: u16 = 0x60;
const VK_MULTIPLY: u16 = 0x6A;
const VK_ADD: u16 = 0x6B;
const VK_SEPARATOR: u16 = 0x6C;
const VK_SUBTRACT: u16 = 0x6D;
const VK_DECIMAL: u16 = 0x6E;
const VK_DIVIDE: u16 = 0x6F;
const VK_F1: u16 = 0x70;
const VK_NUMLOCK: u16 = 0x90;
const VK_SCROLL: u16 = 0x91;
const VK_OEM_1: u16 = 0xBA;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_COMMA: u16 = 0xBC;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_OEM_PERIOD: u16 = 0xBE;
const VK_OEM_2: u16 = 0xBF;
const VK_OEM_3: u16 = 0xC0;
const VK_OEM_4: u16 = 0xDB;
const VK_OEM_5: u16 = 0xDC;
const VK_OEM_6: u16 = 0xDD;
const VK_OEM_7: u16 = 0xDE;
const SCANCODE_ENTER: u16 = 0x001C;
const SCANCODE_NUMPAD_ENTER: u16 = 0xE01C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShortcutBinding {
    action: NativeStreamerShortcutAction,
    keycode: u16,
    scancode: Option<u16>,
    modifiers: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedKey {
    keycode: u16,
    scancode: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NativeShortcutMatcher {
    bindings: Vec<ShortcutBinding>,
}

impl NativeShortcutMatcher {
    pub(crate) fn from_bindings(bindings: &NativeStreamerShortcutBindings) -> Self {
        let mut parsed = Vec::new();
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::ToggleStats,
            &bindings.toggle_stats,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::TogglePointerLock,
            &bindings.toggle_pointer_lock,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::ToggleFullscreen,
            &bindings.toggle_fullscreen,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::StopStream,
            &bindings.stop_stream,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::ToggleAntiAfk,
            &bindings.toggle_anti_afk,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::ToggleMicrophone,
            &bindings.toggle_microphone,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::Screenshot,
            &bindings.screenshot,
        );
        append_binding(
            &mut parsed,
            NativeStreamerShortcutAction::ToggleRecording,
            &bindings.toggle_recording,
        );
        Self { bindings: parsed }
    }

    pub(crate) fn match_keydown(
        &self,
        keycode: u16,
        scancode: u16,
        modifiers: u16,
    ) -> Option<NativeStreamerShortcutAction> {
        let modifiers = modifiers & SHORTCUT_MODIFIER_MASK;
        self.bindings
            .iter()
            .find(|binding| {
                binding.keycode == keycode
                    && binding.modifiers == modifiers
                    && binding.scancode.map_or(true, |expected| expected == scancode)
            })
            .map(|binding| binding.action)
    }
}

fn append_binding(
    bindings: &mut Vec<ShortcutBinding>,
    action: NativeStreamerShortcutAction,
    raw: &str,
) {
    if let Some(binding) = parse_binding(action, raw) {
        bindings.push(binding);
    }
}

fn parse_binding(action: NativeStreamerShortcutAction, raw: &str) -> Option<ShortcutBinding> {
    let mut modifiers = 0u16;
    let mut key = None;

    for token in raw.split('+').map(str::trim).filter(|token| !token.is_empty()) {
        match token.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => modifiers |= MODIFIER_CTRL,
            "ALT" | "OPTION" => modifiers |= MODIFIER_ALT,
            "SHIFT" => modifiers |= MODIFIER_SHIFT,
            "META" | "CMD" | "COMMAND" => modifiers |= MODIFIER_META,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = parse_key_token(token);
            }
        }
    }

    let key = key?;
    Some(ShortcutBinding {
        action,
        keycode: key.keycode,
        scancode: key.scancode,
        modifiers,
    })
}

fn parsed_key(keycode: u16) -> Option<ParsedKey> {
    Some(ParsedKey {
        keycode,
        scancode: None,
    })
}

fn parsed_scancode_key(keycode: u16, scancode: u16) -> Option<ParsedKey> {
    Some(ParsedKey {
        keycode,
        scancode: Some(scancode),
    })
}

fn parse_key_token(token: &str) -> Option<ParsedKey> {
    let upper = token.trim().to_ascii_uppercase();
    if upper.len() == 1 {
        let byte = upper.as_bytes()[0];
        return match byte {
            b'A'..=b'Z' | b'0'..=b'9' => parsed_key(u16::from(byte)),
            b',' => parsed_key(VK_OEM_COMMA),
            b'.' => parsed_key(VK_OEM_PERIOD),
            b'/' => parsed_key(VK_OEM_2),
            b';' => parsed_key(VK_OEM_1),
            b'\'' => parsed_key(VK_OEM_7),
            b'[' => parsed_key(VK_OEM_4),
            b']' => parsed_key(VK_OEM_6),
            b'\\' => parsed_key(VK_OEM_5),
            b'-' => parsed_key(VK_OEM_MINUS),
            b'=' => parsed_key(VK_OEM_PLUS),
            b'`' => parsed_key(VK_OEM_3),
            _ => None,
        };
    }

    if let Some(function_index) = upper
        .strip_prefix('F')
        .and_then(|value| value.parse::<u16>().ok())
    {
        if (1..=24).contains(&function_index) {
            return parsed_key(VK_F1 + function_index - 1);
        }
    }

    if let Some(numpad_index) = upper
        .strip_prefix("NUMPAD")
        .and_then(|value| value.parse::<u16>().ok())
    {
        if numpad_index <= 9 {
            return parsed_key(VK_NUMPAD0 + numpad_index);
        }
    }

    match upper.as_str() {
        "BACKSPACE" => parsed_key(VK_BACK),
        "TAB" => parsed_key(VK_TAB),
        "ENTER" => parsed_scancode_key(VK_RETURN, SCANCODE_ENTER),
        "NUMPADENTER" => parsed_scancode_key(VK_RETURN, SCANCODE_NUMPAD_ENTER),
        "PAUSE" => parsed_key(VK_PAUSE),
        "CAPSLOCK" => parsed_key(VK_CAPITAL),
        "ESCAPE" => parsed_key(VK_ESCAPE),
        "SPACE" => parsed_key(VK_SPACE),
        "PAGEUP" => parsed_key(VK_PRIOR),
        "PAGEDOWN" => parsed_key(VK_NEXT),
        "END" => parsed_key(VK_END),
        "HOME" => parsed_key(VK_HOME),
        "ARROWLEFT" => parsed_key(VK_LEFT),
        "ARROWUP" => parsed_key(VK_UP),
        "ARROWRIGHT" => parsed_key(VK_RIGHT),
        "ARROWDOWN" => parsed_key(VK_DOWN),
        "INSERT" => parsed_key(VK_INSERT),
        "DELETE" => parsed_key(VK_DELETE),
        "PRINTSCREEN" => parsed_key(VK_PRINT),
        "APPS" | "MENU" => parsed_key(VK_APPS),
        "METALEFT" => parsed_key(VK_LWIN),
        "METARIGHT" => parsed_key(VK_RWIN),
        "NUMPADMULTIPLY" => parsed_key(VK_MULTIPLY),
        "NUMPADADD" => parsed_key(VK_ADD),
        "NUMPADSEPARATOR" => parsed_key(VK_SEPARATOR),
        "NUMPADSUBTRACT" => parsed_key(VK_SUBTRACT),
        "NUMPADDECIMAL" => parsed_key(VK_DECIMAL),
        "NUMPADDIVIDE" => parsed_key(VK_DIVIDE),
        "NUMLOCK" => parsed_key(VK_NUMLOCK),
        "SCROLLLOCK" => parsed_key(VK_SCROLL),
        "SEMICOLON" => parsed_key(VK_OEM_1),
        "EQUAL" => parsed_key(VK_OEM_PLUS),
        "COMMA" => parsed_key(VK_OEM_COMMA),
        "MINUS" => parsed_key(VK_OEM_MINUS),
        "PERIOD" => parsed_key(VK_OEM_PERIOD),
        "SLASH" => parsed_key(VK_OEM_2),
        "BACKQUOTE" => parsed_key(VK_OEM_3),
        "BRACKETLEFT" => parsed_key(VK_OEM_4),
        "BACKSLASH" => parsed_key(VK_OEM_5),
        "BRACKETRIGHT" => parsed_key(VK_OEM_6),
        "QUOTE" => parsed_key(VK_OEM_7),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> NativeStreamerShortcutBindings {
        NativeStreamerShortcutBindings {
            toggle_stats: "F3".to_owned(),
            toggle_pointer_lock: "F8".to_owned(),
            toggle_fullscreen: "F10".to_owned(),
            stop_stream: "Ctrl+Shift+Q".to_owned(),
            toggle_anti_afk: "Ctrl+Shift+K".to_owned(),
            toggle_microphone: "Ctrl+Shift+M".to_owned(),
            screenshot: "F11".to_owned(),
            toggle_recording: "F12".to_owned(),
        }
    }

    #[test]
    fn matches_function_key_shortcuts_without_modifiers() {
        let matcher = NativeShortcutMatcher::from_bindings(&bindings());

        assert_eq!(
            matcher.match_keydown(VK_F1 + 2, 0, 0),
            Some(NativeStreamerShortcutAction::ToggleStats)
        );
        assert_eq!(
            matcher.match_keydown(VK_F1 + 10, 0, 0),
            Some(NativeStreamerShortcutAction::Screenshot)
        );
    }

    #[test]
    fn matches_exact_modifier_combinations() {
        let matcher = NativeShortcutMatcher::from_bindings(&bindings());

        assert_eq!(
            matcher.match_keydown(u16::from(b'Q'), 0, MODIFIER_CTRL | MODIFIER_SHIFT),
            Some(NativeStreamerShortcutAction::StopStream)
        );
        assert_eq!(matcher.match_keydown(u16::from(b'Q'), 0, MODIFIER_CTRL), None);
        assert_eq!(
            matcher.match_keydown(
                u16::from(b'Q'),
                0,
                MODIFIER_CTRL | MODIFIER_SHIFT | MODIFIER_ALT
            ),
            None
        );
    }

    #[test]
    fn ignores_invalid_bindings() {
        let matcher = NativeShortcutMatcher::from_bindings(&NativeStreamerShortcutBindings {
            toggle_stats: "Ctrl+Shift".to_owned(),
            ..bindings()
        });

        assert_eq!(matcher.match_keydown(VK_F1 + 2, 0, 0), None);
        assert_eq!(
            matcher.match_keydown(VK_F1 + 7, 0, 0),
            Some(NativeStreamerShortcutAction::TogglePointerLock)
        );
    }

    #[test]
    fn keeps_regular_enter_and_numpad_enter_distinct() {
        let matcher = NativeShortcutMatcher::from_bindings(&NativeStreamerShortcutBindings {
            toggle_stats: "Enter".to_owned(),
            toggle_pointer_lock: "NumpadEnter".to_owned(),
            ..bindings()
        });

        assert_eq!(
            matcher.match_keydown(VK_RETURN, SCANCODE_ENTER, 0),
            Some(NativeStreamerShortcutAction::ToggleStats)
        );
        assert_eq!(
            matcher.match_keydown(VK_RETURN, SCANCODE_NUMPAD_ENTER, 0),
            Some(NativeStreamerShortcutAction::TogglePointerLock)
        );
    }
}
