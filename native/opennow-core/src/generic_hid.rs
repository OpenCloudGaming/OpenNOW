//! Generic raw-HID device policy for the session request.
//!
//! The core decides *which* allowlisted HID devices (HOTAS, wheels) a session
//! forwards and prepares everything the streamer needs to open them: it scans
//! sysfs only (never `/dev/hidraw*`), assigns request ids, and emits both the
//! CloudMatch `requestedStreamingFeatures.hidDevices` entries and the
//! `genericHidDevices` session/context field. Linux only; other targets
//! enumerate nothing.

use std::path::Path;

use serde_json::{Value, json};

/// Geronimo forwards at most three generic devices per session.
pub const MAX_DEVICES: usize = 3;
/// `flags` for CloudMatch `hidDevices` entries, mirroring the change-event flags.
pub const CLOUDMATCH_ENTRY_FLAGS: u64 = 0x0F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowEntry {
    pub vendor: u16,
    pub product: u16,
    /// Explicit USB interface from a `vvvv:pppp:ii` setting; `None` picks the
    /// lowest-numbered HID interface of that device.
    pub interface: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericHidEntry {
    pub request_id: u8,
    pub vendor: u16,
    pub product: u16,
    pub revision: u16,
    pub interface: u8,
    pub hidraw_path: String,
    pub sysfs_hid_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryShape {
    /// `{id, flags, simplifiedHidDeviceData:{vendorId, productId, revision, customId}}`.
    /// Experiment only (`OPENNOW_GENERIC_HID_ENTRY_SHAPE=nested`): no captured
    /// session request uses this shape; it mirrors the Geronimo field names
    /// seen in binary strings and nothing more.
    Nested,
    /// `{vendorId, productId, revision, customId, flags}`: the shape the
    /// official Linux client sent in its CloudMatch session request
    /// (gfn-hotas-research.md section 3b, "Session-level negotiation"). Default.
    Flat,
}

impl EntryShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nested => "nested",
            Self::Flat => "flat",
        }
    }
}

/// Whether generic HID forwarding is requested, and where that came from.
/// The only place `OPENNOW_GENERIC_HID` is read.
pub fn enabled(settings: &Value) -> (bool, &'static str) {
    if std::env::var("OPENNOW_GENERIC_HID")
        .ok()
        .is_some_and(|value| value.trim() == "1")
    {
        return (true, "env");
    }
    (
        settings["genericHidEnabled"].as_bool().unwrap_or(false),
        "setting",
    )
}

/// The only place `OPENNOW_GENERIC_HID_ENTRY_SHAPE` is read. The documented
/// flat entry is the default; `nested` is an opt-in experiment.
pub fn entry_shape() -> EntryShape {
    match std::env::var("OPENNOW_GENERIC_HID_ENTRY_SHAPE")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some(value) if value.eq_ignore_ascii_case("nested") => EntryShape::Nested,
        _ => EntryShape::Flat,
    }
}

/// `requestedStreamingFeatures.supportedHidDevices`. No generic bit is known,
/// so the default stays `0`; the only place `OPENNOW_GENERIC_HID_SUPPORTED_MASK`
/// is read.
pub fn supported_mask() -> i64 {
    std::env::var("OPENNOW_GENERIC_HID_SUPPORTED_MASK")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

/// Parse one `vvvv:pppp` or `vvvv:pppp:ii` allowlist string.
pub fn parse_allow_entry(value: &str) -> Option<AllowEntry> {
    let mut parts = value.trim().split(':');
    let vendor = u16::from_str_radix(parts.next()?, 16).ok()?;
    let product = u16::from_str_radix(parts.next()?, 16).ok()?;
    let interface = match parts.next() {
        None => None,
        Some(interface) => Some(interface.parse::<u8>().ok()?),
    };
    if parts.next().is_some() {
        return None;
    }
    Some(AllowEntry {
        vendor,
        product,
        interface,
    })
}

pub fn allowlist(settings: &Value) -> Vec<AllowEntry> {
    settings["genericHidDevices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(parse_allow_entry)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SysfsNode {
    node: String,
    vendor: u16,
    product: u16,
    revision: u16,
    interface: u8,
    hid_id: String,
    name: String,
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

/// Parse `HID_ID=0003:00000738:00002221` into (bus, vendor, product).
fn parse_hid_id(value: &str) -> Option<(u32, u16, u16)> {
    let mut parts = value.trim().split(':');
    let bus = u32::from_str_radix(parts.next()?, 16).ok()?;
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((
        bus,
        u16::try_from(vendor).ok()?,
        u16::try_from(product).ok()?,
    ))
}

fn read_node(sysfs_hidraw: &Path, node: &str) -> Option<SysfsNode> {
    let device = sysfs_hidraw.join(node).join("device");
    let uevent = std::fs::read_to_string(device.join("uevent")).ok()?;
    let mut hid_id = None;
    let mut name = String::new();
    for line in uevent.lines() {
        if let Some(value) = line.strip_prefix("HID_ID=") {
            hid_id = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("HID_NAME=") {
            name = value.trim().to_owned();
        }
    }
    let hid_id = hid_id?;
    let (_bus, vendor, product) = parse_hid_id(&hid_id)?;
    // `device` is a symlink into the USB tree: `..` is the USB interface and
    // `../..` the USB device, both resolved by the kernel after the link.
    let interface = read_trimmed(&device.join("..").join("bInterfaceNumber"))
        .and_then(|value| u8::from_str_radix(&value, 16).ok())?;
    let revision = read_trimmed(&device.join("..").join("..").join("bcdDevice"))
        .and_then(|value| u16::from_str_radix(&value, 16).ok())
        .unwrap_or(0);
    Some(SysfsNode {
        node: node.to_owned(),
        vendor,
        product,
        revision,
        interface,
        hid_id,
        name,
    })
}

fn node_index(node: &str) -> u32 {
    node.trim_start_matches("hidraw").parse().unwrap_or(u32::MAX)
}

/// Enumerate allowlisted hidraw nodes under `sysfs_hidraw` (normally
/// `/sys/class/hidraw`). Per allowlisted VID:PID the lowest HID interface is
/// kept unless the entry pins one. Results are sorted by (vendor, product,
/// interface, node) and receive request ids `0..`, at most [`MAX_DEVICES`].
pub fn enumerate_in(sysfs_hidraw: &Path, allow: &[AllowEntry]) -> Vec<GenericHidEntry> {
    let Ok(entries) = std::fs::read_dir(sysfs_hidraw) else {
        return Vec::new();
    };
    let mut nodes: Vec<SysfsNode> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|node| node.starts_with("hidraw"))
        .filter_map(|node| read_node(sysfs_hidraw, &node))
        .collect();
    nodes.sort_by_key(|node| {
        (
            node.vendor,
            node.product,
            node.interface,
            node_index(&node.node),
        )
    });
    let mut selected: Vec<SysfsNode> = Vec::new();
    for entry in allow {
        if selected
            .iter()
            .any(|node| node.vendor == entry.vendor && node.product == entry.product)
        {
            continue;
        }
        let candidate = nodes.iter().find(|node| {
            node.vendor == entry.vendor
                && node.product == entry.product
                && entry
                    .interface
                    .is_none_or(|interface| node.interface == interface)
        });
        if let Some(node) = candidate {
            selected.push(node.clone());
        }
    }
    selected.sort_by_key(|node| {
        (
            node.vendor,
            node.product,
            node.interface,
            node_index(&node.node),
        )
    });
    selected
        .into_iter()
        .take(MAX_DEVICES)
        .enumerate()
        .map(|(index, node)| GenericHidEntry {
            request_id: index as u8,
            vendor: node.vendor,
            product: node.product,
            revision: node.revision,
            interface: node.interface,
            hidraw_path: format!("/dev/{}", node.node),
            sysfs_hid_id: node.hid_id,
            name: node.name,
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub fn enumerate(allow: &[AllowEntry]) -> Vec<GenericHidEntry> {
    enumerate_in(Path::new("/sys/class/hidraw"), allow)
}

#[cfg(not(target_os = "linux"))]
pub fn enumerate(_allow: &[AllowEntry]) -> Vec<GenericHidEntry> {
    Vec::new()
}

/// The `genericHidDevices` session/context value consumed by the streamer.
pub fn session_value(entries: &[GenericHidEntry]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| {
                json!({
                    "requestId": entry.request_id,
                    "vendorId": entry.vendor,
                    "productId": entry.product,
                    "revision": entry.revision,
                    "interface": entry.interface,
                    "hidrawPath": entry.hidraw_path,
                    "sysfsHidId": entry.sysfs_hid_id,
                    "name": entry.name,
                })
            })
            .collect(),
    )
}

/// `requestedStreamingFeatures.hidDevices` built from a [`session_value`]
/// array. `Null` when nothing is listed.
pub fn cloudmatch_hid_devices(session_devices: &Value, shape: EntryShape) -> Value {
    let Some(entries) = session_devices.as_array() else {
        return Value::Null;
    };
    let devices: Vec<Value> = entries
        .iter()
        .filter_map(|entry| {
            let request_id = entry["requestId"].as_u64()?;
            let vendor = entry["vendorId"].as_u64()?;
            let product = entry["productId"].as_u64()?;
            let revision = entry["revision"].as_u64().unwrap_or(0);
            Some(match shape {
                EntryShape::Nested => json!({
                    "id": request_id,
                    "flags": CLOUDMATCH_ENTRY_FLAGS,
                    "simplifiedHidDeviceData": {
                        "vendorId": vendor,
                        "productId": product,
                        "revision": revision,
                        "customId": 0
                    }
                }),
                EntryShape::Flat => json!({
                    "vendorId": vendor,
                    "productId": product,
                    "revision": revision,
                    "customId": 0,
                    "flags": CLOUDMATCH_ENTRY_FLAGS
                }),
            })
        })
        .collect();
    if devices.is_empty() {
        Value::Null
    } else {
        Value::Array(devices)
    }
}

/// Resolve the session's generic HID devices from settings: logs the policy
/// decision and every enumerated device, and returns the session value (an
/// empty array when disabled or nothing matched).
pub fn resolve_for_session(settings: &Value) -> Value {
    let (enabled, source) = enabled(settings);
    let allow = allowlist(settings);
    eprintln!(
        "Generic HID: enabled={enabled} source={source} allowlist=[{}]",
        allow
            .iter()
            .map(|entry| match entry.interface {
                Some(interface) => format!("{:04x}:{:04x}:{interface}", entry.vendor, entry.product),
                None => format!("{:04x}:{:04x}", entry.vendor, entry.product),
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !enabled {
        return Value::Array(Vec::new());
    }
    let entries = enumerate(&allow);
    if entries.is_empty() {
        eprintln!("Generic HID: no allowlisted hidraw device present");
    }
    for entry in &entries {
        eprintln!(
            "Generic HID: enumerated requestId={} {:04x}:{:04x} rev={:04x} iface={} node={} name={:?}",
            entry.request_id,
            entry.vendor,
            entry.product,
            entry.revision,
            entry.interface,
            entry.hidraw_path,
            entry.name
        );
    }
    session_value(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_entries_parse_hex_pairs_with_an_optional_interface() {
        assert_eq!(
            parse_allow_entry("0738:2221"),
            Some(AllowEntry {
                vendor: 0x0738,
                product: 0x2221,
                interface: None
            })
        );
        assert_eq!(
            parse_allow_entry("0738:A221:2"),
            Some(AllowEntry {
                vendor: 0x0738,
                product: 0xa221,
                interface: Some(2)
            })
        );
        assert_eq!(parse_allow_entry("0738"), None);
        assert_eq!(parse_allow_entry("zzzz:2221"), None);
        assert_eq!(parse_allow_entry("0738:2221:x"), None);
        assert_eq!(parse_allow_entry("0738:2221:0:0"), None);
        let settings = json!({"genericHidDevices": ["0738:2221", "bad", 5, "044f:0402:1"]});
        assert_eq!(allowlist(&settings).len(), 2);
        assert!(allowlist(&json!({})).is_empty());
    }

    #[test]
    fn enabled_follows_the_setting_when_the_env_switch_is_absent() {
        // The env switch is process-global; only the setting path is asserted
        // here so the test is independent of the caller's environment.
        if std::env::var("OPENNOW_GENERIC_HID").ok().as_deref() != Some("1") {
            assert_eq!(enabled(&json!({"genericHidEnabled": true})), (true, "setting"));
            assert_eq!(enabled(&json!({"genericHidEnabled": false})), (false, "setting"));
            assert_eq!(enabled(&json!({})), (false, "setting"));
        }
    }

    #[cfg(unix)]
    fn fixture(root: &Path) {
        use std::os::unix::fs::symlink;
        let usb_device = root.join("devices/1-10.4");
        let bcd = usb_device.join("bcdDevice");
        std::fs::create_dir_all(&usb_device).unwrap();
        std::fs::write(&bcd, "0100\n").unwrap();
        let class = root.join("class/hidraw");
        std::fs::create_dir_all(&class).unwrap();
        let add = |node: &str, interface: &str, hid_id: &str, name: &str| {
            let usb_interface = usb_device.join(format!("1-10.4:1.{}", interface.trim_start_matches('0').chars().next().unwrap_or('0')));
            std::fs::create_dir_all(&usb_interface).unwrap();
            std::fs::write(usb_interface.join("bInterfaceNumber"), format!("{interface}\n")).unwrap();
            let hid = usb_interface.join(format!("0003:{}.{node}", &hid_id[5..]));
            std::fs::create_dir_all(&hid).unwrap();
            std::fs::write(
                hid.join("uevent"),
                format!("DRIVER=hid-generic\nHID_ID={hid_id}\nHID_NAME={name}\nHID_PHYS=usb-0000:00:14.0-10.4/input0\n"),
            )
            .unwrap();
            let class_node = class.join(node);
            std::fs::create_dir_all(&class_node).unwrap();
            symlink(&hid, class_node.join("device")).unwrap();
        };
        add("hidraw8", "00", "0003:00000738:00002221", "Mad Catz Saitek Pro Flight X-56 Rhino Stick");
        add("hidraw9", "02", "0003:00000738:00002221", "Mad Catz Saitek Pro Flight X-56 Rhino Stick");
        add("hidraw3", "03", "0003:00001038:000012E0", "SteelSeries Arctis Nova Pro Wireless");
        add("hidraw12", "00", "0003:00000738:0000A221", "Mad Catz Saitek Pro Flight X-56 Rhino Throttle");
    }

    #[cfg(unix)]
    #[test]
    fn enumeration_keeps_the_lowest_interface_and_honors_pinned_ones() {
        let temp = tempfile::tempdir().unwrap();
        fixture(temp.path());
        let sysfs = temp.path().join("class/hidraw");
        let allow = [
            parse_allow_entry("0738:2221").unwrap(),
            parse_allow_entry("0738:a221").unwrap(),
        ];
        let entries = enumerate_in(&sysfs, &allow);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].request_id, 0);
        assert_eq!(entries[0].vendor, 0x0738);
        assert_eq!(entries[0].product, 0x2221);
        assert_eq!(entries[0].interface, 0, "iface 0 (joystick) beats iface 2 (vendor page)");
        assert_eq!(entries[0].revision, 0x0100);
        assert_eq!(entries[0].hidraw_path, "/dev/hidraw8");
        assert_eq!(entries[0].sysfs_hid_id, "0003:00000738:00002221");
        assert_eq!(entries[0].name, "Mad Catz Saitek Pro Flight X-56 Rhino Stick");
        assert_eq!(entries[1].request_id, 1);
        assert_eq!(entries[1].product, 0xa221);
        assert_eq!(entries[1].hidraw_path, "/dev/hidraw12");

        let pinned = [parse_allow_entry("0738:2221:2").unwrap()];
        let entries = enumerate_in(&sysfs, &pinned);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].interface, 2);
        assert_eq!(entries[0].hidraw_path, "/dev/hidraw9");

        let absent = [parse_allow_entry("0738:2221:5").unwrap()];
        assert!(enumerate_in(&sysfs, &absent).is_empty());
        assert!(enumerate_in(&sysfs, &[]).is_empty());
        assert!(enumerate_in(&temp.path().join("missing"), &allow).is_empty());
        let duplicate = [
            parse_allow_entry("0738:2221").unwrap(),
            parse_allow_entry("0738:2221:2").unwrap(),
        ];
        assert_eq!(enumerate_in(&sysfs, &duplicate).len(), 1, "one entry per physical device");
    }

    #[test]
    fn session_and_cloudmatch_values_carry_the_x56_identity() {
        let entries = vec![GenericHidEntry {
            request_id: 0,
            vendor: 0x0738,
            product: 0x2221,
            revision: 0x0100,
            interface: 0,
            hidraw_path: "/dev/hidraw8".to_owned(),
            sysfs_hid_id: "0003:00000738:00002221".to_owned(),
            name: "X-56".to_owned(),
        }];
        let session = session_value(&entries);
        assert_eq!(
            session,
            json!([{
                "requestId": 0, "vendorId": 1848, "productId": 8737, "revision": 256,
                "interface": 0, "hidrawPath": "/dev/hidraw8",
                "sysfsHidId": "0003:00000738:00002221", "name": "X-56"
            }])
        );
        assert_eq!(
            cloudmatch_hid_devices(&session, EntryShape::Nested),
            json!([{"id":0,"flags":15,"simplifiedHidDeviceData":{"vendorId":1848,"productId":8737,"revision":256,"customId":0}}])
        );
        assert_eq!(
            cloudmatch_hid_devices(&session, EntryShape::Flat),
            json!([{"vendorId":1848,"productId":8737,"revision":256,"customId":0,"flags":15}])
        );
        assert_eq!(cloudmatch_hid_devices(&json!([]), EntryShape::Nested), Value::Null);
        assert_eq!(cloudmatch_hid_devices(&Value::Null, EntryShape::Nested), Value::Null);
        assert_eq!(cloudmatch_hid_devices(&json!([{"requestId": 1}]), EntryShape::Flat), Value::Null);
    }

    #[test]
    fn the_documented_flat_entry_shape_is_the_default() {
        if std::env::var("OPENNOW_GENERIC_HID_ENTRY_SHAPE").is_ok() {
            return;
        }
        assert_eq!(entry_shape(), EntryShape::Flat);
    }

    #[test]
    fn disabled_forwarding_resolves_to_an_empty_list_without_touching_sysfs() {
        if std::env::var("OPENNOW_GENERIC_HID").ok().as_deref() == Some("1") {
            return;
        }
        let settings = json!({"genericHidEnabled": false, "genericHidDevices": ["0738:2221"]});
        assert_eq!(resolve_for_session(&settings), json!([]));
    }
}
