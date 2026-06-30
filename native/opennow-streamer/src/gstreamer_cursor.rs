use crate::gstreamer_backend::send_log;
use crate::gstreamer_input::{channel_label, create_data_channel};
use crate::gstreamer_platform::{apply_native_cursor_update, reset_native_cursor};
use crate::protocol::Event;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use gstreamer as gst;
use gstreamer_webrtc as gst_webrtc;
use std::sync::mpsc::Sender;

const CURSOR_CHANNEL_LABEL: &str = "cursor_channel";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeCursorPosition {
    pub(crate) x: u16,
    pub(crate) y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeCursorImage {
    pub(crate) mime_type: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeCursorUpdate {
    Hidden,
    Predefined {
        cursor_id: u8,
        position: Option<NativeCursorPosition>,
    },
    Custom {
        cursor_id: u8,
        hotspot_x: u8,
        hotspot_y: u8,
        image: Option<NativeCursorImage>,
        position: Option<NativeCursorPosition>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GfnCursorChannelMessage {
    Predefined {
        cursor_id: u8,
        position: Option<NativeCursorPosition>,
    },
    Custom {
        cursor_id: u8,
        hotspot_x: u8,
        hotspot_y: u8,
        mime_type: String,
        image_base64: String,
        position: Option<NativeCursorPosition>,
    },
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let low = *bytes.get(offset)?;
    let high = *bytes.get(offset + 1)?;
    Some(u16::from_le_bytes([low, high]))
}

fn parse_optional_cursor_position(
    bytes: &[u8],
    offset: usize,
) -> (Option<NativeCursorPosition>, usize) {
    if offset + 4 > bytes.len() {
        return (None, offset);
    }

    let Some(x) = read_u16_le(bytes, offset) else {
        return (None, offset);
    };
    let Some(y) = read_u16_le(bytes, offset + 2) else {
        return (None, offset);
    };
    (Some(NativeCursorPosition { x, y }), offset + 4)
}

fn parse_gfn_cursor_channel_message(bytes: &[u8]) -> Option<GfnCursorChannelMessage> {
    let message_type = *bytes.first()?;
    if message_type != 0 && message_type != 1 {
        return None;
    }

    let cursor_id = *bytes.get(1)?;
    if bytes.len() < 5 {
        return (message_type == 0).then_some(GfnCursorChannelMessage::Predefined {
            cursor_id,
            position: None,
        });
    }

    let hotspot_x = bytes[2];
    let hotspot_y = bytes[3];
    let mime_type_len = bytes[4] as usize;
    let mut offset = 5usize;
    if offset + mime_type_len > bytes.len() {
        return None;
    }
    let mime_type = if mime_type_len > 0 {
        std::str::from_utf8(&bytes[offset..offset + mime_type_len])
            .ok()?
            .to_owned()
    } else {
        String::new()
    };
    offset += mime_type_len;

    if offset + 2 > bytes.len() {
        return (message_type == 0).then_some(GfnCursorChannelMessage::Predefined {
            cursor_id,
            position: None,
        });
    }

    let image_len = read_u16_le(bytes, offset)? as usize;
    offset += 2;
    if offset + image_len > bytes.len() {
        return None;
    }
    let image_base64 = if image_len > 0 {
        std::str::from_utf8(&bytes[offset..offset + image_len])
            .ok()?
            .to_owned()
    } else {
        String::new()
    };
    offset += image_len;

    let (position, _next_offset) = parse_optional_cursor_position(bytes, offset);
    if message_type == 0 {
        return Some(GfnCursorChannelMessage::Predefined {
            cursor_id,
            position,
        });
    }

    Some(GfnCursorChannelMessage::Custom {
        cursor_id,
        hotspot_x,
        hotspot_y,
        mime_type,
        image_base64,
        position,
    })
}

fn cursor_update_from_message(
    message: GfnCursorChannelMessage,
) -> Result<NativeCursorUpdate, String> {
    match message {
        GfnCursorChannelMessage::Predefined { cursor_id: 0, .. } => Ok(NativeCursorUpdate::Hidden),
        GfnCursorChannelMessage::Predefined {
            cursor_id,
            position,
        } => Ok(NativeCursorUpdate::Predefined {
            cursor_id,
            position,
        }),
        GfnCursorChannelMessage::Custom {
            cursor_id,
            hotspot_x,
            hotspot_y,
            mime_type,
            image_base64,
            position,
        } => {
            let image = if image_base64.is_empty() {
                None
            } else {
                Some(NativeCursorImage {
                    mime_type: if mime_type.is_empty() {
                        "image/png".to_owned()
                    } else {
                        mime_type
                    },
                    bytes: BASE64_STANDARD
                        .decode(image_base64.as_bytes())
                        .map_err(|error| format!("Invalid cursor image base64: {error}"))?,
                })
            };
            Ok(NativeCursorUpdate::Custom {
                cursor_id,
                hotspot_x,
                hotspot_y,
                image,
                position,
            })
        }
    }
}

pub(crate) fn create_cursor_data_channel(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) -> Result<gst_webrtc::WebRTCDataChannel, String> {
    let channel = create_data_channel(webrtc, CURSOR_CHANNEL_LABEL, None)?;
    connect_cursor_channel_callbacks(&channel, event_sender.clone());
    send_log(
        &event_sender,
        "info",
        "Created WebRTC cursor data channel (cursor_channel).".to_owned(),
    );
    Ok(channel)
}

fn connect_cursor_channel_callbacks(
    channel: &gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    channel.connect_on_open({
        let event_sender = event_sender.clone();
        move |channel| {
            send_log(
                &event_sender,
                "info",
                format!(
                    "Cursor data channel open: label={}, id={}, ordered={}.",
                    channel_label(channel),
                    channel.id(),
                    channel.is_ordered()
                ),
            );
        }
    });

    channel.connect_on_close({
        let event_sender = event_sender.clone();
        move |_| {
            reset_native_cursor();
            send_log(
                &event_sender,
                "info",
                "Cursor data channel closed.".to_owned(),
            );
        }
    });

    channel.connect_on_error({
        let event_sender = event_sender.clone();
        move |_, error| {
            send_log(
                &event_sender,
                "warn",
                format!("Cursor data channel error: {error}."),
            );
        }
    });

    channel.connect_on_message_data({
        let event_sender = event_sender.clone();
        move |_, data| {
            let Some(data) = data else {
                return;
            };
            handle_cursor_channel_message(data.as_ref(), event_sender.clone());
        }
    });

    channel.connect_on_message_string(move |_, message| {
        let Some(message) = message else {
            return;
        };
        handle_cursor_channel_message(message.as_bytes(), event_sender.clone());
    });
}

fn handle_cursor_channel_message(bytes: &[u8], event_sender: Option<Sender<Event>>) {
    let Some(message) = parse_gfn_cursor_channel_message(bytes) else {
        send_log(
            &event_sender,
            "debug",
            format!("Cursor channel message ignored ({} bytes).", bytes.len()),
        );
        return;
    };

    match cursor_update_from_message(message).and_then(apply_native_cursor_update) {
        Ok(true) => {}
        Ok(false) => send_log(
            &event_sender,
            "debug",
            "Cursor channel update did not change the native cursor.".to_owned(),
        ),
        Err(error) => send_log(
            &event_sender,
            "warn",
            format!("Failed to apply cursor channel update: {error}."),
        ),
    }
}

pub(crate) fn disable_native_cursor_channel(event_sender: &Option<Sender<Event>>) {
    reset_native_cursor();
    send_log(
        event_sender,
        "info",
        "Cursor channel disabled; using server-side cursor rendering.".to_owned(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_le(value: u16) -> [u8; 2] {
        value.to_le_bytes()
    }

    #[test]
    fn parses_predefined_cursor_update_with_position() {
        let bytes = [
            0,
            12,
            0,
            0,
            0,
            0,
            0,
            u16_le(32768)[0],
            u16_le(32768)[1],
            u16_le(65535)[0],
            u16_le(65535)[1],
        ];

        assert_eq!(
            parse_gfn_cursor_channel_message(&bytes),
            Some(GfnCursorChannelMessage::Predefined {
                cursor_id: 12,
                position: Some(NativeCursorPosition { x: 32768, y: 65535 }),
            })
        );
    }

    #[test]
    fn parses_custom_cursor_metadata() {
        let mime = b"image/png";
        let image = b"AAAA";
        let mut bytes = vec![1, 7, 3, 4, mime.len() as u8];
        bytes.extend_from_slice(mime);
        bytes.extend_from_slice(&(image.len() as u16).to_le_bytes());
        bytes.extend_from_slice(image);
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&150u16.to_le_bytes());

        assert_eq!(
            parse_gfn_cursor_channel_message(&bytes),
            Some(GfnCursorChannelMessage::Custom {
                cursor_id: 7,
                hotspot_x: 3,
                hotspot_y: 4,
                mime_type: "image/png".to_owned(),
                image_base64: "AAAA".to_owned(),
                position: Some(NativeCursorPosition { x: 10, y: 20 }),
            })
        );
    }

    #[test]
    fn rejects_truncated_custom_image() {
        let bytes = [1, 1, 0, 0, 0, 4, 0, b'A', b'A'];
        assert_eq!(parse_gfn_cursor_channel_message(&bytes), None);
    }

    #[test]
    fn converts_hidden_predefined_cursor_to_hidden_update() {
        assert_eq!(
            cursor_update_from_message(GfnCursorChannelMessage::Predefined {
                cursor_id: 0,
                position: Some(NativeCursorPosition { x: 1, y: 2 }),
            })
            .unwrap(),
            NativeCursorUpdate::Hidden
        );
    }
}
