//! <image src="https://user-images.githubusercontent.com/227204/226143511-66fe5264-6ab7-47b9-9551-90ba7e155b96.svg" alt="str0m logo" ></image>
//!
//! A Sans I/O WebRTC implementation in Rust.
//!
//! This is a [Sans I/O][sansio] implementation meaning the `Rtc` instance itself is not doing any network
//! talking. Furthermore it has no internal threads or async tasks. All operations are happening from the
//! calls of the public API.
//!
//! This is deliberately not a standard `RTCPeerConnection` API since that isn't a great fit for Rust.
//! See more details in below section.
//!
//! # Join us
//!
//! We are discussing str0m things on Discord. Join us using this [invitation link][discord].
//!
//! <image width="300px" src="https://user-images.githubusercontent.com/227204/209446544-f8a8d673-cb1b-4144-a0f2-42307b8d8869.gif" alt="silly clip showing video playing" ></image>
//!
//! # Usage
//!
//! The [`chat`][x-chat] example shows how to connect multiple browsers
//! together and act as an SFU (Selective Forwarding Unit). The example
//! multiplexes all traffic over one server UDP socket and uses two threads
//! (one for the web server, and one for the SFU loop).
//!
//! ## TLS
//!
//! For the browser to do WebRTC, all traffic must be under TLS. The
//! project ships with a self-signed certificate that is used for the
//! examples. The certificate is for hostname `str0m.test` since TLD .test
//! should never resolve to a real DNS name.
//!
//! ```text
//! cargo run --example chat
//! ```
//!
//! The log should prompt you to connect a browser to https://10.0.0.103:3000 – this will
//! most likely cause a security warning that you must get the browser to accept.
//!
//! The [`http-post`][x-post] example roughly illustrates how to receive
//! media data from a browser client. The example is single threaded and
//! is a bit simpler than the chat. It is a good starting point to understand the API.
//!
//! ```text
//! cargo run --example http-post
//! ```
//!
//! ### Real example
//!
//! To see how str0m is used in a real project, check out [BitWHIP][bitwhip] –
//! a CLI WebRTC Agent written in Rust.
//!
//! ## Passive
//!
//! For passive connections, i.e. where the media and initial OFFER is
//! made by a remote peer, we need these steps to open the connection.
//!
//! ```no_run
//! # use std::time::Instant;
//! # use str0m::{Rtc, Candidate};
//! // Instantiate a new Rtc instance.
//! let mut rtc = Rtc::new(Instant::now());
//!
//! //  Add some ICE candidate such as a locally bound UDP port.
//! let addr = "1.2.3.4:5000".parse().unwrap();
//! let candidate = Candidate::host(addr, "udp").unwrap();
//! rtc.add_local_candidate(candidate);
//!
//! // Accept an incoming offer from the remote peer
//! // and get the corresponding answer.
//! let offer = todo!();
//! let answer = rtc.sdp_api().accept_offer(offer).unwrap();
//!
//! // Forward the answer to the remote peer.
//!
//! // Go to _run loop_
//! ```
//!
//! ## Active
//!
//! Active connections means we are making the inital OFFER and waiting for a
//! remote ANSWER to start the connection.
//!
//! ```no_run
//! # use std::time::Instant;
//! # use str0m::{Rtc, Candidate};
//! # use str0m::media::{MediaKind, Direction};
//! // Instantiate a new Rtc instance.
//! let mut rtc = Rtc::new(Instant::now());
//!
//! // Add some ICE candidate such as a locally bound UDP port.
//! let addr = "1.2.3.4:5000".parse().unwrap();
//! let candidate = Candidate::host(addr, "udp").unwrap();
//! rtc.add_local_candidate(candidate);
//!
//! // Create a `SdpApi`. The change lets us make multiple changes
//! // before sending the offer.
//! let mut change = rtc.sdp_api();
//!
//! // Do some change. A valid OFFER needs at least one "m-line" (media).
//! let mid = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
//!
//! // Get the offer.
//! let (offer, pending) = change.apply().unwrap();
//!
//! // Forward the offer to the remote peer and await the answer.
//! // How to transfer this is outside the scope for this library.
//! let answer = todo!();
//!
//! // Apply answer.
//! rtc.sdp_api().accept_answer(pending, answer).unwrap();
//!
//! // Go to _run loop_
//! ```
//!
//! ## Run loop
//!
//! ### The single-mutation invariant
//!
//! str0m's API has one strict contract that the run loop is built around:
//!
//! > **Every mutation of an `Rtc` instance must be followed by a complete
//! > drain of `poll_output` until it returns `Output::Timeout`, before the
//! > next mutation on the same `Rtc`.**
//!
//! A "mutation" is anything that takes `&mut Rtc` (directly or through a
//! handle obtained from it). The common ones are:
//!
//! - `Rtc::handle_input` — feeding a network packet or a timeout
//! - `Writer::write` / `Writer::request_keyframe` — sending media
//! - `Channel::write` — sending data channel data
//! - `SdpApi::apply` / `DirectApi::*` — negotiation
//! - `Rtc::add_local_candidate` / `Rtc::add_remote_candidate`
//!
//! Always: **mutate → drain to `Timeout` → mutate → drain to `Timeout` → …**
//!
//! Doing two mutations back-to-back without draining in between, or
//! waiting on I/O while the engine still has output queued, leaves the
//! engine in an inconsistent state and produces wrong behavior. Mutations
//! issued from inside the drain loop (e.g. calling `Writer::write` in
//! response to an `Output::Event`) are fine — the drain loop naturally
//! continues calling `poll_output` afterward and so the invariant holds.
//!
//! ### Canonical shape
//!
//! Driving an `Rtc` forward follows the same six-step shape, regardless
//! of sync or async:
//!
//! 1. **Wait** for one of: the next timeout firing, an incoming network
//!    packet, or the application wanting to perform a mutation (e.g.
//!    write media).
//! 2. **Perform that ONE mutation** — feed `Input` to `handle_input`, or
//!    call into a writer / channel / SDP API.
//! 3. **Poll** `Rtc::poll_output`.
//! 4. **Handle** the output: `Output::Transmit` is sent on the socket,
//!    `Output::Event` is dispatched to the application, `Output::Timeout`
//!    records the next deadline.
//! 5. **Goto 3** until `poll_output` returns `Output::Timeout`. Only then
//!    is the engine fully drained.
//! 6. The returned timeout is what we wait on next — **goto 1**.
//!
//! ```no_run
//! # use str0m::{Rtc, Output, IceConnectionState, Event, Input};
//! # use str0m::net::{Receive, Protocol};
//! # use std::io::ErrorKind;
//! # use std::net::UdpSocket;
//! # use std::time::{Duration, Instant};
//! # let mut rtc = Rtc::new(Instant::now());
//! // A UdpSocket we obtained _somehow_.
//! let socket: UdpSocket = todo!();
//!
//! // Buffer for reading incoming UDP packets.
//! let mut buf = vec![0; 2000];
//!
//! loop {
//!     // === Steps 3-5: drain poll_output until we get the next timeout. ===
//!     let timeout = loop {
//!         match rtc.poll_output().unwrap() {
//!             // Step 5: a Timeout exits the drain loop.
//!             Output::Timeout(t) => break t,
//!
//!             // Step 4: transmit on the socket and keep draining.
//!             // The destination IP comes from the ICE agent and may
//!             // change during the session.
//!             Output::Transmit(t) => {
//!                 socket.send_to(&t.contents, t.destination).unwrap();
//!             }
//!
//!             // Step 4: hand the event to the application and keep draining.
//!             // Events are mainly incoming media data from the remote peer,
//!             // but also data channel data and statistics.
//!             Output::Event(e) => {
//!                 if e == Event::IceConnectionStateChange(IceConnectionState::Disconnected) {
//!                     return;
//!                 }
//!                 // TODO: handle other events here, such as incoming media data.
//!             }
//!         }
//!     };
//!
//!     // === Step 1: wait for ONE of: the timeout firing, an incoming
//!     // packet, or application-side data. The example below uses a
//!     // blocking UDP socket with a read timeout. With async you would
//!     // `select!` over multiple futures; with application-side data you
//!     // would also include a channel.
//!     //
//!     // set_read_timeout(Some(ZERO)) is not allowed, so clamp to >= 1ms.
//!     let duration = (timeout - Instant::now()).max(Duration::from_millis(1));
//!     socket.set_read_timeout(Some(duration)).unwrap();
//!     buf.resize(2000, 0);
//!
//!     // === Step 2: take ONE event and feed it as Input ===
//!     let input = match socket.recv_from(&mut buf) {
//!         Ok((n, source)) => {
//!             buf.truncate(n);
//!             Input::Receive(
//!                 Instant::now(),
//!                 Receive {
//!                     proto: Protocol::Udp,
//!                     source,
//!                     destination: socket.local_addr().unwrap(),
//!                     contents: buf.as_slice().try_into().unwrap(),
//!                 },
//!             )
//!         }
//!
//!         // The socket read timed out — feed Input::Timeout to advance
//!         // the engine to the deadline. WouldBlock is the unix error,
//!         // TimedOut is the windows error.
//!         Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
//!             Input::Timeout(Instant::now())
//!         }
//!
//!         Err(e) => {
//!             eprintln!("Error: {:?}", e);
//!             return;
//!         }
//!     };
//!
//!     rtc.handle_input(input).unwrap();
//!
//!     // === Step 6: back to the top of the outer loop (goto step 3). ===
//! }
//! ```
//!
//! ## Sending media data
//!
//! When creating the media, we can decide which codecs to support, and they
//! are negotiated with the remote side. Each codec corresponds to a
//! "payload type" (PT). To send media data we need to figure out which PT
//! to use when sending.
//!
//! ```no_run
//! # use str0m::Rtc;
//! # use str0m::media::Mid;
//! # let rtc: Rtc = todo!();
//! // Obtain mid from Event::MediaAdded
//! let mid: Mid = todo!();
//!
//! // Create a media writer for the mid.
//! let writer = rtc.writer(mid).unwrap();
//!
//! // Get the payload type (pt) for the wanted codec.
//! let pt = writer.payload_params().nth(0).unwrap().pt();
//!
//! // Write the data
//! let wallclock = todo!();   // Absolute time of the data
//! let media_time = todo!();  // Media time, in RTP time
//! let data: &[u8] = todo!(); // Actual data
//! writer.write(pt, wallclock, media_time, data).unwrap();
//! ```
//!
//! `Writer::write` is a mutation, so the
//! [single-mutation invariant](#the-single-mutation-invariant) applies:
//! after writing, drain `Rtc::poll_output` to `Output::Timeout` before
//! the next mutation on this `Rtc`.
//!
//! ## Media time, wallclock and local time
//!
//! str0m has three main concepts of time. "now", media time and wallclock.
//!
//! ### Now
//!
//! Some calls in str0m, such as `Rtc::handle_input` takes a `now` argument
//! that is a `std::time::Instant`. These calls "drive the time forward" in
//! the internal state. This is used for everything like deciding when
//! to produce various feedback reports (RTCP) to remote peers, to
//! bandwidth estimation (BWE) and statistics.
//!
//! Str0m has _no internal clock_ calls. I.e. str0m never calls
//! `Instant::now()` itself. All time is external input. That means it's
//! possible to construct test cases driving an `Rtc` instance faster
//! than realtime (see the [integration tests][intg]).
//!
//! ### Media time
//!
//! Each RTP header has a 32 bit number that str0m calls _media time_.
//! Media time is in some time base that is dependent on the codec,
//! however all codecs in str0m use 90_000Hz for video and 48_000Hz
//! for audio.
//!
//! For video the `MediaTime` type is `<timestamp>/90_000` str0m extends
//! the 32 bit number in the RTP header to 64 bit taking into account
//! "rollover". 64 bit is such a large number the user doesn't need to
//! think about rollovers.
//!
//! ### Wallclock
//!
//! With _wallclock_ str0m means the time a sample of media was produced
//! at an originating source. I.e. if we are talking into a microphone the
//! wallclock is the NTP time the sound is sampled.
//!
//! We can't know the exact wallclock for media from a remote peer since
//! not every device is synchronized with NTP. Every sender does
//! periodically produce a Sender Report (SR) that contains the peer's
//! idea of its wallclock, however this number can be very wrong compared to
//! "real" NTP time.
//!
//! Furthermore, not all remote devices will have a linear idea of
//! time passing that exactly matches the local time. A minute on the
//! remote peer might not be exactly one minute locally.
//!
//! These timestamps become important when handling simultaneous audio from
//! multiple peers.
//!
//! When writing media we need to provide str0m with an estimated wallclock.
//! The simplest strategy is to only trust local time and use arrival time
//! of the incoming UDP packet. Another simple strategy is to lock some
//! time T at the first UDP packet, and then offset each wallclock using
//! `MediaTime`, i.e. for video we could have `T + <media time>/90_000`
//!
//! A production worthy SFU probably needs an even more sophisticated
//! strategy weighing in all possible time sources to get a good estimate
//! of the remote wallclock for a packet.
//!
//! # Crypto backends
//!
//! str0m supports multiple crypto backends via feature flags. The default is `aws-lc-rs`.
//!
//! | Feature           | Crate                 | DTLS                             | Platforms |
//! |-------------------|-----------------------|----------------------------------|-----------|
//! | `aws-lc-rs`       | `str0m-aws-lc-rs`     | dimpl + AWS-LC-RS                | All       |
//! | `rust-crypto`     | `str0m-rust-crypto`   | dimpl + RustCrypto               | All       |
//! | `openssl`         | `str0m-openssl`       | OpenSSL (DTLS 1.2 only)          | All       |
//! | `openssl-dimpl`   | `str0m-openssl`       | dimpl + OpenSSL crypto           | All       |
//! | `apple-crypto`    | `str0m-apple-crypto`  | dimpl + Apple CryptoKit          | macOS/iOS |
//! | `wincrypto`       | `str0m-wincrypto`     | Windows SChannel (DTLS 1.2 only) | Windows   |
//! | `wincrypto-dimpl` | `str0m-wincrypto`     | dimpl + Windows CNG              | Windows   |
//!
//! If multiple backend features are enabled, str0m automatically selects the backend in this
//! priority order: `aws-lc-rs`, `rust-crypto`, `openssl-dimpl`, `openssl`, `apple-crypto`
//! (Apple platforms only), `wincrypt-dimpl` (Windows only), `wincrypto` (Windows only).
//!
//! If you disable the default features, you MUST explicitly configure an alternative
//! crypto backend either process-wide or per-instance.
//!
//! ## Process-wide default
//!
//! For applications, the easiest is to set a process-wide default at startup.
//! Note that you can use any backend crate directly without enabling its feature flag:
//!
//! ```no_run
//! // Set process default (will panic if called twice)
//! // No need to enable the "rust-crypto" feature flag
//! str0m_rust_crypto::default_provider().install_process_default();
//! ```
//!
//! ## Crypto provider per Rtc instance
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::time::Instant;
//! use str0m::Rtc;
//!
//! let rtc = Rtc::builder()
//!     .set_crypto_provider(Arc::new(str0m_rust_crypto::default_provider()))
//!     .build(Instant::now());
//! ```
//!
//! # Project status
//!
//! Str0m was originally developed by Martin Algesten of
//! [Lookback][lookback]. We use str0m for a specific use case: str0m as a
//! server SFU (as opposed to peer-2-peer). That means we are heavily
//! testing and developing the parts needed for our use case. Str0m is
//! intended to be an all-purpose WebRTC library, which means it also
//! works for peer-2-peer, though that aspect has received less testing.
//!
//! Performance is very good, there have been some work the discover and
//! optimize bottlenecks. Such efforts are of course never ending with
//! diminishing returns. While there are no glaringly obvious performance
//! bottlenecks, more work is always welcome – both algorithmically and
//! allocation/cloning in hot paths etc.
//!
//! # Design
//!
//! Output from the `Rtc` instance can be grouped into three kinds.
//!
//! 1. Events (such as receiving media or data channel data).
//! 2. Network output. Data to be sent, typically from a UDP socket.
//! 3. Timeouts. Indicates when the instance next expects a time input.
//!
//! Input to the `Rtc` instance is:
//!
//! 1. User operations (such as sending media or data channel data).
//! 2. Network input. Typically read from a UDP socket.
//! 3. Timeouts. As obtained from the output above.
//!
//! The correct use can be seen in the above [Run loop](#run-loop) or in the
//! examples.
//!
//! Sans I/O is a pattern where we turn both network input/output as well
//! as time passing into external input to the API. This means str0m has
//! no internal threads, just an enormous state machine that is driven
//! forward by different kinds of input.
//!
//! ## Frame or RTP level?
//!
//! Str0m defaults to the "frame level" which treats the RTP as an internal detail. The user
//! will thus mainly interact with:
//!
//! 1. [`Event::MediaData`][evmed] to receive full frames (audio frames or video frames).
//! 2. [`Writer::write`][writer] to write full frames.
//! 3. [`Writer::request_keyframe`][reqkey] to request keyframes.
//!
//! ### Frame level
//!
//! All codecs such as h264, vp8, vp9 and opus outputs what we call
//! "Frames". A frame has a very specific meaning for video, but this
//! project uses it in a broader sense, where a frame is either a video
//! or audio time stamped chunk of encoded data that typically represents
//! a chunk of audio, or _one single frame for video_.
//!
//! Frames are not suitable to use directly in UDP (RTP) packets - for
//! one they are too big. Frames are therefore further chunked up by
//! codec specific payloaders into RTP packets.
//!
//! ### RTP mode
//!
//! Str0m also provides an RTP level API. This would be similar to many other
//! RTP libraries where the RTP packets themselves are the API surface
//! towards the user (when building an SFU one would often talk about "forwarding
//! RTP packets", while with str0m we can also "forward frames").  Using
//! this API requires a deeper knowledge of RTP and WebRTC.
//!
//! To enable RTP mode
//!
//! ```
//! # use std::time::Instant;
//! # use str0m::Rtc;
//! let rtc = Rtc::builder()
//!     // Enable RTP mode for this Rtc instance.
//!     // This disables `MediaEvent` and the `Writer::write` API.
//!     .set_rtp_mode(true)
//!     .build(Instant::now());
//! ```
//!
//! RTP mode gives us some new API points.
//!
//! 1. [`Event::RtpPacket`][rtppak] emitted for every incoming RTP packet. Empty packets for bandwidth
//!    estimation are silently discarded.
//! 2. [`StreamTx::write_rtp`][wrtrtp] to write outgoing RTP packets.
//! 3. [`StreamRx::request_keyframe`][reqkey2] to request keyframes from remote.
//!
//! ## NIC enumeration and TURN (and STUN)
//!
//! The [ICE RFC][ice] talks about "gathering ice candidates". This means
//! inspecting the local network interfaces and potentially binding UDP
//! sockets on each usable interface. Since str0m is Sans I/O, this part
//! is outside the scope of what str0m does. How the user figures out
//! local IP addresses, via config or via looking up local NICs is not
//! something str0m cares about.
//!
//! TURN is a way of obtaining IP addresses that can be used as fallback
//! in case direct connections fail. We consider TURN similar to
//! enumerating local network interfaces – it's a way of obtaining
//! sockets.
//!
//! All discovered candidates, be they local (NIC) or remote sockets
//! (TURN), are added to str0m and str0m will perform the task of ICE
//! agent, forming "candidate pairs" and figuring out the best connection
//! while the actual task of sending the network traffic is left to the
//! user.
//!
//! ## The importance of `&mut self`
//!
//! Rust shines when we can eschew locks and heavily rely `&mut` for data
//! write access. Since str0m has no internal threads, we never have to
//! deal with shared data. Furthermore the the internals of the library is
//! organized such that we don't need multiple references to the same
//! entities. In str0m there are no `Rc`, `Mutex`, `mpsc`, `Arc`(*),  or
//! other locks.
//!
//! This means all input to the lib can be modelled as
//! `handle_something(&mut self, something)`.
//!
//! (*) Ok. There is one `Arc` if you use Windows where we also require openssl.
//!
//! ## Not a standard WebRTC "Peer Connection" API
//!
//! The library deliberately steps away from the "standard" WebRTC API as
//! seen in JavaScript and/or [webrtc-rs][webrtc-rs] (or [Pion][pion] in Go).
//! There are few reasons for this.
//!
//! First, in the standard API, events are callbacks, which are not a
//! great fit for Rust. Callbacks require some kind of reference
//! (ownership?) over the entity the callback is being dispatched
//! upon. I.e. if in Rust we want `pc.addEventListener(x)`, `x` needs
//! to be wholly owned by `pc`, or have some shared reference (like
//! `Arc`). Shared references means shared data, and to get mutable shared
//! data, we will need some kind of lock. i.e. `Arc<Mutex<EventListener>>`
//! or similar.
//!
//! As an alternative we could turn all events into `mpsc` channels, but
//! listening to multiple channels is awkward without async.
//!
//! Second, in the standard API, entities like `RTCPeerConnection` and
//! `RTCRtpTransceiver`, are easily clonable and/or long lived
//! references. I.e. `pc.getTranscievers()` returns objects that can be
//! retained and owned by the caller. This pattern is fine for garbage
//! collected or reference counted languages, but not great with Rust.
//!
//! ## Panics, Errors and unwraps
//!
//! Str0m adheres to [fail-fast][ff]. That means rather than brushing state
//! bugs under the carpet, it panics. We make a distinction between errors and
//! bugs.
//!
//! * Errors are as a result of incorrect or impossible to understand user input.
//! * Bugs are broken internal invariants (assumptions).
//!
//! If you scan the str0m code you find a few `unwrap()` (or `expect()`). These
//! will (should) always be accompanied by a code comment that explains why the
//! unwrap is okay. This is an internal invariant, a state assumption that
//! str0m is responsible for maintaining.
//!
//! We do not believe it's correct to change every `unwrap()`/`expect()` into
//! `unwrap_or_else()`, `if let Some(x) = x { ... }` etc, because doing so
//! brushes an actual problem (an incorrect assumption) under the carpet. Trying
//! to hobble along with an incorrect state would at best result in broken
//! behavior, at worst a security risk!
//!
//! Panics are our friends: *panic means bug*
//!
//! And also: str0m should *never* panic on any user input. If you encounter a panic,
//! please report it!
//!
//! ### Catching panics
//!
//! Panics should be incredibly rare, or we have a serious problem as a project. For an SFU,
//! it might not be ideal if str0m encounters a bug and brings the entire server down with it.
//!
//! For those who want an extra level of safety, we recommend looking at [`catch_unwind`][catch]
//! to safely discard a faulty `Rtc` instance. Since `Rtc` has no internal threads, locks or async
//! tasks, discarding the instance never risk poisoning locks or other issues that can happen
//! when catching a panic.
//!
//! ## FAQ
//!
//! ### Features
//!
//! Below is a brief comparison of features between libWebRTC and str0m to help you determine
//! if str0m is suitable for your project.
//!
//! | Feature                  | str0m              | libWebRTC          |
//! | ------------------------ | ------------------ | ------------------ |
//! | Peer Connection API      | :x:                | :white_check_mark: |
//! | SDP                      | :white_check_mark: | :white_check_mark: |
//! | ICE                      | :white_check_mark: | :white_check_mark: |
//! | Data Channels            | :white_check_mark: | :white_check_mark: |
//! | Send/Recv Reports        | :white_check_mark: | :white_check_mark: |
//! | Transport Wide CC        | :white_check_mark: | :white_check_mark: |
//! | Bandwidth Estimation     | :white_check_mark: | :white_check_mark: |
//! | Simulcast                | :white_check_mark: | :white_check_mark: |
//! | NACK                     | :white_check_mark: | :white_check_mark: |
//! | Packetize                | :white_check_mark: | :white_check_mark: |
//! | Fixed Depacketize Buffer | :white_check_mark: | :white_check_mark: |
//! | Adaptive Jitter Buffer   | :x:                | :white_check_mark: |
//! | Video/audio capture      | :x:                | :white_check_mark: |
//! | Video/audio encode       | :x:                | :white_check_mark: |
//! | Video/audio decode       | :x:                | :white_check_mark: |
//! | Audio render             | :x:                | :white_check_mark: |
//! | Turn                     | :x:                | :white_check_mark: |
//! | Network interface enum   | :x:                | :white_check_mark: |
//!
//! ### Platform Support
//!
//! Platforms str0m is compiled and tested on:
//!
//! | Platform                       | Compiled          | Tested            |
//! | ------------------------------ | ----------------- | ----------------- |
//! | `x86_64-pc-windows-msvc`       | :white_check_mark:| :white_check_mark:|
//! | `x86_64-unknown-linux-gnu`     | :white_check_mark:| :white_check_mark:|
//! | `x86_64-apple-darwin`          | :white_check_mark:| :white_check_mark:|
//! | `aarch64-apple-darwin`         | :white_check_mark:| :white_check_mark:|
//! | `aarch64-unknown-linux-gnu`    | :white_check_mark:| :white_check_mark:|
//! | `aarch64-pc-windows-msvc`      | :white_check_mark:| :white_check_mark:|
//! | `aarch64-apple-ios`            | :white_check_mark:| :x:               |
//! | `aarch64-linux-android`        | :white_check_mark:| :x:               |
//!
//! If your platform isn't listed but is supported by Rust, we'd love for you to give str0m a try and
//! share your experience. We greatly appreciate your feedback!
//!
//! ### Does str0m support IPv4, IPv6, UDP and TCP?
//!
//! Certainly! str0m fully support IPv4, IPv6, UDP and TCP protocols.
//!
//! ### Can I utilize str0m with any Rust async runtime?
//!
//! Absolutely! str0m is fully sync, ensuring that it integrates seamlessly with any Rust async
//! runtime you opt for.
//!
//! ### Can I create a client with str0m?
//!
//! Of course! You have the freedom to create a client with str0m. However, please note that some
//! common client features like media encoding, decoding, and capture are not included in str0m. But
//! don't let that stop you from building amazing applications!
//!
//! ### Can I use str0m in a media server?
//!
//! Yes! str0m excels as a server component with support for both RTP API and Frame API. You can
//! easily build that recording server or SFU you dreamt of in Rust!
//!
//! ### Can I deploy the chat example into production?
//!
//! While the chat example showcases how to use str0m's API, it's not intended for production use or
//! heavy load. Writing a full-featured SFU or MCU (Multipoint Control Unit) is a significant
//! undertaking, involving various design decisions based on production requirements.
//!
//! ### Discovered a bug? Here's how to share it with us
//!
//! We'd love to hear about it! Please submit an issue and consider joining our Discord community
//! to discuss further. For a seamless reporting experience, refer to this exemplary
//! bug report: <https://github.com/algesten/str0m/issues/382>. We appreciate your contribution
//! to making str0m better!
//!
//! ### I am allergic to SDP can you help me?
//!
//! Yes use the direct API!
//!
//! [sansio]:     https://sans-io.readthedocs.io
//! [quinn]:      https://github.com/quinn-rs/quinn
//! [pion]:       https://github.com/pion/webrtc
//! [webrtc-rs]:  https://github.com/webrtc-rs/webrtc
//! [discord]:    https://discord.gg/e2CC8UYebP
//! [zulip]:      https://str0m.zulipchat.com
//! [ice]:        https://www.rfc-editor.org/rfc/rfc8445
//! [lookback]:   https://www.lookback.com
//! [x-post]:     https://github.com/algesten/str0m/blob/main/examples/http-post.rs
//! [x-chat]:     https://github.com/algesten/str0m/blob/main/examples/chat.rs
//! [intg]:       https://github.com/algesten/str0m/blob/main/tests/unidirectional.rs#L12
//! [ff]:         https://en.wikipedia.org/wiki/Fail-fast
//! [catch]:      https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
//! [evmed]:      https://docs.rs/str0m/*/str0m/enum.Event.html#variant.MediaData
//! [writer]:     https://docs.rs/str0m/*/str0m/media/struct.Writer.html#method.write
//! [reqkey]:     https://docs.rs/str0m/*/str0m/media/struct.Writer.html#method.request_keyframe
//! [rtppak]:     https://docs.rs/str0m/*/str0m/enum.Event.html#variant.RtpPacket
//! [wrtrtp]:     https://docs.rs/str0m/*/str0m/rtp/struct.StreamTx.html#method.write_rtp
//! [reqkey2]:    https://docs.rs/str0m/*/str0m/rtp/struct.StreamRx.html#method.request_keyframe
//! [bitwhip]:    https://github.com/bitwhip/bitwhip

#![forbid(unsafe_code)]
#![allow(clippy::new_without_default)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::get_first)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::precedence)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::uninlined_format_args)]
#![allow(mismatched_lifetime_syntaxes)]
#![deny(clippy::needless_pass_by_ref_mut)]
#![deny(missing_docs)]

#[macro_use]
extern crate tracing;

use bwe::{Bwe, BweKind};
use change::{DirectApi, SdpApi};
use rtp::RawPacket;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use str0m_proto::Pii;
use streams::RtpPacket;
use streams::StreamPaused;
use util::InstantExt;

// Identity `drv::ToStatic` (`Static = Self`) for the `Copy` identity types.
// Used instead of `#[derive(drv::Input)]` because the derive's generated
// shadow type isn't `Eq`/`Hash`/`Borrow<Self>`, so it can't be a `HashMap`
// key inside a `#[drv::memo]` projection; `Static = Self` can. `eq_static`
// defers to each type's own `PartialEq`. Each module invokes this for its
// own types.
#[cfg(feature = "drv")]
macro_rules! drv_identity_copy {
    ($($t:ty),* $(,)?) => {
        $(
            impl drv::ToStatic for $t {
                type Static = $t;
                fn to_static(&self) -> $t { *self }
                fn eq_static(&self, other: &$t) -> bool { self == other }
            }
        )*
    };
}
#[cfg(feature = "drv")]
pub(crate) use drv_identity_copy;

/// Cryptographic provider traits and implementations.
///
/// This module provides the traits for pluggable cryptographic operations
/// used in DTLS, SRTP, and STUN.
pub mod crypto;
use crypto::Fingerprint;

mod dtls;
use crate::crypto::dtls::DtlsOutput;
use crate::crypto::{CryptoProvider, DtlsError, from_feature_flags};
use crate::dtls::is_would_block;
use dtls::Dtls;

use is::IceAgent;
use is::IceAgentEvent;
pub use is::{Candidate, CandidateBuilder, CandidateKind, IceConnectionState, IceCreds};

#[path = "config.rs"]
mod config_mod;
pub use config_mod::RtcConfig;

/// Default target MTU used when none is configured.
pub use io::DATAGRAM_MTU_TARGET;
/// Upper bound that the target endpoint of [`RtcConfig::set_mtu`] must satisfy.
pub use io::DATAGRAM_MTU_TARGET_MAX;
/// Lower bound that the target endpoint of [`RtcConfig::set_mtu`] must satisfy.
pub use io::DATAGRAM_MTU_TARGET_MIN;
/// Default warning threshold for over-sized packets.
pub use io::DATAGRAM_MTU_WARN;

/// Additional configuration.
pub mod config {
    pub use super::crypto::dtls::{DtlsCert, DtlsVersion, KeyingMaterial};
    pub use super::crypto::{CryptoProvider, Fingerprint};
}

/// Low level ICE access.
// The ICE API is not necessary to interact with directly for "regular"
// use of str0m. This is exported for other libraries that want to
// reuse str0m's ICE implementation. This is now in the `is` crate.
#[doc(hidden)]
pub mod ice {
    pub use is::IceCreds;
    pub use is::stun::{StunMessage, StunMessageBuilder, StunPacket, TransId};
    pub use is::{IceAgent, IceAgentEvent};
    pub use is::{LocalPreference, default_local_preference};
}

mod io;
use io::DatagramRecvInner;

mod packet;

#[path = "rtp/mod.rs"]
mod rtp_;
use rtp_::{Bitrate, DataSize};

/// Low level RTP access.
pub mod rtp {
    /// Feedback for RTP.
    pub mod rtcp {
        pub use crate::rtp_::AppSpecificFeedback;
        pub use crate::rtp_::{Descriptions, ExtendedReport, Fir, Goodbye, Nack, Pli};
        pub use crate::rtp_::{Dlrr, NackEntry, ReceptionReport, ReportBlock};
        pub use crate::rtp_::{FirEntry, ReceiverReport, SenderInfo, SenderReport, Twcc};
        pub use crate::rtp_::{ReportList, Rrtr, Rtcp, Sdes, SdesType};
    }
    use self::rtcp::Rtcp;

    /// Video Layers Allocation RTP Header Extension
    pub mod vla;
    pub use crate::rtp_::{AbsCaptureTime, ExtensionValues, UserExtensionValues};
    pub use crate::rtp_::{Extension, ExtensionMap, ExtensionSerializer};

    pub use crate::packet::{
        Vp8Descriptor, Vp8DescriptorError, Vp8Patch, Vp8PatchBuilder, Vp8PatchError,
    };
    pub use crate::rtp_::{RtpHeader, SeqNo, Ssrc, VideoOrientation};
    pub use crate::streams::{
        RtpPacket, RtpWrite, StreamPaused, StreamRx, StreamTx, StreamTxQueueInfo,
    };

    /// Debug output of the unencrypted RTP and RTCP packets.
    ///
    /// Enable using [`RtcConfig::enable_raw_packets()`][crate::RtcConfig::enable_raw_packets].
    /// This clones data, and is therefore expensive.
    /// Should not be enabled outside of tests and troubleshooting.
    #[derive(Debug)]
    pub enum RawPacket {
        /// Sent RTCP.
        RtcpTx(Rtcp),
        /// Incoming RTCP.
        RtcpRx(Rtcp),
        /// Sent RTP.
        RtpTx(RtpHeader, Vec<u8>),
        /// Incoming RTP.
        RtpRx(RtpHeader, Vec<u8>),
    }
}

pub(crate) mod pacer;

#[path = "bwe/mod.rs"]
pub(crate) mod bwe_;

/// Bandwidth estimation.
pub mod bwe {
    pub use crate::bwe_::api::*;
}

mod sctp;
use sctp::{RtcSctp, SctpEvent, SctpInitData};

mod sdp;

pub mod format;
use format::CodecConfig;

pub mod channel;
use channel::{Channel, ChannelData, ChannelHandler, ChannelId};

pub mod media;
use media::AppSpecificFeedback;
use media::SenderFeedback;
use media::{Direction, Media, Mid, Pt, Rid, Writer};
use media::{KeyframeRequest, KeyframeRequestKind};
use media::{MediaAdded, MediaChanged, MediaData};

pub mod change;

mod util;
use util::{Soonest, not_happening};

mod session;
use session::Session;

pub mod stats;

use stats::{CandidatePairStats, CandidateStats, MediaEgressStats, MediaIngressStats};
use stats::{PeerStats, Stats, StatsEvent, StatsSnapshot};

mod streams;

pub mod error;

/// Network related types to get socket data in/out of [`Rtc`].
pub mod net {
    pub use crate::io::{DatagramRecv, DatagramSend, Protocol, Receive, TcpType, Transmit};
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use error::RtcError;

/// Instance that does WebRTC. Main struct of the entire library.
///
/// ## Usage
///
/// The canonical run loop is: perform exactly one mutation (typically
/// `handle_input` with a network packet or timeout), then drain
/// `poll_output` until it returns `Output::Timeout`, then wait for the
/// next event. **Every mutation must be followed by a full drain before
/// the next mutation.** See the [crate docs](crate#run-loop) for the full
/// explanation of the single-mutation invariant.
///
/// ```no_run
/// # use std::time::Instant;
/// # use str0m::{Rtc, Output, Input};
/// let mut rtc = Rtc::new(Instant::now());
///
/// loop {
///     // Drain poll_output until it returns the next timeout.
///     let timeout = loop {
///         match rtc.poll_output().unwrap() {
///             Output::Timeout(t) => break t,
///             Output::Transmit(t) => {
///                 // TODO: Send t.contents to t.destination on a UDP socket.
///             }
///             Output::Event(e) => {
///                 // TODO: Handle event.
///             }
///         }
///     };
///
///     // TODO: Wait for ONE of: `timeout` firing, an incoming network
///     //       packet, or application-side data to send. Build an
///     //       `Input` from whichever fires first.
///     let input: Input = todo!();
///
///     rtc.handle_input(input).unwrap();
/// }
/// ```
pub struct Rtc {
    state: RtcState,
    ice: IceAgent,
    dtls: Dtls,
    dtls_connected: bool,
    dtls_buf: Vec<u8>,
    next_dtls_timeout: Option<Instant>,
    sctp: RtcSctp,
    chan: ChannelHandler,
    stats: Option<Stats>,
    session: Session,
    remote_fingerprint: Option<Fingerprint>,
    remote_addrs: Vec<SocketAddr>,
    send_addr: Option<SendAddr>,
    need_init_time: bool,
    last_now: Instant,
    peer_bytes_rx: u64,
    peer_bytes_tx: u64,
    change_counter: usize,
    last_timeout_reason: Reason,
    crypto_provider: Arc<crate::crypto::CryptoProvider>,
    fingerprint_verification: bool,
    close_dtls_started: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RtcState {
    Alive,
    Closing,
    Closed,
}

struct SendAddr {
    proto: net::Protocol,
    source: SocketAddr,
    destination: SocketAddr,
}

/// Events produced by [`Rtc::poll_output()`].
#[derive(Debug)]
#[non_exhaustive]
#[rustfmt::skip]
pub enum Event {
    // =================== ICE related events ===================

    /// Emitted when we got ICE connection and established DTLS.
    Connected,

    /// ICE connection state changes tells us whether the [`Rtc`] instance is
    /// connected to the peer or not.
    IceConnectionStateChange(IceConnectionState),

    // =================== Media related events ==================

    /// Upon detecting the remote side adding new media to the session.
    ///
    /// For locally added media, this event never fires. Thus it can be thought of as an
    /// "SDP only" event. If the direct API is used on both sides, the declaration is local \
    /// to both sides and the event never fires.
    ///
    /// The [`Media`] instance is available via [`Rtc::media()`].
    MediaAdded(MediaAdded),

    /// Incoming media data sent by the remote peer.
    MediaData(MediaData),

    /// Changes to the media may be emitted.
    ///
    ///. Currently only covers a change of direction.
    MediaChanged(MediaChanged),

    // =================== Data channel related events ===================

    /// A data channel has opened.
    ///
    /// The string is the channel label which is set by the opening peer and can
    /// be used to identify the purpose of the channel when there are more than one.
    ///
    /// The negotiation is to set up an SCTP association via DTLS. Subsequent data
    /// channels reuse the same association.
    ///
    /// Upon this event, the [`Channel`] can be obtained via [`Rtc::channel()`].
    ///
    /// For [`SdpApi`]: The first ever data channel results in an SDP
    /// negotiation, and this events comes at the end of that.
    ChannelOpen(ChannelId, String),

    /// Incoming data channel data from the remote peer.
    ChannelData(ChannelData),

    /// A data channel has been closed.
    ChannelClose(ChannelId),

    /// A data channel's buffered amount has dropped below the configured threshold.
    ChannelBufferedAmountLow(ChannelId),

    // =================== Statistics and BWE related events ===================

    /// Statistics event for the Rtc instance
    ///
    /// Includes both media traffic (rtp payload) as well as all traffic
    PeerStats(PeerStats),

    /// Aggregated statistics for each media (mid, rid) in the ingress direction
    MediaIngressStats(MediaIngressStats),

    /// Aggregated statistics for each media (mid, rid) in the egress direction
    MediaEgressStats(MediaEgressStats),

    /// A new estimate from the bandwidth estimation subsystem.
    EgressBitrateEstimate(BweKind),

    // =================== RTP related events ===================

    /// Incoming keyframe request for media that we are sending to the remote peer.
    ///
    /// The request is either PLI (Picture Loss Indication) or FIR (Full Intra Request).
    KeyframeRequest(KeyframeRequest),

    /// Whether an incoming encoded stream is paused.
    ///
    /// This means the stream has not received any data for some time (default 1.5 seconds).
    StreamPaused(StreamPaused),

    /// Sender feedback for an incoming stream, derived from RTCP SR.
    SenderFeedback(SenderFeedback),

    /// Incoming RTP data.
    RtpPacket(RtpPacket),

    /// Incoming application-specific Payload-Specific Feedback (PSFB FMT=15, PT=206).
    ///
    /// Emitted when a non-REMB FMT=15 RTCP PSFB message is received. The payload
    /// is opaque and application-defined (RFC 4585 Section 6.4).
    AppSpecificFeedback(AppSpecificFeedback),

    /// The remote DTLS connection or SCTP association has closed.
    ///
    /// When this event is emitted, `Rtc` starts local shutdown signals and
    /// remains alive only while [`Rtc::poll_output()`] drains pending local
    /// close output such as RTCP BYE and the DTLS close_notify response.
    Closed,

    /// Debug output of incoming and outgoing RTCP/RTP packets.
    ///
    /// Enable using [`RtcConfig::enable_raw_packets()`].
    /// This clones data, and is therefore expensive.
    /// Should not be enabled outside of tests and troubleshooting.
    RawPacket(Box<RawPacket>),

    /// For internal testing only.
    ///
    /// The probe cluster config when a probe fires.
    #[cfg(feature = "_internal_test_exports")]
    Probe(crate::bwe_::ProbeClusterConfig),
}

impl Event {
    /// Reference to the [`RawPacket`] if this is indeed an `Event::RawPacket`.
    pub fn as_raw_packet(&self) -> Option<&RawPacket> {
        if let Self::RawPacket(boxed) = &self {
            Some(&**boxed)
        } else {
            None
        }
    }
}

/// Input as expected by [`Rtc::handle_input()`]. Either network data or a timeout.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // We purposely don't want to allocate.
pub enum Input<'a> {
    /// A timeout without any network input.
    Timeout(Instant),
    /// Network input. The [`Instant`] is the time the packet was received from the socket -
    /// not necessarily "now" (e.g. when UDP demultiplexing runs on a separate thread). It
    /// drives time forward when more recent than the last "now" the instance has seen.
    Receive(Instant, net::Receive<'a>),
}

/// Output produced by [`Rtc::poll_output()`]
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Output {
    /// When the [`Rtc`] instance expects an [`Input::Timeout`].
    Timeout(Instant),

    /// Network data that is to be sent.
    Transmit(net::Transmit),

    /// Some event such as media data arriving from the remote peer or connection events.
    Event(Event),
}

pub use crate::pacer::PacerReason;

/// The reason for the next [`Output::Timeout`].
///
/// This enum is not considered stable API and may change in minor revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Reason {
    /// No timeout scheduled.
    ///
    /// The timeout value is in the distant future.
    #[default]
    NotHappening,

    /// The DTLS subsystem.
    ///
    /// Only relevant during handshaking.
    DTLS,

    /// The ICE agent.
    ///
    /// Includes checking candidate pairs and various cleanups.
    Ice,

    /// The SCTP subsystem.
    ///
    /// Things like handling retransmissions and keep-alive checks.
    Sctp,

    /// Data channels.
    ///
    /// Scheduled when we need to open allocations using SCTP.
    Channel,

    /// Stats gathering (if enabled).
    ///
    /// Periodic gathering of statistics.
    Stats,

    /// Regular RTP feedback.
    ///
    /// Receiver reports (RR) and sender reports (SR).
    Feedback,

    /// Sending of RTP NACK.
    ///
    /// When missing packets are discovered, a NACK is scheduled.
    Nack,

    /// Reporting of TWCC (if enabled).
    ///
    /// All incoming RTP packets are reported using TWCC. Enabled via SDP if both
    /// sides support it.
    Twcc,

    /// RTP streams not receiving data goes into a paused state.
    ///
    /// Whenever an RTP receive stream receives data, a new timeout is scheduled.
    PauseCheck,

    /// Preprocessing of RTP packets to be sent.
    ///
    /// Housekeeping task in RTP send streams.
    SendStream,

    /// Packetizing of media into RTP data (if used).
    ///
    /// Written media data needs packetizing. This is not used in RTP mode.
    Packetize,

    /// Pacer doing things.
    Pacer(PacerReason),

    /// The delay controller of the BWE subsystem.
    BweDelayControl,

    /// The probe controller of the BWE subsystem.
    BweProbeControl,

    /// The probe estimator of the BWE subsystem.
    BweProbeEstimator,
}

impl Rtc {
    /// Creates a new instance with default settings.
    ///
    /// To configure the instance, use [`RtcConfig`].
    ///
    /// ```
    /// use std::time::Instant;
    /// use str0m::Rtc;
    ///
    /// let rtc = Rtc::new(Instant::now());
    /// ```
    pub fn new(start: Instant) -> Self {
        let config = RtcConfig::default();
        Self::new_from_config(config, start).expect("Failed to create Rtc from default config")
    }

    /// Creates a config builder that configures an [`Rtc`] instance.
    ///
    /// ```
    /// # use std::time::Instant;
    /// # use str0m::Rtc;
    /// let rtc = Rtc::builder()
    ///     .set_ice_lite(true)
    ///     .build(Instant::now());
    /// ```
    pub fn builder() -> RtcConfig {
        RtcConfig::new()
    }

    pub(crate) fn new_from_config(config: RtcConfig, start: Instant) -> Result<Self, RtcError> {
        let crypto_provider = config
            .crypto_provider
            .clone()
            // If crypto_provider is not set in config, check process default
            .or_else(|| CryptoProvider::get_default().cloned().map(Arc::new))
            // Or fall back on feature flags
            .or_else(|| Some(Arc::new(from_feature_flags())))
            // from_feature_flags panics already, so we should never see
            // this expect message.
            .expect("a crash earlier if no crypto provider was set");

        let session = Session::new(&config);

        // Capture before any partial moves of `config` below.
        let mtu = config.mtu.clone();

        let local_creds = config.local_ice_credentials.unwrap_or_else(IceCreds::new);
        let mut ice = IceAgent::with_hmac(local_creds, crypto_provider.sha1_hmac_provider);
        if config.ice_lite {
            ice.set_ice_lite(config.ice_lite);
        }

        if let Some(initial_stun_rto) = config.initial_stun_rto {
            ice.set_initial_stun_rto(initial_stun_rto);
        }

        if let Some(max_stun_rto) = config.max_stun_rto {
            ice.set_max_stun_rto(max_stun_rto);
        }

        if let Some(max_stun_retransmits) = config.max_stun_retransmits {
            ice.set_max_stun_retransmits(max_stun_retransmits);
        }

        ice.set_mtu(mtu.clone());

        let dtls_cert = config
            .dtls_cert
            .or_else(|| crypto_provider.dtls_provider.generate_certificate())
            .expect(
                "No DTLS certificate provided and the crypto provider cannot generate one. \
             Either provide a certificate via RtcConfig::set_dtls_cert or use a \
             crypto provider that supports certificate generation.",
            );

        let mut sctp = RtcSctp::new(*mtu.start());
        if config.snap_enabled {
            sctp.enable_snap();
        }

        Ok(Rtc {
            state: RtcState::Alive,
            ice,
            dtls: Dtls::new(
                &dtls_cert,
                crypto_provider.dtls_provider,
                crypto_provider.sha256_provider,
                start,
                config.dtls_version,
                mtu,
            )
            .expect("DTLS to init without problem"),
            dtls_connected: false,
            dtls_buf: vec![0; 2000],
            next_dtls_timeout: None,
            session,
            sctp,
            chan: ChannelHandler::default(),
            stats: config.stats_interval.map(Stats::new),
            remote_fingerprint: None,
            remote_addrs: vec![],
            send_addr: None,
            need_init_time: true,
            last_now: start,
            peer_bytes_rx: 0,
            peer_bytes_tx: 0,
            change_counter: 0,
            last_timeout_reason: Reason::NotHappening,
            crypto_provider,
            fingerprint_verification: config.fingerprint_verification,
            close_dtls_started: false,
        })
    }

    /// Tests if this instance is still working.
    ///
    /// Certain events will straight away disconnect the `Rtc` instance, such as
    /// the DTLS fingerprint from the setup not matching that of the TLS negotiation
    /// (since that would potentially indicate a MITM attack!).
    ///
    /// The instance can be manually disconnected using [`Rtc::disconnect()`].
    ///
    /// During shutdown, this remains `true` until [`Rtc::poll_output()`]
    /// has drained pending local close output. App-facing operations such as
    /// [`Rtc::writer()`] and [`Rtc::channel()`] are unavailable during that
    /// closing drain.
    ///
    /// ```
    /// # use std::time::Instant;
    /// # use str0m::Rtc;
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// assert!(rtc.is_alive());
    ///
    /// rtc.disconnect();
    /// assert!(!rtc.is_alive());
    /// ```
    pub fn is_alive(&self) -> bool {
        self.state != RtcState::Closed
    }

    /// Force disconnects the instance making [`Rtc::is_alive()`] return `false`.
    ///
    /// This makes [`Rtc::poll_output`] and [`Rtc::handle_input`] go inert and not
    /// produce anymore network output or events.
    ///
    /// ```
    /// # use std::time::Instant;
    /// # use str0m::Rtc;
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// rtc.disconnect();
    /// assert!(!rtc.is_alive());
    /// ```
    pub fn disconnect(&mut self) {
        self.state = RtcState::Closed;
    }

    /// Add a local ICE candidate. Local candidates are socket addresses the `Rtc` instance
    /// use for communicating with the peer.
    ///
    /// If the candidate is accepted by the `Rtc` instance, it will return `Some` with a reference
    /// to it. You should then signal this candidate to the remote peer.
    ///
    /// This library has no built-in discovery of local network addresses on the host
    /// or NATed addresses via a STUN server or TURN server. The user of the library
    /// is expected to add new local candidates as they are discovered.
    ///
    /// In WebRTC lingo, the `Rtc` instance is permanently in a mode of [Trickle Ice][1]. It's
    /// however advisable to add at least one local candidate before starting the instance.
    ///
    /// ```
    /// # use std::time::Instant;
    /// # use str0m::{Rtc, Candidate};
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// let a = "127.0.0.1:5000".parse().unwrap();
    /// let c = Candidate::host(a, "udp").unwrap();
    ///
    /// rtc.add_local_candidate(c);
    /// ```
    ///
    /// [1]: https://www.rfc-editor.org/rfc/rfc8838.txt
    pub fn add_local_candidate(&mut self, c: Candidate) -> Option<&Candidate> {
        self.ice.add_local_candidate(c)
    }

    /// Add a remote ICE candidate. Remote candidates are addresses of the peer.
    ///
    /// For [`SdpApi`]: Remote candidates are typically added via
    /// receiving a remote [`SdpOffer`][change::SdpOffer] or [`SdpAnswer`][change::SdpAnswer].
    ///
    /// However for the case of [Trickle Ice][1], this is the way to add remote candidates
    /// that are "trickled" from the other side.
    ///
    /// ```
    /// # use std::time::Instant;
    /// # use str0m::{Rtc, Candidate};
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// let a = "1.2.3.4:5000".parse().unwrap();
    /// let c = Candidate::host(a, "udp").unwrap();
    ///
    /// rtc.add_remote_candidate(c);
    /// ```
    ///
    /// [1]: https://www.rfc-editor.org/rfc/rfc8838.txt
    pub fn add_remote_candidate(&mut self, c: Candidate) {
        self.ice.add_remote_candidate(c);
    }

    /// Checks if we are connected.
    ///
    /// This tests if we have ICE connection, DTLS and the SRTP crypto derived contexts are up.
    pub fn is_connected(&self) -> bool {
        self.ice.state().is_connected() && self.dtls_connected && self.session.is_connected()
    }

    /// The local and remote address of the nominated ICE pair media is sent on, if any.
    pub fn selected_pair(&self) -> Option<(SocketAddr, SocketAddr)> {
        self.send_addr
            .as_ref()
            .map(|send| (send.source, send.destination))
    }

    /// Make changes to the Rtc session via SDP.
    ///
    /// ```no_run
    /// # use std::time::Instant;
    /// # use str0m::Rtc;
    /// # use str0m::media::{MediaKind, Direction};
    /// # use str0m::change::SdpAnswer;
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// let mut changes = rtc.sdp_api();
    /// let mid_audio = changes.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
    /// let mid_video = changes.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
    ///
    /// let (offer, pending) = changes.apply().unwrap();
    /// let json = serde_json::to_vec(&offer).unwrap();
    ///
    /// // Send json OFFER to remote peer. Receive an answer back.
    /// let answer: SdpAnswer = todo!();
    ///
    /// rtc.sdp_api().accept_answer(pending, answer).unwrap();
    /// ```
    pub fn sdp_api(&mut self) -> SdpApi {
        SdpApi::new(self)
    }

    /// Makes direct changes to the Rtc session.
    ///
    /// This is a low level API. For "normal" use via SDP, see [`Rtc::sdp_api()`].
    pub fn direct_api(&mut self) -> DirectApi {
        DirectApi::new(self)
    }

    /// Send outgoing media data (frames) or request keyframes.
    ///
    /// Returns `None` if the direction isn't sending (`sendrecv` or `sendonly`).
    ///
    /// ```no_run
    /// # use std::time::Instant;
    /// # use str0m::Rtc;
    /// # use str0m::media::{MediaData, Mid};
    /// # use str0m::format::PayloadParams;
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// // add candidates, do SDP negotiation
    /// let mid: Mid = todo!(); // obtain mid from Event::MediaAdded.
    ///
    /// // Writer for this mid.
    /// let writer = rtc.writer(mid).unwrap();
    ///
    /// // Get incoming media data from another peer
    /// let data: MediaData = todo!();
    ///
    /// // Match incoming PT to an outgoing PT.
    /// let pt = writer.match_params(data.params).unwrap();
    ///
    /// writer.write(pt, data.network_time, data.time, data.data).unwrap();
    /// ```
    ///
    /// This is a frame level API: For RTP level see [`DirectApi::stream_tx()`]
    /// and [`DirectApi::stream_rx()`].
    ///
    pub fn writer(&mut self, mid: Mid) -> Option<Writer> {
        if self.session.rtp_mode {
            panic!("In rtp_mode use direct_api().stream_tx().write_rtp()");
        }

        if self.state != RtcState::Alive {
            return None;
        }

        // This does not catch potential RIDs required to send simulcast, but
        // it's a good start. An error might arise later on RID mismatch.
        self.session.media_by_mid_mut(mid)?;

        Some(Writer::new(&mut self.session, mid))
    }

    /// Currently configured media.
    ///
    /// Read only access. Changes are made via [`Rtc::sdp_api()`] or [`Rtc::direct_api()`].
    pub fn media(&self, mid: Mid) -> Option<&Media> {
        self.session.media_by_mid(mid)
    }

    fn init_dtls(&mut self, active: bool) -> Result<(), RtcError> {
        if self.dtls.is_inited() {
            return Ok(());
        }

        debug!("DTLS setup is: {:?}", active);
        self.dtls.set_active(active);

        // Initialize the DTLS state (client or server) before any operations
        // This ensures internal state like random (client) or last_now (server) is initialized
        self.dtls.handle_timeout(self.last_now)?;

        if active {
            // Drive handshake by sending an empty packet to trigger ClientHello
            let _ = self.dtls.handle_receive(&[]);
        }

        Ok(())
    }

    fn try_init_sctp(
        &mut self,
        client: bool,
        sctp_init_data: Option<SctpInitData>,
        remote_max_message_size: Option<u32>,
    ) -> Result<(), RtcError> {
        // If we got an m=application line, ensure we have negotiated the
        // SCTP association with the other side.
        if self.sctp.is_inited() {
            return Ok(());
        }

        self.sctp.init(
            client,
            self.last_now,
            sctp_init_data,
            remote_max_message_size,
        )?;
        Ok(())
    }

    /// Creates a new Mid that is not in the session already.
    pub(crate) fn new_mid(&self) -> Mid {
        loop {
            let mid = Mid::new();
            if !self.session.has_mid(mid) {
                break mid;
            }
        }
    }

    /// Poll the `Rtc` instance for output. Output can be three things, something to _Transmit_
    /// via a UDP socket (maybe via a TURN server). An _Event_, such as receiving media data,
    /// or a _Timeout_.
    ///
    /// The user of the library is expected to continuously call this function and deal with
    /// the output until it encounters an [`Output::Timeout`] at which point no further output
    /// is produced (if polled again, it will result in just another timeout).
    ///
    /// After exhausting the `poll_output`, the function will only produce more output again
    /// when one of two things happen:
    ///
    /// 1. The polled timeout is reached.
    /// 2. New network input.
    ///
    /// See [`Rtc`] instance documentation for how this is expected to be used in a loop.
    pub fn poll_output(&mut self) -> Result<Output, RtcError> {
        let o = self.do_poll_output()?;

        match &o {
            Output::Event(e) => match e {
                Event::ChannelData(_)
                | Event::MediaData(_)
                | Event::RtpPacket(_)
                | Event::SenderFeedback(_)
                | Event::MediaEgressStats(_)
                | Event::MediaIngressStats(_)
                | Event::PeerStats(_)
                | Event::ChannelBufferedAmountLow(_)
                | Event::EgressBitrateEstimate(_)
                | Event::KeyframeRequest(_) => {
                    trace!("{:?}", e)
                }
                _ => debug!("{:?}", e),
            },
            Output::Transmit(t) => {
                self.peer_bytes_tx += t.contents.len() as u64;
                trace!("OUT {:?}", t)
            }
            Output::Timeout(_t) => {}
        }

        Ok(o)
    }

    fn do_poll_output(&mut self) -> Result<Output, RtcError> {
        if self.state == RtcState::Closed {
            self.last_timeout_reason = Reason::NotHappening;
            return Ok(Output::Timeout(not_happening()));
        }

        while let Some(e) = self.ice.poll_event() {
            match e {
                IceAgentEvent::IceRestart(_) => {
                    //
                }
                IceAgentEvent::IceConnectionStateChange(v) => {
                    return Ok(Output::Event(Event::IceConnectionStateChange(v)));
                }
                IceAgentEvent::DiscoveredRecv { proto, source } => {
                    debug!("ICE remote address: {:?}/{:?}", Pii(source), proto);
                    self.remote_addrs.push(source);
                    while self.remote_addrs.len() > 20 {
                        self.remote_addrs.remove(0);
                    }
                }
                IceAgentEvent::NominatedSend {
                    proto,
                    source,
                    destination,
                } => {
                    debug!(
                        "ICE nominated send from: {:?} to: {:?} with protocol {:?}",
                        Pii(source),
                        Pii(destination),
                        proto,
                    );
                    self.send_addr = Some(SendAddr {
                        proto,
                        source,
                        destination,
                    });
                }
            }
        }

        // Handle DTLS timeout before polling output so any retransmit packets
        // queued by dimpl and the re-armed flight timer are picked up by the
        // poll loop below in the same iteration.
        if let Some(timeout) = self.next_dtls_timeout {
            if timeout <= self.last_now {
                self.next_dtls_timeout = None;

                if let Err(error) = self.dtls.handle_timeout(self.last_now) {
                    // A failed timer transition leaves DTLS unrecoverable. Close the
                    // RTC so continued polling cannot expose the same due timeout again.
                    self.disconnect();
                    return Err(error.into());
                }
            }
        }

        // Poll DTLS output - collect packets, handle events
        let mut just_connected = false;
        loop {
            match self.dtls.poll_output(&mut self.dtls_buf) {
                DtlsOutput::Packet(_) => {
                    unreachable!("We don't expect DTLS packets here since we use poll_packet");
                }
                DtlsOutput::Connected => {
                    if !self.dtls_connected {
                        debug!("DTLS connected");
                        self.dtls_connected = true;
                        just_connected = true;
                    }
                }
                DtlsOutput::KeyingMaterial(km, profile) => {
                    use config::KeyingMaterial;
                    let km_bytes = km.as_ref().to_vec();
                    debug!("DTLS set SRTP keying material and profile: {}", profile);
                    let active = self.dtls.is_active().expect("DTLS must be inited by now");
                    self.session.set_keying_material(
                        KeyingMaterial::new(&km_bytes),
                        &self.crypto_provider,
                        profile,
                        active,
                    );
                }
                DtlsOutput::PeerCert(der) => {
                    debug!("DTLS verify remote fingerprint");
                    // Compute fingerprint from peer's DER certificate
                    let fingerprint = crate::crypto::Fingerprint {
                        hash_func: "sha-256".to_string(),
                        bytes: self.crypto_provider.sha256_provider.sha256(der).to_vec(),
                    };
                    self.dtls.set_remote_fingerprint(fingerprint.clone());
                    if let Some(expected) = &self.remote_fingerprint {
                        if !self.fingerprint_verification {
                            debug!("DTLS fingerprint verification disabled");
                        } else if fingerprint != *expected {
                            self.disconnect();
                            return Err(RtcError::RemoteSdp("remote fingerprint no match".into()));
                        }
                    } else {
                        self.disconnect();
                        return Err(RtcError::RemoteSdp("no a=fingerprint before dtls".into()));
                    }
                }
                DtlsOutput::ApplicationData(data) => {
                    self.sctp.handle_input(self.last_now, data);
                }
                DtlsOutput::Timeout(t) => {
                    self.next_dtls_timeout = Some(t);
                    break;
                }
                DtlsOutput::CloseNotify => {
                    self.start_close()?;
                    return Ok(Output::Event(Event::Closed));
                }
                other => {
                    return Err(RtcError::Dtls(DtlsError::Io(std::io::Error::other(
                        format!("Unexpected DTLS output: {other:?}"),
                    ))));
                }
            }
        }

        if just_connected {
            return Ok(Output::Event(Event::Connected));
        }

        while let Some(e) = self.sctp.poll() {
            match e {
                SctpEvent::Transmit { mut packets } => {
                    if let Some(v) = packets.front() {
                        if let Err(e) = self.dtls.handle_input(v) {
                            if is_would_block(&e) {
                                self.sctp.push_back_transmit(packets);
                                break;
                            } else if self.state == RtcState::Closing {
                                debug!(
                                    "Dropping SCTP transmit while closing after DTLS error: {e}"
                                );
                                packets.pop_front();
                                if !packets.is_empty() {
                                    self.sctp.push_back_transmit(packets);
                                }
                                return self.do_poll_output();
                            } else {
                                return Err(e.into());
                            }
                        }

                        packets.pop_front();
                        // If there are still packets, they are sent on next
                        // poll_output()
                        if !packets.is_empty() {
                            self.sctp.push_back_transmit(packets);
                        }

                        // Run again since this would feed the DTLS subsystem
                        // to produce a packet now.
                        return self.do_poll_output();
                    }
                }
                SctpEvent::Open { id, label } => {
                    self.chan.ensure_channel_id_for(id);
                    let id = self.chan.channel_id_by_stream_id(id).unwrap();
                    return Ok(Output::Event(Event::ChannelOpen(id, label)));
                }
                SctpEvent::Close {
                    id: stream_id,
                    reset_pending,
                } => {
                    let Some(channel_id) = self.chan.channel_id_by_stream_id(stream_id) else {
                        warn!("Drop ChannelClose event for id: {:?}", stream_id);
                        continue;
                    };
                    // When a reset is outstanding, remove_channel holds the stream id
                    // back from reallocation until the handshake completes.
                    self.chan.remove_channel(channel_id, reset_pending);
                    return Ok(Output::Event(Event::ChannelClose(channel_id)));
                }
                SctpEvent::StreamResetComplete { id } => {
                    // The reset handshake completed, the stream id can be used again.
                    self.chan.stream_reset_complete(id);
                    continue;
                }
                SctpEvent::AssociationLost => {
                    self.chan.association_lost();
                    self.start_close()?;
                    return Ok(Output::Event(Event::Closed));
                }
                SctpEvent::Data { id, binary, data } => {
                    let Some(id) = self.chan.channel_id_by_stream_id(id) else {
                        warn!("Drop ChannelData event for id: {:?}", id);
                        continue;
                    };
                    let cd = ChannelData { id, binary, data };
                    return Ok(Output::Event(Event::ChannelData(cd)));
                }
                SctpEvent::BufferedAmountLow { id } => {
                    let Some(id) = self.chan.channel_id_by_stream_id(id) else {
                        warn!("Drop BufferedAmountLow for id: {:?}", id);
                        continue;
                    };
                    return Ok(Output::Event(Event::ChannelBufferedAmountLow(id)));
                }
            }
        }

        if let Some(ev) = self.session.poll_event() {
            return Ok(Output::Event(ev));
        }

        // Some polling needs to bubble up errors.
        if let Some(ev) = self.session.poll_event_fallible()? {
            return Ok(Output::Event(ev));
        }

        if let Some(e) = self.stats.as_mut().and_then(|s| s.poll_output()) {
            return Ok(match e {
                StatsEvent::Peer(s) => Output::Event(Event::PeerStats(s)),
                StatsEvent::MediaIngress(s) => Output::Event(Event::MediaIngressStats(s)),
                StatsEvent::MediaEgress(s) => Output::Event(Event::MediaEgressStats(s)),
            });
        }

        if let Some(v) = self.ice.poll_transmit() {
            return Ok(Output::Transmit(v));
        }

        if let Some(send) = &self.send_addr {
            // These can only be sent after we got an ICE connection.
            let datagram = None
                .or_else(|| self.dtls.poll_packet())
                .or_else(|| self.session.poll_datagram(self.last_now));

            if let Some(contents) = datagram {
                let t = net::Transmit {
                    proto: send.proto,
                    source: send.source,
                    destination: send.destination,
                    contents,
                };
                return Ok(Output::Transmit(t));
            }
        } else {
            // Don't allow accumulated feedback to build up indefinitely
            self.session.clear_feedback();
        }

        let stats_timeout = self.stats.as_mut().and_then(|s| s.poll_timeout());

        let time_and_reason = (None, Reason::NotHappening)
            .soonest((self.next_dtls_timeout, Reason::DTLS))
            .soonest((self.ice.poll_timeout(), Reason::Ice))
            .soonest(self.session.poll_timeout())
            .soonest((self.sctp.poll_timeout(), Reason::Sctp))
            .soonest((self.chan.poll_timeout(&self.sctp), Reason::Channel))
            .soonest((stats_timeout, Reason::Stats));

        // trace!("poll_output timeout reason: {}", time_and_reason.1);

        let time = time_and_reason.0.unwrap_or_else(not_happening);
        let reason = time_and_reason.1;

        // We want to guarantee time doesn't go backwards.
        let next = if time < self.last_now {
            self.last_now
        } else {
            time
        };

        if self.state == RtcState::Closing && self.close_drain_complete() {
            self.state = RtcState::Closed;
            self.last_timeout_reason = Reason::NotHappening;
            return Ok(Output::Timeout(not_happening()));
        }

        self.last_timeout_reason = reason;
        Ok(Output::Timeout(next))
    }

    /// The reason for the last [`Output::Timeout`]
    ///
    /// This is updated when calling [`Rtc::poll_output()`] and the next output
    /// is a timeout.
    ///
    /// ```
    /// # use str0m::{Rtc, Reason};
    /// # use std::time::Instant;
    /// let rtc = Rtc::new(Instant::now());
    ///
    /// // Before any call to poll_output(), the reason is the default.
    /// assert_eq!(rtc.last_timeout_reason(), Reason::NotHappening);
    /// ```
    pub fn last_timeout_reason(&self) -> Reason {
        self.last_timeout_reason
    }

    /// Check if this `Rtc` instance accepts the given input. This is used for demultiplexing
    /// several `Rtc` instances over the same UDP server socket.
    ///
    /// [`Input::Timeout`] is always accepted. [`Input::Receive`] is tested against the nominated
    /// ICE candidate. If that doesn't match and the incoming data is a STUN packet, the accept call
    /// is delegated to the ICE agent which recognizes the remote peer from `a=ufrag`/`a=password`
    /// credentials negotiated in the SDP. If that also doesn't match, all remote ICE candidates are
    /// checked for a match.
    ///
    /// In a server setup, the server would try to find an `Rtc` instances using [`Rtc::accepts()`].
    /// The first found instance would be given the input via [`Rtc::handle_input()`].
    ///
    /// ```no_run
    /// # use std::time::Instant;
    /// # use str0m::{Rtc, Input};
    /// // A vec holding the managed rtc instances. One instance per remote peer.
    /// let now = Instant::now();
    /// let mut rtcs = vec![Rtc::new(now), Rtc::new(now), Rtc::new(now)];
    ///
    /// // Configure instances with local ice candidates etc.
    ///
    /// loop {
    ///     // TODO poll_timeout() and handle the output.
    ///
    ///     let input: Input = todo!(); // read network data from socket.
    ///     for rtc in &mut rtcs {
    ///         if rtc.accepts(&input) {
    ///             rtc.handle_input(input).unwrap();
    ///         }
    ///     }
    /// }
    /// ```
    pub fn accepts(&self, input: &Input) -> bool {
        let Input::Receive(_, r) = input else {
            // always accept the Input::Timeout.
            return true;
        };

        // Fast path: DTLS, RTP, and RTCP traffic coming in from the same socket address
        // we've nominated for sending via the ICE agent. This is the typical case
        if let Some(send_addr) = &self.send_addr {
            if r.source == send_addr.destination {
                return true;
            }
        }

        // STUN can use the ufrag/password to identify that a message belongs
        // to this Rtc instance.
        if let DatagramRecvInner::Stun(v) = &r.contents.inner {
            return self.ice.accepts_message(v);
        }

        // Slow path: Occasionally, traffic comes in on a socket address corresponding
        // to a successful candidate pair other than the one we've currently nominated.
        // This typically happens at the beginning of the connection
        if self.ice.has_viable_remote_candidate(r.source) {
            return true;
        }

        false
    }

    /// Provide input to this `Rtc` instance. Input is either a [`Input::Timeout`] for some
    /// time that was previously obtained from [`Rtc::poll_output()`], or [`Input::Receive`]
    /// for network data.
    ///
    /// Both the timeout and the network data contains a [`std::time::Instant`] which drives
    /// time forward in the instance. For network data, the intention is to record the time
    /// of receiving the network data as precise as possible. This time is used to calculate
    /// things like jitter and bandwidth.
    ///
    /// It's always okay to call [`Rtc::handle_input()`] with a timeout, also before the
    /// time obtained via [`Rtc::poll_output()`].
    ///
    /// ```no_run
    /// # use str0m::{Rtc, Input};
    /// # use std::time::Instant;
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// loop {
    ///     let timeout: Instant = todo!(); // rtc.poll_output() until we get a timeout.
    ///
    ///     let input: Input = todo!(); // wait for network data or timeout.
    ///     rtc.handle_input(input);
    /// }
    /// ```
    pub fn handle_input(&mut self, input: Input) -> Result<(), RtcError> {
        if self.state == RtcState::Closed {
            return Ok(());
        }

        match input {
            Input::Timeout(now) => self.do_handle_timeout(now)?,
            Input::Receive(recv_time, r) => {
                self.do_handle_receive(recv_time, r)?;
                self.do_handle_timeout(recv_time)?;
            }
        }
        Ok(())
    }

    /// Starts local shutdown.
    ///
    /// The `Rtc` instance schedules RTCP BYE, requests SCTP shutdown, and sends
    /// DTLS close_notify without waiting for remote replies. It enters a closing
    /// drain state where app-facing operations stop, but [`Rtc::poll_output()`]
    /// continues polling until pending local close output has drained. Once the
    /// drain is complete, [`Rtc::is_alive()`] returns `false`.
    pub fn close(&mut self) -> Result<(), RtcError> {
        self.start_close()
    }

    fn start_close(&mut self) -> Result<(), RtcError> {
        if self.state == RtcState::Closed {
            return Ok(());
        }

        if self.state != RtcState::Closing {
            self.session.close_rtp();
            self.sctp.close()?;
            self.state = RtcState::Closing;
        }

        self.start_dtls_close()
    }

    fn start_dtls_close(&mut self) -> Result<(), RtcError> {
        if !self.close_dtls_started {
            self.dtls.close()?;
            self.close_dtls_started = true;
        }

        Ok(())
    }

    fn close_drain_complete(&self) -> bool {
        self.close_dtls_started && self.dtls.is_closed() && self.session.is_rtp_closed()
    }

    fn init_time(&mut self, now: Instant) {
        // The operation is somewhat expensive, hence we only do it once.
        if !self.need_init_time {
            return;
        }

        // We assume this first "now" is a time 0 start point for calculating ntp/unix time offsets.
        // This initializes the conversion of Instant -> NTP/Unix time.
        let _ = now.to_unix_duration();

        self.need_init_time = false;
    }

    fn do_handle_timeout(&mut self, now: Instant) -> Result<(), RtcError> {
        self.init_time(now);

        // Prevent time from going backwards.
        if now < self.last_now {
            return Ok(());
        }

        self.last_now = now;
        self.ice.handle_timeout(now);
        self.sctp.handle_timeout(now);
        self.chan.handle_timeout(now, &mut self.sctp);
        self.session.handle_timeout(now)?;

        if let Some(stats) = &mut self.stats {
            if stats.wants_timeout(now) {
                let mut snapshot = StatsSnapshot::new(now);
                snapshot.peer_rx = self.peer_bytes_rx;
                snapshot.peer_tx = self.peer_bytes_tx;
                let current_round_trip_time = self.ice.nominated_pair_rtt();
                let total_round_trip_time = self.ice.nominated_pair_total_rtt().unwrap_or_default();
                let responses_received = self.ice.nominated_pair_responses_received().unwrap_or(0);
                snapshot.selected_candidate_pair =
                    self.send_addr.as_ref().map(|s| CandidatePairStats {
                        protocol: s.proto,
                        local: CandidateStats { addr: s.source },
                        remote: CandidateStats {
                            addr: s.destination,
                        },
                        current_round_trip_time,
                        total_round_trip_time,
                        responses_received,
                    });
                self.session.visit_stats(now, &mut snapshot);
                stats.do_handle_timeout(&mut snapshot);
            }
        }

        Ok(())
    }

    fn do_handle_receive(&mut self, recv_time: Instant, r: net::Receive) -> Result<(), RtcError> {
        trace!("IN {:?}", r);
        use DatagramRecvInner::*;

        let bytes_rx = match r.contents.inner {
            // TODO: stun is already parsed (depacketized) here
            Stun(_) => 0,
            Dtls(v) | Rtp(v) | Rtcp(v) => v.len(),
        };

        self.peer_bytes_rx += bytes_rx as u64;

        match r.contents.inner {
            Stun(stun) => {
                let packet = is::stun::StunPacket {
                    proto: r.proto,
                    source: r.source,
                    destination: r.destination,
                    message: stun,
                };
                self.ice.handle_packet(recv_time, packet);
            }
            Dtls(dtls) => self.dtls.handle_receive(dtls)?,
            Rtp(rtp) => self.session.handle_rtp_receive(recv_time, rtp),
            Rtcp(rtcp) => self.session.handle_rtcp_receive(recv_time, rtcp),
        }

        Ok(())
    }

    /// Obtain handle for writing to a data channel.
    ///
    /// This is first available when a [`ChannelId`] is advertised via [`Event::ChannelOpen`].
    /// The function returns `None` also for IDs from [`SdpApi::add_channel()`].
    ///
    /// Incoming channel data is via the [`Event::ChannelData`] event.
    ///
    /// ```no_run
    /// # use std::time::Instant;
    /// # use str0m::{Rtc, channel::ChannelId};
    /// let mut rtc = Rtc::new(Instant::now());
    ///
    /// let cid: ChannelId = todo!(); // obtain channel id from Event::ChannelOpen
    /// let channel = rtc.channel(cid).unwrap();
    /// // TODO write data channel data.
    /// ```
    pub fn channel(&mut self, id: ChannelId) -> Option<Channel<'_>> {
        if self.state != RtcState::Alive {
            return None;
        }

        let sctp_stream_id = self.chan.stream_id_by_channel_id(id)?;

        if !self.sctp.is_open(sctp_stream_id) {
            return None;
        }

        Some(Channel::new(sctp_stream_id, self))
    }

    /// Configure the Bandwidth Estimate (BWE) subsystem.
    ///
    /// Only relevant if BWE was enabled in the [`RtcConfig::enable_bwe()`]
    pub fn bwe(&mut self) -> Bwe {
        Bwe(self)
    }

    fn is_correct_change_id(&self, change_id: usize) -> bool {
        self.change_counter == change_id + 1
    }

    fn next_change_id(&mut self) -> usize {
        let n = self.change_counter;
        self.change_counter += 1;
        n
    }

    /// The codec configs for sending/receiving data.
    ///
    /// The configurations can be set with [`RtcConfig`] before setting up the session, and they
    /// might be further updated by SDP negotiation.
    pub fn codec_config(&self) -> &CodecConfig {
        &self.session.codec_config
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IceConnectionStateChange(l0), Self::IceConnectionStateChange(r0)) => l0 == r0,
            (Self::MediaAdded(m0), Self::MediaAdded(m1)) => m0 == m1,
            (Self::MediaData(m1), Self::MediaData(m2)) => m1 == m2,
            (Self::ChannelOpen(l0, l1), Self::ChannelOpen(r0, r1)) => l0 == r0 && l1 == r1,
            (Self::ChannelData(l0), Self::ChannelData(r0)) => l0 == r0,
            (Self::ChannelClose(l0), Self::ChannelClose(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl Eq for Event {}

impl fmt::Debug for Rtc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rtc").finish()
    }
}

/// Log a CSV like stat to stdout.
///
/// ```ignore
/// log_stat!("MY_STAT", 1, "hello", 3);
/// ```
///
/// will result in the following being printed
///
/// ```text
/// MY_STAT 1, hello, 3, {unix_timestamp_ms}
/// ````
///
/// These logs can be easily grepped for, parsed and graphed, or otherwise analyzed.
///
/// This macro turns into a NO-OP if the `_internal_dont_use_log_stats` feature is not enabled
macro_rules! log_stat {
    ($name:expr, $($arg:expr),+) => {
        #[cfg(feature = "_internal_dont_use_log_stats")]
        {
            use std::time::SystemTime;
            use std::io::{self, Write};

            let now = SystemTime::now();
            let since_epoch = now.duration_since(SystemTime::UNIX_EPOCH).unwrap();
            let unix_time_ms = since_epoch.as_millis();
            let mut lock = io::stdout().lock();
            write!(lock, "{} ", $name).expect("Failed to write to stdout");

            $(
                write!(lock, "{},", $arg).expect("Failed to write to stdout");
            )+
            writeln!(lock, "{}", unix_time_ms).expect("Failed to write to stdout");
        }
    };
}
pub(crate) use log_stat;

#[cfg(test)]
#[doc(hidden)]
pub fn init_crypto_default() {
    crate::crypto::from_feature_flags().install_process_default();
}

#[cfg(test)]
mod test {
    use std::panic::UnwindSafe;

    use super::*;

    #[test]
    fn rtc_is_send() {
        fn is_send<T: Send>(_t: T) {}
        fn is_sync<T: Sync>(_t: T) {}
        is_send(Rtc::new(Instant::now()));
        is_sync(Rtc::new(Instant::now()));
    }

    #[test]
    fn rtc_is_unwind_safe() {
        fn is_unwind_safe<T: UnwindSafe>(_t: T) {}
        is_unwind_safe(Rtc::new(Instant::now()));
    }

    #[test]
    fn event_is_reasonably_sized() {
        let n = std::mem::size_of::<Event>();
        assert!(n < 490); // Increased to accommodate abs-capture-time fields in ExtensionValues
    }
}

#[cfg(feature = "_internal_test_exports")]
#[allow(missing_docs)]
pub mod _internal_test_exports;

#[cfg(feature = "unversioned")]
pub mod unversioned {
    //! This module provides functionality that is not versioned according to semver.
    //! It may change in breaking ways between minor/patch releases, there are no guarantees.
    //! USE AT YOUR OWN RISK.
    //!
    //! To use this module, enable the `unversioned` feature flag in your Cargo.toml.

    pub use super::packet::{
        Depacketizer, H264Depacketizer, H264Packetizer, OpusPacketizer, Packetizer,
        Vp8Depacketizer, Vp8Packetizer,
    };
}
