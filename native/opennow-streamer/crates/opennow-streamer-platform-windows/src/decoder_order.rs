/// Where a hardware-mode decoder search looks for Media Foundation activations.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecoderCandidateSource {
    /// Hardware MFTs registered against the Qt adapter's LUID.
    AdapterHardware,
    /// Hardware MFTs that are not tagged with an adapter LUID. Intel iGPU
    /// decoders are commonly visible only in this set; skipping it selects the
    /// Microsoft software H.264 MFT instead.
    UnscopedHardware,
    /// Every registered decoder. NVIDIA and AMD expose DXVA through the
    /// Microsoft MFT, which is why this remains the last hardware-mode step.
    Registered,
}

/// Adapter-scoped hardware MFTs win when any exist. Otherwise try unscoped
/// hardware MFTs before the registered software/DXVA fallback. Trying the
/// registered list first on an Intel iGPU sticks playback on the software
/// decoder, which cannot sustain the stream and then stalls.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn decoder_candidate_sources(
    adapter_hardware_count: usize,
) -> &'static [DecoderCandidateSource] {
    if adapter_hardware_count == 0 {
        &[
            DecoderCandidateSource::UnscopedHardware,
            DecoderCandidateSource::Registered,
        ]
    } else {
        &[
            DecoderCandidateSource::AdapterHardware,
            DecoderCandidateSource::Registered,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{DecoderCandidateSource, decoder_candidate_sources};

    #[test]
    fn intel_style_adapters_search_unscoped_hardware_before_software() {
        assert_eq!(
            decoder_candidate_sources(0),
            &[
                DecoderCandidateSource::UnscopedHardware,
                DecoderCandidateSource::Registered,
            ]
        );
    }

    #[test]
    fn adapter_matched_hardware_is_not_replaced_by_another_gpu() {
        assert_eq!(
            decoder_candidate_sources(1),
            &[
                DecoderCandidateSource::AdapterHardware,
                DecoderCandidateSource::Registered,
            ]
        );
    }
}
