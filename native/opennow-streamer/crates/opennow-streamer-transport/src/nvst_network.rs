fn known_vpn_adapter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "cloudflarewarp",
        "cloudflare warp",
        "wireguard",
        "wintun",
        "tap-windows",
        "openvpn",
        "tailscale",
        "zerotier",
        "nordlynx",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn numbered_interface(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix).is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

#[cfg(target_os = "linux")]
pub(super) fn is_vpn_interface(name: &str) -> bool {
    known_vpn_adapter(name)
        || numbered_interface(name, "wg")
        || std::path::Path::new("/sys/class/net")
            .join(name)
            .join("tun_flags")
            .is_file()
}

#[cfg(target_os = "macos")]
pub(super) fn is_vpn_interface(name: &str) -> bool {
    known_vpn_adapter(name)
        || ["utun", "tun", "tap", "wg"]
            .iter()
            .any(|prefix| numbered_interface(name, prefix))
}

#[cfg(windows)]
pub(super) fn is_vpn_interface(name: &str) -> bool {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        ConvertInterfaceNameToLuidA, GetIfEntry2, MIB_IF_ROW2,
    };

    let Ok(name) = std::ffi::CString::new(name) else {
        return false;
    };
    let mut row = MIB_IF_ROW2::default();
    if unsafe { ConvertInterfaceNameToLuidA(name.as_ptr().cast(), &mut row.InterfaceLuid) } != 0
        || unsafe { GetIfEntry2(&mut row) } != 0
    {
        return false;
    }
    let end = row
        .Description
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(row.Description.len());
    known_vpn_adapter(&String::from_utf16_lossy(&row.Description[..end]))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn is_vpn_interface(_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpn_adapter_detection_recognizes_tunnel_drivers_not_generic_virtual_adapters() {
        for name in [
            "CloudflareWARP",
            "Cloudflare WARP Interface Tunnel",
            "WireGuard Tunnel",
            "Wintun Userspace Tunnel",
            "TAP-Windows Adapter V9",
            "OpenVPN Data Channel Offload",
            "Tailscale Tunnel",
            "ZeroTier Virtual Port",
            "NordLynx Tunnel",
        ] {
            assert!(known_vpn_adapter(name), "{name}");
        }
        for name in [
            "Ethernet",
            "Wi-Fi",
            "Intel Ethernet Controller",
            "Hyper-V Virtual Ethernet Adapter",
            "lo",
            "en0",
            "eth0",
            "wg-home-ethernet",
        ] {
            assert!(!known_vpn_adapter(name), "{name}");
        }
    }

    #[test]
    fn numbered_tunnel_names_require_an_exact_prefix_and_numeric_suffix() {
        assert!(numbered_interface("utun4", "utun"));
        assert!(numbered_interface("wg0", "wg"));
        assert!(!numbered_interface("wg", "wg"));
        assert!(!numbered_interface("wg-office", "wg"));
        assert!(!numbered_interface("ethernet0", "wg"));
    }
}
