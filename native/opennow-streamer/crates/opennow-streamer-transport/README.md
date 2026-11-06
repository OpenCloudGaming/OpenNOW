# Normal NVST encoded-byte counts

For normal version-1 assembled prefixes with `H >= 8`, extension `0x202` carries
a little-endian `u32` encoded-byte count. The parser requires at least four payload
bytes and retains the last complete duplicate. A final zero count is ignored.

For H.264, H.265, and AV1, a positive count trims the assembled codec payload before
the output-size limit is checked. A count larger than the available codec payload
rejects the frame and resets assembly. The count never increases the payload length.
Raw assembly remains bounded independently. Historical `H = 0` handling is unchanged.

This policy adopts bounded valid sender counts, not the official client's exact
configuration gate. The recovered consumer applies its override when an internal
configuration value at `+0x404` exceeds one. No negotiated equivalent of that gate
is established here.

The `synthetic_av1_length_tag_uses_shape_observed_in_real_hevc_capture` test uses
the header layout observed in a real HEVC capture, but its capture counter and
codec bytes are synthetic. The tests contain no captured picture data.
