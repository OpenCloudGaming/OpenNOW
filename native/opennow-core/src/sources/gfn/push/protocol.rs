use crate::sources::gfn::push::PushError;

pub const MCS_VERSION: u8 = 41;

pub const TAG_HEARTBEAT_PING: u8 = 0;
pub const TAG_HEARTBEAT_ACK: u8 = 1;
pub const TAG_LOGIN_REQUEST: u8 = 2;
pub const TAG_LOGIN_RESPONSE: u8 = 3;
pub const TAG_CLOSE: u8 = 4;
pub const TAG_IQ_STANZA: u8 = 7;
pub const TAG_DATA_MESSAGE_STANZA: u8 = 8;

pub const STREAM_ACK_EXTENSION_ID: u64 = 13;
pub const IQ_TYPE_SET: u64 = 1;

pub const AUTH_SERVICE_ANDROID_ID: u64 = 2;

pub struct CheckinRequest {
    pub android_id: Option<i64>,
    pub security_token: Option<u64>,
}

pub struct CheckinResponse {
    pub android_id: Option<u64>,
    pub security_token: Option<u64>,
}

pub struct LoginResponse {
    pub error_code: Option<i32>,
    pub heartbeat_interval_ms: Option<i32>,
}

pub struct DataMessage {
    pub from: String,
    pub category: String,
    pub app_data: Vec<(String, String)>,
    pub persistent_id: Option<String>,
    pub raw_data: Option<Vec<u8>>,
    pub immediate_ack: bool,
}

pub enum Frame {
    HeartbeatPing,
    HeartbeatAck,
    LoginResponse(LoginResponse),
    DataMessage(Box<DataMessage>),
    Close,
    Other,
}

fn write_frame(versioned: bool, tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 8);
    if versioned {
        frame.push(MCS_VERSION);
    }
    frame.push(tag);
    write_varint(payload.len() as u64, &mut frame);
    frame.extend_from_slice(payload);
    frame
}

fn heartbeat(last_stream_id_received: Option<u64>) -> Vec<u8> {
    let mut payload = Vec::with_capacity(12);
    if let Some(stream_id) = last_stream_id_received {
        write_varint_field(2, stream_id, &mut payload);
    }
    payload
}

pub fn encode_heartbeat_ack(last_stream_id_received: Option<u64>) -> Vec<u8> {
    write_frame(
        false,
        TAG_HEARTBEAT_ACK,
        &heartbeat(last_stream_id_received),
    )
}

pub fn encode_heartbeat_ping(last_stream_id_received: Option<u64>) -> Vec<u8> {
    write_frame(
        false,
        TAG_HEARTBEAT_PING,
        &heartbeat(last_stream_id_received),
    )
}

pub fn encode_close() -> Vec<u8> {
    write_frame(false, TAG_CLOSE, &[])
}

pub fn encode_stream_ack(last_stream_id_received: Option<u64>) -> Vec<u8> {
    let mut payload = Vec::with_capacity(24);
    write_varint_field(2, IQ_TYPE_SET, &mut payload);
    write_string(3, "", &mut payload);
    let mut extension = Vec::with_capacity(8);
    write_varint_field(1, STREAM_ACK_EXTENSION_ID, &mut extension);
    write_bytes(2, &[], &mut extension);
    write_bytes(7, &extension, &mut payload);
    if let Some(stream_id) = last_stream_id_received {
        write_varint_field(10, stream_id, &mut payload);
    }
    write_frame(false, TAG_IQ_STANZA, &payload)
}

pub fn encode_login_request(
    android_id: i64,
    security_token: u64,
    received_persistent_ids: &[String],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(256 + received_persistent_ids.len() * 32);
    write_string(
        1,
        &format!("chrome-{}", env!("CARGO_PKG_VERSION")),
        &mut payload,
    );
    write_string(2, "mcs.android.com", &mut payload);
    write_string(3, &android_id.to_string(), &mut payload);
    write_string(4, &android_id.to_string(), &mut payload);
    write_string(5, &security_token.to_string(), &mut payload);
    write_string(6, &format!("android-{android_id:x}"), &mut payload);
    write_setting("new_vc", "1", &mut payload);
    for id in received_persistent_ids.iter().take(64) {
        write_string(10, id, &mut payload);
    }
    write_bool(12, false, &mut payload);
    write_bool(14, true, &mut payload);
    write_varint_field(16, AUTH_SERVICE_ANDROID_ID, &mut payload);
    write_varint_field(17, 1, &mut payload);
    write_frame(true, TAG_LOGIN_REQUEST, &payload)
}

pub fn decode_frame(tag: u8, payload: &[u8]) -> Result<Frame, PushError> {
    Ok(match tag {
        TAG_HEARTBEAT_PING => Frame::HeartbeatPing,
        TAG_HEARTBEAT_ACK => Frame::HeartbeatAck,
        TAG_CLOSE => Frame::Close,
        TAG_LOGIN_RESPONSE => Frame::LoginResponse(decode_login_response(payload)?),
        TAG_DATA_MESSAGE_STANZA => Frame::DataMessage(Box::new(decode_data_message(payload)?)),
        _ => Frame::Other,
    })
}

fn decode_login_response(payload: &[u8]) -> Result<LoginResponse, PushError> {
    let mut reader = Reader::new(payload);
    let mut response = LoginResponse {
        error_code: None,
        heartbeat_interval_ms: None,
    };
    while let Some((field, wire)) = reader.next_tag()? {
        match (field, wire) {
            (3, WIRE_LENGTH) => {
                let nested = reader.take_length()?;
                let mut error = Reader::new(nested);
                while let Some((inner, inner_wire)) = error.next_tag()? {
                    if inner == 1 && inner_wire == WIRE_VARINT {
                        response.error_code = Some(error.read_varint()? as i32);
                    } else {
                        error.skip(inner_wire)?;
                    }
                }
            }
            (7, WIRE_LENGTH) => {
                let nested = reader.take_length()?;
                let mut config = Reader::new(nested);
                while let Some((inner, inner_wire)) = config.next_tag()? {
                    if inner == 3 && inner_wire == WIRE_VARINT {
                        response.heartbeat_interval_ms = Some(config.read_varint()? as i32);
                    } else {
                        config.skip(inner_wire)?;
                    }
                }
            }
            _ => reader.skip(wire)?,
        }
    }
    Ok(response)
}

fn decode_data_message(payload: &[u8]) -> Result<DataMessage, PushError> {
    let mut reader = Reader::new(payload);
    let mut message = DataMessage {
        from: String::new(),
        category: String::new(),
        app_data: Vec::new(),
        persistent_id: None,
        raw_data: None,
        immediate_ack: false,
    };
    while let Some((field, wire)) = reader.next_tag()? {
        match (field, wire) {
            (3, WIRE_LENGTH) => message.from = reader.read_string()?,
            (5, WIRE_LENGTH) => message.category = reader.read_string()?,
            (7, WIRE_LENGTH) => {
                let nested = reader.take_length()?;
                let mut app = Reader::new(nested);
                let mut key = String::new();
                let mut value = String::new();
                while let Some((inner, inner_wire)) = app.next_tag()? {
                    match (inner, inner_wire) {
                        (1, WIRE_LENGTH) => key = app.read_string()?,
                        (2, WIRE_LENGTH) => value = app.read_string()?,
                        _ => app.skip(inner_wire)?,
                    }
                }
                if !key.is_empty() && message.app_data.len() < 128 {
                    message.app_data.push((key, value));
                }
            }
            (9, WIRE_LENGTH) => message.persistent_id = Some(reader.read_string()?),
            (21, WIRE_LENGTH) => message.raw_data = Some(reader.take_length()?.to_vec()),
            (24, WIRE_VARINT) => message.immediate_ack = reader.read_varint()? != 0,
            _ => reader.skip(wire)?,
        }
    }
    Ok(message)
}

pub fn encode_checkin_request(request: &CheckinRequest) -> Vec<u8> {
    let mut payload = Vec::with_capacity(128);
    if let Some(id) = request.android_id {
        write_varint_field(2, id as u64, &mut payload);
    }
    let mut checkin = Vec::with_capacity(48);
    write_varint_field(12, 3, &mut checkin);
    let mut chrome_build = Vec::with_capacity(32);
    write_varint_field(1, 3, &mut chrome_build);
    write_string(2, "63.0.3234.0", &mut chrome_build);
    write_varint_field(3, 1, &mut chrome_build);
    write_bytes(13, &chrome_build, &mut checkin);
    write_bytes(4, &checkin, &mut payload);
    write_varint_field(14, 3, &mut payload);
    write_varint_field(22, 0, &mut payload);
    if let Some(token) = request.security_token {
        write_fixed64(13, token, &mut payload);
    }
    payload
}

pub fn decode_checkin_response(payload: &[u8]) -> Result<CheckinResponse, PushError> {
    let mut reader = Reader::new(payload);
    let mut response = CheckinResponse {
        android_id: None,
        security_token: None,
    };
    while let Some((field, wire)) = reader.next_tag()? {
        match (field, wire) {
            (7, WIRE_FIXED64) => response.android_id = Some(reader.read_fixed64()?),
            (8, WIRE_FIXED64) => response.security_token = Some(reader.read_fixed64()?),
            _ => reader.skip(wire)?,
        }
    }
    Ok(response)
}

pub struct FrameReader {
    buffer: Vec<u8>,
    expects_version: bool,
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            expects_version: true,
        }
    }

    pub fn extend(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next_frame(&mut self, maximum: usize) -> Result<Option<(u8, Vec<u8>)>, PushError> {
        let mut position = 0;
        if self.expects_version {
            if self.buffer.len() < 2 {
                return Ok(None);
            }
            let version = self.buffer[0];
            if version < MCS_VERSION && version != 38 {
                return Err(PushError::new(
                    "push_protocol_version",
                    "The push service reported an unsupported protocol version",
                ));
            }
            position = 1;
        }
        if self.buffer.len() <= position {
            return Ok(None);
        }
        let tag = self.buffer[position];
        position += 1;
        let mut size = 0_u64;
        let mut shift = 0;
        let mut header = 0;
        loop {
            let Some(byte) = self.buffer.get(position + header) else {
                return Ok(None);
            };
            if header >= 5 {
                return Err(PushError::new(
                    "push_protocol_frame",
                    "The push service sent an invalid frame size",
                ));
            }
            size |= u64::from(byte & 0x7f) << shift;
            header += 1;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        if size as usize > maximum {
            return Err(PushError::new(
                "push_protocol_frame",
                "The push service sent an oversized frame",
            ));
        }
        let start = position + header;
        let end = start + size as usize;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let payload = self.buffer[start..end].to_vec();
        self.buffer.drain(..end);
        self.expects_version = false;
        Ok(Some((tag, payload)))
    }
}

const WIRE_VARINT: u64 = 0;
const WIRE_FIXED64: u64 = 1;
const WIRE_LENGTH: u64 = 2;

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn next_tag(&mut self) -> Result<Option<(u64, u64)>, PushError> {
        if self.position >= self.data.len() {
            return Ok(None);
        }
        let tag = self.read_varint()?;
        Ok(Some((tag >> 3, tag & 0x7)))
    }

    fn read_varint(&mut self) -> Result<u64, PushError> {
        let mut value = 0_u64;
        let mut shift = 0;
        loop {
            let byte = *self.data.get(self.position).ok_or_else(|| {
                PushError::new("push_protocol_frame", "The frame ended unexpectedly")
            })?;
            self.position += 1;
            if shift >= 64 {
                return Err(PushError::new(
                    "push_protocol_frame",
                    "The frame carried an oversized integer",
                ));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn read_fixed64(&mut self) -> Result<u64, PushError> {
        let end = self.position + 8;
        let bytes = self
            .data
            .get(self.position..end)
            .ok_or_else(|| PushError::new("push_protocol_frame", "The frame ended unexpectedly"))?;
        self.position = end;
        Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
            PushError::new(
                "push_protocol_frame",
                "The frame carried an invalid integer",
            )
        })?))
    }

    fn take_length(&mut self) -> Result<&'a [u8], PushError> {
        let length = self.read_varint()? as usize;
        if length > self.data.len().saturating_sub(self.position) {
            return Err(PushError::new(
                "push_protocol_frame",
                "The frame carried an oversized field",
            ));
        }
        let end = self.position + length;
        let slice = &self.data[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn read_string(&mut self) -> Result<String, PushError> {
        String::from_utf8(self.take_length()?.to_vec())
            .map_err(|_| PushError::new("push_protocol_frame", "The frame carried invalid text"))
    }

    fn skip(&mut self, wire: u64) -> Result<(), PushError> {
        match wire {
            WIRE_VARINT => {
                self.read_varint()?;
            }
            WIRE_FIXED64 => {
                self.read_fixed64()?;
            }
            WIRE_LENGTH => {
                self.take_length()?;
            }
            _ => {
                return Err(PushError::new(
                    "push_protocol_frame",
                    "The frame used an unsupported field type",
                ));
            }
        }
        Ok(())
    }
}

fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn write_tag(field: u64, wire: u64, out: &mut Vec<u8>) {
    write_varint((field << 3) | wire, out);
}

fn write_varint_field(field: u64, value: u64, out: &mut Vec<u8>) {
    write_tag(field, WIRE_VARINT, out);
    write_varint(value, out);
}

fn write_bool(field: u64, value: bool, out: &mut Vec<u8>) {
    write_varint_field(field, u64::from(value), out);
}

fn write_bytes(field: u64, value: &[u8], out: &mut Vec<u8>) {
    write_tag(field, WIRE_LENGTH, out);
    write_varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

fn write_string(field: u64, value: &str, out: &mut Vec<u8>) {
    write_bytes(field, value.as_bytes(), out);
}

fn write_fixed64(field: u64, value: u64, out: &mut Vec<u8>) {
    write_tag(field, WIRE_FIXED64, out);
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_setting(name: &str, value: &str, out: &mut Vec<u8>) {
    let mut setting = Vec::with_capacity(name.len() + value.len() + 8);
    write_string(1, name, &mut setting);
    write_string(2, value, &mut setting);
    write_bytes(8, &setting, out);
}
