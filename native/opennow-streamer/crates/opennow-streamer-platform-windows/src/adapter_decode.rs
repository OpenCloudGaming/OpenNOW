use std::sync::OnceLock;

use opennow_streamer_protocol::GraphicsAdapterCapability;

/// Hardware decode profiles observed on one physical adapter.
///
/// High-performance enumeration order is preserved. Automatic selection uses
/// the first adapter that exposes any profile, so a hybrid laptop whose
/// discrete GPU cannot decode (GeForce MX110 on an Intel HD Graphics 620
/// machine, for example) falls through to the integrated GPU that can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDecodeIndex {
    pub luid: u64,
    pub name: String,
    pub h264: bool,
    pub h265: bool,
    pub h265_main10: bool,
    pub av1: bool,
    pub reason: Option<String>,
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecodeProfile {
    H264,
    H265Main,
    H265Main10,
    Av1,
    Other,
}

impl AdapterDecodeIndex {
    #[cfg(any(windows, test))]
    pub(crate) fn named(luid: u64, name: impl Into<String>) -> Self {
        Self {
            luid,
            name: sanitize_adapter_name(&name.into()),
            h264: false,
            h265: false,
            h265_main10: false,
            av1: false,
            reason: None,
        }
    }

    pub fn can_decode(&self) -> bool {
        self.h264 || self.h265 || self.av1
    }

    pub fn codec_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.h264 {
            names.push("h264");
        }
        if self.h265 {
            names.push("h265");
        }
        if self.av1 {
            names.push("av1");
        }
        names
    }

    #[cfg(any(windows, test))]
    pub(crate) fn apply_profile(&mut self, profile: DecodeProfile, supported: bool) {
        if !supported {
            return;
        }
        match profile {
            DecodeProfile::H264 => self.h264 = true,
            DecodeProfile::H265Main => self.h265 = true,
            DecodeProfile::H265Main10 => {
                self.h265 = true;
                self.h265_main10 = true;
            }
            DecodeProfile::Av1 => self.av1 = true,
            DecodeProfile::Other => {}
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn finish(&mut self) {
        if self.reason.is_none() && !self.can_decode() {
            self.reason = Some("no supported hardware decoder profile".to_owned());
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) fn sanitize_adapter_name(value: &str) -> String {
    let mut cleaned = String::new();
    for character in value.chars().take(128) {
        if character.is_control() {
            cleaned.push(' ');
        } else {
            cleaned.push(character);
        }
    }
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        "GPU".to_owned()
    } else {
        cleaned
    }
}

pub(crate) fn report_adapter_decode(
    adapters: Vec<AdapterDecodeIndex>,
    active_luid: Option<u64>,
) -> Vec<GraphicsAdapterCapability> {
    adapters
        .into_iter()
        .take(8)
        .map(|adapter| {
            let codecs = adapter
                .codec_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            GraphicsAdapterCapability {
                name: adapter.name,
                active: active_luid == Some(adapter.luid),
                codecs,
                h265_main10: adapter.h265_main10,
                reason: adapter.reason,
            }
        })
        .collect()
}

/// Indexes every physical adapter's hardware decode profiles once per process.
pub fn graphics_adapter_capabilities(active_luid: Option<u64>) -> Vec<GraphicsAdapterCapability> {
    report_adapter_decode(index_adapter_decode(), active_luid)
}

fn index_adapter_decode() -> Vec<AdapterDecodeIndex> {
    static CACHE: OnceLock<Vec<AdapterDecodeIndex>> = OnceLock::new();
    CACHE.get_or_init(probe_adapter_decode).clone()
}

fn probe_adapter_decode() -> Vec<AdapterDecodeIndex> {
    #[cfg(windows)]
    {
        crate::windows::probe_adapter_decode()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{AdapterDecodeIndex, DecodeProfile, report_adapter_decode, sanitize_adapter_name};

    #[test]
    fn adapter_names_keep_vendor_text_and_drop_controls() {
        assert_eq!(
            sanitize_adapter_name("Intel(R) HD Graphics 620"),
            "Intel(R) HD Graphics 620"
        );
        assert_eq!(
            sanitize_adapter_name("NVIDIA GeForce MX110\nextra"),
            "NVIDIA GeForce MX110 extra"
        );
        assert_eq!(sanitize_adapter_name(" \n\t "), "GPU");
    }

    #[test]
    fn mx110_without_profiles_is_empty_and_hd620_indexes_h264_and_h265() {
        let mut mx110 = AdapterDecodeIndex::named(2, "NVIDIA GeForce MX110");
        mx110.apply_profile(DecodeProfile::H264, false);
        mx110.apply_profile(DecodeProfile::Av1, false);
        mx110.finish();
        assert!(mx110.codec_names().is_empty());
        assert!(!mx110.can_decode());
        assert_eq!(
            mx110.reason.as_deref(),
            Some("no supported hardware decoder profile")
        );

        let mut hd620 = AdapterDecodeIndex::named(1, "Intel(R) HD Graphics 620");
        hd620.apply_profile(DecodeProfile::H264, true);
        hd620.apply_profile(DecodeProfile::H265Main, true);
        hd620.apply_profile(DecodeProfile::H265Main10, true);
        hd620.apply_profile(DecodeProfile::Av1, false);
        hd620.apply_profile(DecodeProfile::Other, true);
        hd620.finish();
        assert_eq!(hd620.codec_names(), vec!["h264", "h265"]);
        assert!(hd620.h265_main10);
        assert!(hd620.reason.is_none());

        let reported = report_adapter_decode(vec![mx110, hd620], Some(1));
        assert!(!reported[0].active);
        assert!(reported[0].codecs.is_empty());
        assert!(reported[1].active);
        assert_eq!(
            reported[1].codecs,
            vec!["h264".to_owned(), "h265".to_owned()]
        );
        assert!(reported[1].h265_main10);
    }
}
