use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use libloading::Library;
use serde_json::Value;

use crate::{Error, Result, Subsystem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutputDevice {
    pub id: String,
    pub name: String,
}

pub fn audio_output_devices() -> Result<Vec<AudioOutputDevice>> {
    listed_output_devices(pipewire_sinks(), alsa_devices)
}

/// Devices offered for selection. Opening a saved device validates against the
/// unfiltered `pipewire_devices` and `alsa_devices` lists instead.
fn listed_output_devices(
    pipewire: Result<Vec<PipeWireSink>>,
    alsa_devices: impl FnOnce() -> Result<Vec<AudioOutputDevice>>,
) -> Result<Vec<AudioOutputDevice>> {
    // A raw ALSA PCM opens the hardware underneath a running PipeWire graph,
    // which then cannot use that device for any application, so ALSA is only
    // offered when PipeWire has no sinks to list.
    let alsa = match &pipewire {
        Ok(sinks) if !sinks.is_empty() => Ok(Vec::new()),
        _ => alsa_devices(),
    };
    if let (Err(pipewire), Err(alsa)) = (&pipewire, &alsa) {
        return Err(Error::unavailable(
            Subsystem::Session,
            format!("Audio enumeration failed: {pipewire}; {alsa}"),
        ));
    }
    let devices = pipewire
        .unwrap_or_default()
        .into_iter()
        .filter(|sink| sink.port_available)
        .map(|sink| sink.device)
        .chain(alsa.unwrap_or_default())
        .collect();
    Ok(unique_devices(devices))
}

fn unique_devices(devices: Vec<AudioOutputDevice>) -> Vec<AudioOutputDevice> {
    let mut counts = HashMap::new();
    for device in &devices {
        *counts.entry(device.id.clone()).or_insert(0) += 1;
    }
    devices
        .into_iter()
        .filter(|device| counts[&device.id] == 1)
        .collect()
}

pub(crate) fn require_device(id: &str, devices: &[AudioOutputDevice]) -> Result<()> {
    if devices.iter().filter(|device| device.id == id).count() != 1 {
        return Err(Error::unavailable(
            Subsystem::Session,
            "The selected audio output device is unavailable or ambiguous. Select another device or System default.",
        ));
    }
    Ok(())
}

pub(crate) fn pipewire_devices() -> Result<Vec<AudioOutputDevice>> {
    Ok(pipewire_sinks()?
        .into_iter()
        .map(|sink| sink.device)
        .collect())
}

fn pipewire_sinks() -> Result<Vec<PipeWireSink>> {
    let bytes = bounded_command_output(&mut Command::new("pw-dump"), Duration::from_millis(1500))?;
    parse_pipewire_sinks(&bytes)
}

struct PipeWireSink {
    device: AudioOutputDevice,
    /// False when the sink's card port reports `available: no`, such as an
    /// HDMI output with no display attached. Sinks without a card port, and
    /// ports that cannot detect a connection, count as available.
    port_available: bool,
}

pub(crate) fn bounded_command_output(command: &mut Command, timeout: Duration) -> Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::io(Subsystem::PipeWire, error))?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::backend(Subsystem::PipeWire, "Audio query stdout unavailable"))?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(Error::io(Subsystem::PipeWire, io::Error::last_os_error()));
        }
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            if Instant::now() >= deadline {
                return Err(Error::unavailable(
                    Subsystem::PipeWire,
                    "Audio query timed out",
                ));
            }
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len() + count > 4 * 1024 * 1024 {
                        return Err(Error::unavailable(
                            Subsystem::PipeWire,
                            "Audio query exceeded the 4 MiB limit",
                        ));
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(Error::io(Subsystem::PipeWire, error)),
            }
        }
        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| Error::io(Subsystem::PipeWire, error))?
            {
                if !status.success() {
                    return Err(Error::unavailable(
                        Subsystem::PipeWire,
                        format!("Audio query failed: {status}"),
                    ));
                }
                return Ok(bytes);
            }
            if Instant::now() >= deadline {
                return Err(Error::unavailable(
                    Subsystem::PipeWire,
                    "Audio query timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
fn parse_pipewire_devices(bytes: &[u8]) -> Result<Vec<AudioOutputDevice>> {
    Ok(parse_pipewire_sinks(bytes)?
        .into_iter()
        .map(|sink| sink.device)
        .collect())
}

/// Whether the output port behind a card sink can currently play. The sink's
/// `card.profile.device` is listed in the `devices` of its card's output routes.
fn sink_port_available(objects: &[Value], props: &Value) -> bool {
    let (Some(card), Some(profile_device)) = (
        props["device.id"].as_u64(),
        props["card.profile.device"].as_u64(),
    ) else {
        return true;
    };
    let Some(params) = objects
        .iter()
        .find(|object| {
            object["type"] == "PipeWire:Interface:Device" && object["id"].as_u64() == Some(card)
        })
        .map(|device| &device["info"]["params"])
    else {
        return true;
    };
    let routes = params["EnumRoute"]
        .as_array()
        .or_else(|| params["Route"].as_array());
    !routes.into_iter().flatten().any(|route| {
        route["direction"] == "Output"
            && route["available"] == "no"
            && route["devices"].as_array().is_some_and(|devices| {
                devices
                    .iter()
                    .any(|device| device.as_u64() == Some(profile_device))
            })
    })
}

fn parse_pipewire_sinks(bytes: &[u8]) -> Result<Vec<PipeWireSink>> {
    let objects: Vec<Value> = serde_json::from_slice(bytes).map_err(|error| {
        Error::backend(
            Subsystem::PipeWire,
            format!("Invalid pw-dump response: {error}"),
        )
    })?;
    let mut devices = Vec::new();
    for object in &objects {
        if object["type"] != "PipeWire:Interface:Node"
            || object["info"]["props"]["media.class"] != "Audio/Sink"
        {
            continue;
        }
        let props = &object["info"]["props"];
        let Some(node_name) = props["node.name"].as_str() else {
            continue;
        };
        let id = format!("pipewire:{node_name}");
        if node_name.is_empty()
            || matches!(node_name, "auto" | "0")
            || node_name.parse::<u64>().is_ok()
            || !valid_id(&id)
        {
            continue;
        }
        let name = props["node.description"]
            .as_str()
            .or_else(|| props["node.nick"].as_str())
            .unwrap_or(node_name);
        devices.push(PipeWireSink {
            device: AudioOutputDevice {
                id,
                name: name.chars().take(1024).collect(),
            },
            port_available: sink_port_available(&objects, props),
        });
        if devices.len() > 1024 {
            return Err(Error::unavailable(
                Subsystem::PipeWire,
                "Too many audio output devices",
            ));
        }
    }
    Ok(devices)
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 1024 && !id.contains('\0')
}

pub(crate) fn alsa_devices() -> Result<Vec<AudioOutputDevice>> {
    type Hint = unsafe extern "C" fn(c_int, *const c_char, *mut *mut *mut c_void) -> c_int;
    type Get = unsafe extern "C" fn(*const c_void, *const c_char) -> *mut c_char;
    type Free = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
    unsafe {
        let library = Library::new("libasound.so.2")
            .or_else(|_| Library::new("libasound.so"))
            .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))?;
        let hint: Hint = *library
            .get(b"snd_device_name_hint\0")
            .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))?;
        let get: Get = *library
            .get(b"snd_device_name_get_hint\0")
            .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))?;
        let free: Free = *library
            .get(b"snd_device_name_free_hint\0")
            .map_err(|error| Error::unavailable(Subsystem::Alsa, error.to_string()))?;
        let mut hints = std::ptr::null_mut();
        let status = hint(-1, c"pcm".as_ptr(), &mut hints);
        if status < 0 {
            return Err(Error::unavailable(
                Subsystem::Alsa,
                format!("ALSA device enumeration failed: {status}"),
            ));
        }
        if hints.is_null() {
            return Ok(Vec::new());
        }
        let result = (|| {
            let mut devices = Vec::new();
            for index in 0..4096 {
                let entry = *hints.add(index);
                if entry.is_null() {
                    return Ok(devices);
                }
                let read = |key: &CStr| {
                    let value = get(entry, key.as_ptr());
                    if value.is_null() {
                        return None;
                    }
                    let text = CStr::from_ptr(value).to_str().ok().map(str::to_owned);
                    libc::free(value.cast());
                    text
                };
                if read(c"IOID").as_deref() == Some("Input") {
                    continue;
                }
                let Some(pcm) = read(c"NAME") else { continue };
                if matches!(
                    pcm.as_str(),
                    "null" | "default" | "sysdefault" | "pipewire" | "pulse"
                ) {
                    continue;
                }
                let id = format!("alsa:{pcm}");
                if !valid_id(&id) || pcm.is_empty() {
                    continue;
                }
                let description = read(c"DESC")
                    .unwrap_or_else(|| pcm.clone())
                    .replace('\n', " — ");
                devices.push(AudioOutputDevice {
                    id,
                    name: format!(
                        "{} (ALSA)",
                        description.chars().take(1000).collect::<String>()
                    ),
                });
                if devices.len() > 1024 {
                    return Err(Error::unavailable(
                        Subsystem::Alsa,
                        "Too many audio output devices",
                    ));
                }
            }
            Err(Error::unavailable(
                Subsystem::Alsa,
                "Too many ALSA PCM hints",
            ))
        })();
        free(hints);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audio_query_timeout_and_output_limit_are_bounded() {
        let started = Instant::now();
        let error =
            bounded_command_output(Command::new("sleep").arg("10"), Duration::from_millis(25))
                .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let error = bounded_command_output(
            Command::new("head").args(["-c", "4194305", "/dev/zero"]),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(error.to_string().contains("4 MiB"));
    }

    #[test]
    fn pipewire_ids_use_names_not_transient_numbers_and_only_sinks() {
        let node = |class: &str, name: &str| json!({"id": 42, "type": "PipeWire:Interface:Node", "info": {"props": {"media.class": class, "node.name": name, "node.description": "Speakers"}}});
        let devices = parse_pipewire_devices(
            &serde_json::to_vec(&json!([
                node("Audio/Sink", "alsa_output.usb-speakers"),
                node("Audio/Source", "mic"),
                node("Audio/Sink", "42"),
                node("Audio/Sink", "auto")
            ]))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            devices,
            vec![AudioOutputDevice {
                id: "pipewire:alsa_output.usb-speakers".to_owned(),
                name: "Speakers".to_owned()
            }]
        );
        assert!(require_device("pipewire:missing", &devices).is_err());
        assert!(require_device(&devices[0].id, &devices).is_ok());
        assert!(require_device(&devices[0].id, &[devices[0].clone(), devices[0].clone()]).is_err());
        assert!(unique_devices(vec![devices[0].clone(), devices[0].clone()]).is_empty());
    }

    /// One SOF laptop card with a speaker and two HDMI sinks, plus a Bluetooth
    /// sink that has no card routes.
    fn laptop_dump() -> Vec<u8> {
        let sink = |name: &str, profile_device: Option<u64>| {
            let mut props =
                json!({"media.class": "Audio/Sink", "node.name": name, "node.description": name});
            if let Some(profile_device) = profile_device {
                props["device.id"] = json!(80);
                props["card.profile.device"] = json!(profile_device);
            }
            json!({"id": 100, "type": "PipeWire:Interface:Node", "info": {"props": props}})
        };
        let route = |name: &str, available: &str, device: u64| json!({"direction": "Output", "name": name, "available": available, "devices": [device]});
        serde_json::to_vec(&json!([
            {"id": 80, "type": "PipeWire:Interface:Device", "info": {"params": {"EnumRoute": [
                route("[Out] HDMI2", "no", 1),
                route("[Out] HDMI1", "yes", 2),
                route("[Out] Speaker", "unknown", 3),
                {"direction": "Input", "name": "[In] Mic1", "available": "no", "devices": [3]},
            ]}}},
            sink("hdmi2", Some(1)),
            sink("hdmi1", Some(2)),
            sink("speaker", Some(3)),
            sink("bluez_output.headset", None),
        ]))
        .unwrap()
    }

    fn ids(devices: &[AudioOutputDevice]) -> Vec<&str> {
        devices.iter().map(|device| device.id.as_str()).collect()
    }

    #[test]
    fn disconnected_ports_are_not_listed_but_still_validate_a_saved_choice() {
        let listed = listed_output_devices(
            parse_pipewire_sinks(&laptop_dump()),
            || -> Result<Vec<AudioOutputDevice>> {
                panic!("ALSA is not enumerated while PipeWire lists sinks")
            },
        )
        .unwrap();
        assert_eq!(
            ids(&listed),
            [
                "pipewire:hdmi1",
                "pipewire:speaker",
                "pipewire:bluez_output.headset"
            ]
        );

        let all = parse_pipewire_devices(&laptop_dump()).unwrap();
        assert!(require_device("pipewire:hdmi2", &all).is_ok());
    }

    #[test]
    fn raw_alsa_pcms_are_offered_only_without_pipewire_sinks() {
        let alsa = || {
            Ok(vec![AudioOutputDevice {
                id: "alsa:sysdefault:CARD=sofhdadsp".to_owned(),
                name: "sof-hda-dsp (ALSA)".to_owned(),
            }])
        };
        for pipewire in [
            Ok(Vec::new()),
            Err(Error::unavailable(
                Subsystem::PipeWire,
                "pw-dump is missing",
            )),
        ] {
            assert_eq!(
                ids(&listed_output_devices(pipewire, alsa).unwrap()),
                ["alsa:sysdefault:CARD=sofhdadsp"]
            );
        }
    }
}
