# Experimental iOS NVST transport

Enable **Settings → Stream → Connection → Native NVST Receiver (Experimental)**, then start or resume a session. The standard WebRTC transport remains the default. Native sessions require an advertised RTSPS control endpoint; the client does not guess the previous seat's endpoint or silently change transports.

The protocol and VideoToolbox modules originate from OpenCloudGaming/OpenNOW-Mac at the revision in UPSTREAM.json. The iOS lifecycle and input adaptation are owned by NativeStreamNVST.swift, NativeStreamNVSTInput.swift and NativeStreamNVSTConfiguration.swift. The existing iOS renderer, pointer capture, touch controls, HDR conversion, PiP and UI stay in StreamerView.swift. All native video decoding requires hardware; a 10-bit stream requests only 10-bit output surfaces. AV1 color metadata comes from its actual sequence header.

Video owns a dedicated raw-SRTP UDP socket and receive queue. Decode and completion-based frame acknowledgement use a separate queue. The DTLS/SCTP bundle carries control, QoS, input, audio and RTCP feedback. The iOS audio adapter uses RemoteIO at 48 kHz stereo; microphone carriage is announced only if offered by the host and enabled in settings. Legacy standalone microphone carriage is not implemented.

Build OpenSSL and usrsctp with `python3 ios/OpenNOWiOS/BuildScripts/build-native-protocol-libs.py --cmake /path/to/cmake`. The script verifies source hashes/revisions and builds static XCFrameworks for arm64 devices and simulators. Libraries are linked statically, not embedded as runtime frameworks. The supported experimental runtime is iOS 17 and newer; the standard path retains the app's iOS 16.4 minimum.

Tests include the upstream DTLS/SCTP local-peer tests, cryptographic vectors, packet recovery, RTSP negotiation, and iOS quality/identity preservation and migration checks. Simulator checks do not prove hardware decoding or 120 FPS performance on an iPad. Use the ordinary stream statistics for comparisons. Per-frame diagnostic disk tracing was removed in build 150; error reporting remains available.

Licenses: OpenNOW-Mac MIT, OpenSSL Apache 2.0, usrsctp BSD. Copies are in Licenses. No NVIDIA libraries are included in the native receiver.
