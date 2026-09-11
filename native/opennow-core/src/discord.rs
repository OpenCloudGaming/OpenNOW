use rand::RngCore;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CLIENT_ID: &str = "1479944467112001669";
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct DiscordService {
    last_signature: Arc<Mutex<Option<String>>>,
    exchange_running: Arc<AtomicBool>,
}

impl DiscordService {
    pub fn new() -> Self {
        Self {
            last_signature: Arc::new(Mutex::new(None)),
            exchange_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn sync(&self, params: &Value) -> Result<Value, String> {
        if params["enabled"].as_bool() != Some(true) {
            return self.clear();
        }
        let activity = activity_payload(params)?;
        let signature = serde_json::to_string(&activity).map_err(|error| error.to_string())?;
        if self
            .last_signature
            .lock()
            .expect("discord state poisoned")
            .as_deref()
            == Some(&signature)
        {
            return Ok(json!({"connected":true,"unchanged":true}));
        }
        self.exchange(Some(activity), Some(signature))
    }

    pub fn clear(&self) -> Result<Value, String> {
        *self.last_signature.lock().expect("discord state poisoned") = None;
        self.exchange(None, None)
    }

    /// Runs the connect/handshake/SET_ACTIVITY round trip off the caller's
    /// thread and gives up on it after `EXCHANGE_TIMEOUT`.
    ///
    /// A Windows named pipe opened as a file has no read deadline, so a Discord
    /// client that accepts the connection and then stalls would otherwise block
    /// the caller indefinitely — session teardown has been observed waiting 30
    /// seconds on this. Presence is cosmetic, so an exchange that overruns is
    /// abandoned and left to drain in the background. Only one runs at a time,
    /// so a wedged Discord costs one parked thread rather than one per update.
    fn exchange(
        &self,
        activity: Option<Value>,
        signature: Option<String>,
    ) -> Result<Value, String> {
        if self.exchange_running.swap(true, Ordering::SeqCst) {
            return Ok(json!({"connected":false,"busy":true}));
        }
        let guard = RunningGuard(Arc::clone(&self.exchange_running));
        let last_signature = Arc::clone(&self.last_signature);
        let (sender, receiver) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("opennow-discord".to_owned())
            .spawn(move || {
                let _guard = guard;
                let _ = sender.send(run_exchange(activity, signature, &last_signature));
            });
        if let Err(error) = spawned {
            self.exchange_running.store(false, Ordering::SeqCst);
            return Err(error.to_string());
        }
        match receiver.recv_timeout(EXCHANGE_TIMEOUT) {
            Ok(result) => result,
            Err(_) => Ok(json!({"connected":false,"timedOut":true})),
        }
    }
}

/// Clears the in-flight marker even if the worker panics mid-exchange.
struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn run_exchange(
    activity: Option<Value>,
    signature: Option<String>,
    last_signature: &Mutex<Option<String>>,
) -> Result<Value, String> {
    let clearing = activity.is_none();
    let Ok(mut stream) = connect() else {
        return Ok(if clearing {
            json!({"connected":false,"cleared":true})
        } else {
            json!({"connected":false,"message":"Discord is not running"})
        });
    };
    handshake(&mut stream)?;
    command(&mut stream, activity)?;
    if let Some(signature) = signature {
        *last_signature.lock().expect("discord state poisoned") = Some(signature);
    }
    Ok(if clearing {
        json!({"connected":true,"cleared":true})
    } else {
        json!({"connected":true,"unchanged":false})
    })
}

fn activity_payload(params: &Value) -> Result<Value, String> {
    let game_name = params["gameName"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Discord activity requires a game name".to_owned())?;
    let kind = params["kind"].as_str().unwrap_or("streaming");
    let state = match kind {
        "queued" => params["queuePosition"]
            .as_u64()
            .filter(|value| *value > 0)
            .map(|value| format!("In queue (#{value})"))
            .unwrap_or_else(|| "In queue".to_owned()),
        "starting" => "Starting stream".to_owned(),
        "streaming" => "Streaming via OpenNOW".to_owned(),
        _ => return Err("Unsupported Discord activity kind".to_owned()),
    };
    let mut activity = json!({
        "details": bounded(game_name, 128),
        "state": state,
        "instance": false
    });
    if kind == "streaming"
        && let Some(timestamp) = params["startTimestampMs"].as_u64()
    {
        activity["timestamps"] = json!({"start":timestamp / 1000});
    }
    if let Some(image) = params["gameImageUrl"].as_str().filter(|value| {
        value.starts_with("https://") && value.len() <= 2_048 && !value.contains(['\r', '\n'])
    }) {
        activity["assets"] = json!({
            "large_image": image,
            "large_text": bounded(game_name, 128)
        });
    }
    Ok(activity)
}

fn handshake(stream: &mut IpcStream) -> Result<(), String> {
    write_frame(stream, 0, &json!({"v":1,"client_id":CLIENT_ID}))?;
    let (_, response) = read_frame(stream)?;
    if response["evt"] == "ERROR" {
        return Err("Discord rejected the RPC handshake".to_owned());
    }
    Ok(())
}

fn command(stream: &mut IpcStream, activity: Option<Value>) -> Result<(), String> {
    let mut nonce = [0_u8; 12];
    rand::rng().fill_bytes(&mut nonce);
    let nonce = nonce
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect::<String>();
    write_frame(
        stream,
        1,
        &json!({
            "cmd":"SET_ACTIVITY",
            "args":{"pid":std::process::id(),"activity":activity},
            "nonce":nonce
        }),
    )?;
    let (_, response) = read_frame(stream)?;
    if response["evt"] == "ERROR" {
        return Err("Discord rejected the activity update".to_owned());
    }
    Ok(())
}

fn write_frame(stream: &mut impl Write, opcode: u32, value: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if body.len() > MAX_FRAME_BYTES {
        return Err("Discord RPC frame is too large".to_owned());
    }
    stream
        .write_all(&opcode.to_le_bytes())
        .and_then(|_| stream.write_all(&(body.len() as u32).to_le_bytes()))
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| error.to_string())
}

fn read_frame(stream: &mut impl Read) -> Result<(u32, Value), String> {
    let mut header = [0_u8; 8];
    stream
        .read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    let opcode = u32::from_le_bytes(header[..4].try_into().expect("four-byte opcode"));
    let length = u32::from_le_bytes(header[4..].try_into().expect("four-byte length")) as usize;
    if length > MAX_FRAME_BYTES {
        return Err("Discord RPC response is too large".to_owned());
    }
    let mut body = vec![0_u8; length];
    stream
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    let value =
        serde_json::from_slice(&body).map_err(|_| "Invalid Discord RPC response".to_owned())?;
    Ok((opcode, value))
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(unix)]
type IpcStream = std::os::unix::net::UnixStream;

#[cfg(unix)]
fn connect() -> io::Result<IpcStream> {
    use std::env;
    use std::path::PathBuf;
    let mut directories = Vec::new();
    for name in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(path) = env::var_os(name).map(PathBuf::from)
            && !directories.contains(&path)
        {
            directories.push(path);
        }
    }
    directories.push(PathBuf::from("/tmp"));
    for directory in directories {
        for index in 0..10 {
            let path = directory.join(format!("discord-ipc-{index}"));
            if let Ok(stream) = IpcStream::connect(path) {
                stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
                stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;
                return Ok(stream);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord IPC unavailable",
    ))
}

#[cfg(windows)]
type IpcStream = std::fs::File;

#[cfg(windows)]
fn connect() -> io::Result<IpcStream> {
    for index in 0..10 {
        let path = format!(r"\\?\pipe\discord-ipc-{index}");
        if let Ok(stream) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            return Ok(stream);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord IPC unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn activity_payload_is_bounded_and_privacy_scoped() {
        let payload = activity_payload(&json!({
            "gameName":"Example",
            "kind":"queued",
            "queuePosition":14,
            "gameImageUrl":"http://unsafe.example/image"
        }))
        .unwrap();
        assert_eq!(payload["state"], "In queue (#14)");
        assert!(payload.get("assets").is_none());
        assert!(activity_payload(&json!({"gameName":"X","kind":"unknown"})).is_err());
    }

    #[test]
    fn an_exchange_is_refused_while_another_is_in_flight() {
        let service = DiscordService::new();
        service.exchange_running.store(true, Ordering::SeqCst);
        assert_eq!(service.clear().unwrap()["busy"], true);
    }

    #[test]
    fn a_panicking_exchange_still_releases_the_in_flight_marker() {
        let marker = Arc::new(AtomicBool::new(true));
        drop(RunningGuard(Arc::clone(&marker)));
        assert!(!marker.load(Ordering::SeqCst));
    }

    #[test]
    fn discord_frames_are_little_endian_and_bounded() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, 1, &json!({"evt":"READY"})).unwrap();
        let (opcode, body) = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(opcode, 1);
        assert_eq!(body["evt"], "READY");
    }
}
