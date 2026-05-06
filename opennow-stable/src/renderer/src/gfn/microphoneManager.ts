/**
 * Microphone Manager - Handles microphone capture and state management
 * Following the pattern from the official GeForce NOW client
 */

export type MicState =
  | "uninitialized"
  | "permission_pending"
  | "permission_denied"
  | "started"
  | "no_suitable_device"
  | "stopped"
  | "unsupported"
  | "error";

export interface MicStateChange {
  state: MicState;
  deviceLabel?: string;
}

export class MicrophoneManager {
  private micStream: MediaStream | null = null;
  private placeholderStream: MediaStream | null = null;
  private currentState: MicState = "uninitialized";
  private pc: RTCPeerConnection | null = null;
  private micSender: RTCRtpSender | null = null;
  private deviceId: string = "";
  private micLevel = 1;
  private onStateChangeCallback: ((state: MicStateChange) => void) | null = null;
  private sampleRate: number = 48000; // Official client uses 48kHz

  /** Web Audio graph: raw getUserMedia → GainNode → track sent to WebRTC (`volume` constraints are rarely supported for input). */
  private micProcessCtx: AudioContext | null = null;
  private micMediaSource: MediaStreamAudioSourceNode | null = null;
  private micGainNode: GainNode | null = null;
  private outboundMicTrack: MediaStreamTrack | null = null;

  // Track if we should auto-retry with different devices on failure
  private attemptedDevices: Set<string> = new Set();
  private readonly handleMicStreamInactive = (): void => {
    console.log("[Microphone] Stream inactive");
    this.tearDownMicProcessing();
    this.detachMicStreamListeners(this.micStream);
    this.attemptedDevices.clear();
    this.micStream = null;
  };

  /**
   * Check if microphone is supported in this browser
   */
  static isSupported(): boolean {
    return !!(
      navigator.mediaDevices &&
      typeof navigator.mediaDevices.getUserMedia === "function" &&
      typeof navigator.mediaDevices.enumerateDevices === "function"
    );
  }

  /**
   * Check microphone permission state without prompting
   */
  async checkPermissionState(): Promise<PermissionState | null> {
    if (!navigator.permissions) {
      return null;
    }
    try {
      const result = await navigator.permissions.query({ name: "microphone" as PermissionName });
      return result.state;
    } catch {
      return null;
    }
  }

  /**
   * Set callback for state changes
   */
  setOnStateChange(callback: (state: MicStateChange) => void): void {
    this.onStateChangeCallback = callback;
  }

  /**
   * Get current microphone state
   */
  getState(): MicState {
    return this.currentState;
  }

  /**
   * Set the peer connection to use for adding mic tracks
   */
  setPeerConnection(pc: RTCPeerConnection | null): void {
    this.pc = pc;
  }

  /**
   * Attach microphone sender to the peer connection.
   * If real mic is not ready yet, arm a silent placeholder so m=audio(mid=3)
   * negotiates as sendrecv/sendonly in the initial answer.
   */
  async attachTrackToPeerConnection(): Promise<void> {
    if (!this.pc) {
      return;
    }
    if (this.outboundMicTrack && this.outboundMicTrack.readyState === "live") {
      await this.addTrackToPeerConnection(this.outboundMicTrack);
      return;
    }
    if (this.micStream) {
      const out = this.buildMicProcessingPipeline(this.micStream);
      if (out) {
        await this.addTrackToPeerConnection(out);
        return;
      }
      const raw = this.micStream.getAudioTracks()[0];
      if (raw) {
        await this.addTrackToPeerConnection(raw);
        void this.applyTrackMicLevel(raw);
        return;
      }
    }
    await this.ensurePlaceholderSender();
  }

  /**
   * Set device ID to use (empty = default)
   */
  setDeviceId(deviceId: string): void {
    this.deviceId = deviceId;
  }

  /**
   * Enumerate available audio input devices
   */
  async enumerateDevices(): Promise<MediaDeviceInfo[]> {
    try {
      const permission = await this.checkPermissionState();
      if (permission === "denied") {
        const devices = await navigator.mediaDevices.enumerateDevices();
        return devices.filter(device => device.kind === "audioinput");
      }

      const tempStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      tempStream.getTracks().forEach(track => track.stop());

      const devices = await navigator.mediaDevices.enumerateDevices();
      return devices.filter(device => device.kind === "audioinput");
    } catch {
      // If permission denied, return devices without labels
      try {
        const devices = await navigator.mediaDevices.enumerateDevices();
        return devices.filter(device => device.kind === "audioinput");
      } catch {
        return [];
      }
    }
  }

  /**
   * Initialize microphone with specified device
   */
  async initialize(): Promise<boolean> {
    if (!MicrophoneManager.isSupported()) {
      this.setState("unsupported");
      return false;
    }

    // Check current permission state
    const permission = await this.checkPermissionState();
    if (permission === "denied") {
      this.setState("permission_denied");
      return false;
    }

    this.setState("permission_pending");
    this.attemptedDevices.clear();

    try {
      await this.startCapture();
      return true;
    } catch (error) {
      console.error("[Microphone] Failed to initialize:", error);
      return false;
    }
  }

  /**
   * Start microphone capture
   */
  private async startCapture(): Promise<void> {
    this.clearMicStream();

    const constraints: MediaStreamConstraints = {
      audio: {
        sampleRate: { ideal: this.sampleRate },
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
        channelCount: 1,
      } as MediaTrackConstraints,
    };

    // Add deviceId constraint if specified
    if (this.deviceId) {
      (constraints.audio as MediaTrackConstraints).deviceId = { exact: this.deviceId };
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia(constraints);
      this.attachMicStream(stream);
      const track = stream.getAudioTracks()[0];

      if (!track) {
        throw new Error("No audio track available");
      }

      // Set up track ended handler
      track.onended = () => {
        console.log("[Microphone] Track ended");
        this.stop();
      };

      const outbound = this.buildMicProcessingPipeline(stream);
      const sendTrack = outbound ?? track;
      if (!outbound) {
        void this.applyTrackMicLevel(track);
      }

      if (this.pc) {
        await this.addTrackToPeerConnection(sendTrack);
      }

      this.setState("started", track.label);
    } catch (error) {
      await this.handleCaptureError(error, constraints);
    }
  }

  /**
   * Handle capture errors with fallback logic
   */
  private async handleCaptureError(error: unknown, constraints: MediaStreamConstraints): Promise<void> {
    const deviceId = (constraints.audio as MediaTrackConstraints)?.deviceId;
    const attemptedDevice = typeof deviceId === "object" && "exact" in deviceId
      ? deviceId.exact
      : "default";

    if (error instanceof DOMException) {
      switch (error.name) {
        case "NotAllowedError":
          console.error("[Microphone] Permission denied");
          this.setState("permission_denied");
          throw error;

        case "NotFoundError":
          console.error("[Microphone] No suitable device found");
          this.setState("no_suitable_device");
          throw error;

        case "NotReadableError":
          // Device in use or hardware error - try another device
          this.attemptedDevices.add(attemptedDevice as string);
          console.warn("[Microphone] Device not readable, trying alternative:", attemptedDevice);

          try {
            const devices = await navigator.mediaDevices.enumerateDevices();
            const audioInputs = devices.filter(d => d.kind === "audioinput" && !this.attemptedDevices.has(d.deviceId));

            if (audioInputs.length > 0 && audioInputs[0]?.deviceId) {
              console.log("[Microphone] Trying device:", audioInputs[0].label);
              this.deviceId = audioInputs[0].deviceId;
              await this.startCapture();
              return;
            }
          } catch (enumError) {
            console.error("[Microphone] Enumerate devices failed:", enumError);
          }

          this.setState("error");
          throw error;

        case "OverconstrainedError":
          // Try without sample rate constraint
          console.warn("[Microphone] Constraints not supported, trying with basic constraints");
          try {
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
            this.attachMicStream(stream);
            const track = stream.getAudioTracks()[0];
            if (!track) {
              throw new Error("No audio track available");
            }
            track.onended = () => {
              console.log("[Microphone] Track ended");
              this.stop();
            };
            const outbound = this.buildMicProcessingPipeline(stream);
            const sendTrack = outbound ?? track;
            if (!outbound) {
              void this.applyTrackMicLevel(track);
            }
            if (this.pc && sendTrack) {
              await this.addTrackToPeerConnection(sendTrack);
            }
            this.setState("started", track?.label);
            return;
          } catch (fallbackError) {
            this.setState("error");
            throw fallbackError;
          }

        default:
          console.error("[Microphone] Capture error:", error.name, error.message);
          this.setState("error");
          throw error;
      }
    }

    this.setState("error");
    throw error;
  }

  /**
   * Add audio track to peer connection
   */
  private async addTrackToPeerConnection(track: MediaStreamTrack): Promise<void> {
    if (!this.pc) {
      console.warn("[Microphone] No peer connection available");
      return;
    }

    const transceivers = this.pc.getTransceivers();
    const audioTransceivers = transceivers.filter((t) => {
      const receiverKind = t.receiver?.track?.kind;
      const senderKind = t.sender?.track?.kind;
      return receiverKind === "audio" || senderKind === "audio";
    });

    // Prefer the dedicated mic m-line if present; otherwise pick an already-negotiated
    // audio transceiver with an empty sender so we don't create a new unnegotiated one.
    const micTransceiver =
      audioTransceivers.find((t) => t.mid === "3")
      ?? audioTransceivers.find((t) => !t.sender.track)
      ?? audioTransceivers.find(
        (t) => t.direction === "sendrecv" || t.direction === "recvonly" || t.direction === "inactive",
      );

    if (micTransceiver) {
      if (micTransceiver.direction === "recvonly") {
        micTransceiver.direction = "sendrecv";
      } else if (micTransceiver.direction === "inactive") {
        micTransceiver.direction = "sendonly";
      }
      console.log("[Microphone] Attaching track to mic transceiver", micTransceiver.mid ?? "(no mid)");
      await micTransceiver.sender.replaceTrack(track);
      this.micSender = micTransceiver.sender;
      return;
    }

    // Fallback: replace any existing audio sender before creating a new one.
    const senders = this.pc.getSenders();
    const existingAudioSender = senders.find((s) => s.track?.kind === "audio");

    if (existingAudioSender) {
      console.log("[Microphone] Replacing existing audio track");
      await existingAudioSender.replaceTrack(track);
      this.micSender = existingAudioSender;
    } else {
      console.warn("[Microphone] No negotiated audio sender found; adding new track (may require renegotiation)");
      this.micSender = this.pc.addTrack(track, new MediaStream([track]));
    }
  }

  /**
   * Ensure a negotiated sender exists even before mic permission/capture succeeds.
   * This mirrors official behavior: seed sender with a silent track, then replaceTrack(realMic).
   */
  private async ensurePlaceholderSender(): Promise<void> {
    if (!this.pc) {
      return;
    }

    const placeholderTrack = this.getOrCreatePlaceholderTrack();
    if (!placeholderTrack) {
      console.warn("[Microphone] Failed to create placeholder mic track");
      return;
    }
    await this.addTrackToPeerConnection(placeholderTrack);
  }

  private getOrCreatePlaceholderTrack(): MediaStreamTrack | null {
    let track = this.placeholderStream?.getAudioTracks()[0] ?? null;
    if (track) {
      return track;
    }

    const AudioCtx = window.AudioContext || (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioCtx) {
      return null;
    }

    try {
      const ctx = new AudioCtx({ sampleRate: this.sampleRate });
      const destination = ctx.createMediaStreamDestination();
      track = destination.stream.getAudioTracks()[0] ?? null;
      void ctx.close();

      if (!track) {
        return null;
      }
      track.enabled = true;
      this.placeholderStream = new MediaStream([track]);
      return track;
    } catch (error) {
      console.warn("[Microphone] Placeholder stream creation failed:", error);
      return null;
    }
  }

  private attachMicStream(stream: MediaStream): void {
    this.detachMicStreamListeners(this.micStream);
    this.micStream = stream;
    stream.addEventListener("inactive", this.handleMicStreamInactive);
  }

  private detachMicStreamListeners(stream: MediaStream | null): void {
    if (!stream) {
      return;
    }
    stream.removeEventListener("inactive", this.handleMicStreamInactive);
    for (const track of stream.getTracks()) {
      track.onended = null;
    }
  }

  private tearDownMicProcessing(): void {
    try {
      this.micMediaSource?.disconnect();
      this.micMediaSource = null;
      this.micGainNode?.disconnect();
      this.micGainNode = null;
      this.outboundMicTrack = null;
      const ctx = this.micProcessCtx;
      this.micProcessCtx = null;
      if (ctx && ctx.state !== "closed") {
        void ctx.close();
      }
    } catch {
      // ignore
    }
  }

  /**
   * Build mic → GainNode → MediaStreamDestination; {@link outboundMicTrack} is what we attach to WebRTC.
   * Raw {@link micStream} remains for mute state and {@link getTrack} (UI meter).
   */
  private buildMicProcessingPipeline(rawStream: MediaStream): MediaStreamTrack | null {
    this.tearDownMicProcessing();
    if (!rawStream.getAudioTracks()[0]) {
      return null;
    }

    const AudioCtx = window.AudioContext || (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioCtx) {
      return null;
    }

    try {
      let ctx: AudioContext;
      try {
        ctx = new AudioCtx({ sampleRate: this.sampleRate });
      } catch {
        ctx = new AudioCtx();
      }
      void ctx.resume().catch(() => undefined);

      const src = ctx.createMediaStreamSource(rawStream);
      const gain = ctx.createGain();
      gain.gain.value = this.micLevel;
      const dest = ctx.createMediaStreamDestination();
      src.connect(gain).connect(dest);

      const out = dest.stream.getAudioTracks()[0] ?? null;
      if (!out) {
        void ctx.close();
        return null;
      }

      this.micProcessCtx = ctx;
      this.micMediaSource = src;
      this.micGainNode = gain;
      this.outboundMicTrack = out;
      return out;
    } catch (e) {
      console.warn("[Microphone] Web Audio gain pipeline failed:", e);
      this.tearDownMicProcessing();
      return null;
    }
  }

  private clearMicStream(): void {
    this.tearDownMicProcessing();
    if (!this.micStream) {
      return;
    }
    const stream = this.micStream;
    this.detachMicStreamListeners(stream);
    stream.getTracks().forEach((track) => track.stop());
    this.micStream = null;
  }

  /**
   * Enable/disable microphone track (mute/unmute)
   */
  setEnabled(enabled: boolean): void {
    if (!this.micStream) {
      if (enabled && this.currentState !== "started") {
        this.initialize();
      }
      return;
    }

    const track = this.micStream.getAudioTracks()[0];
    if (track) {
      track.enabled = enabled;
      console.log(`[Microphone] ${enabled ? "Unmuted" : "Muted"}`);

      if (enabled && this.currentState === "stopped") {
        this.setState("started", track.label);
      } else if (!enabled && this.currentState === "started") {
        this.setState("stopped");
      }
    }
  }

  setMicLevel(level01: number): void {
    this.micLevel = Math.max(0, Math.min(1, Number.isFinite(level01) ? level01 : 1));
    if (this.micGainNode) {
      this.micGainNode.gain.value = this.micLevel;
      return;
    }
    const track = this.micStream?.getAudioTracks()[0] ?? null;
    if (track) {
      void this.applyTrackMicLevel(track);
    }
  }

  getMicLevel(): number {
    return this.micLevel;
  }

  /**
   * Check if microphone is currently enabled (unmuted)
   */
  isEnabled(): boolean {
    if (!this.micStream) return false;
    const track = this.micStream.getAudioTracks()[0];
    return track?.enabled ?? false;
  }

  /**
   * Stop microphone capture
   */
  stop(): void {
    console.log("[Microphone] Stopping capture");

    if (this.micSender && this.pc) {
      try {
        // Keep sender negotiated by falling back to a silent track.
        const placeholderTrack = this.getOrCreatePlaceholderTrack();
        if (placeholderTrack) {
          this.micSender.replaceTrack(placeholderTrack).catch(() => {});
        } else {
          this.micSender.replaceTrack(null).catch(() => {});
        }
      } catch {
        // Ignore errors
      }
    }

    this.clearMicStream();

    this.attemptedDevices.clear();
    this.setState("stopped");
  }

  /**
   * Dispose and cleanup
   */
  dispose(): void {
    this.stop();
    if (this.placeholderStream) {
      this.placeholderStream.getTracks().forEach((track) => track.stop());
      this.placeholderStream = null;
    }
    this.micSender = null;
    this.pc = null;
    this.onStateChangeCallback = null;
  }

  /**
   * MediaStreamTrack suitable for metering and local monitoring: post-gain audio that matches
   * what is attached to the WebRTC sender when the processing pipeline is active; otherwise
   * the raw capture track (fallback when Web Audio routing failed).
   */
  getTrack(): MediaStreamTrack | null {
    if (this.outboundMicTrack && this.outboundMicTrack.readyState === "live") {
      return this.outboundMicTrack;
    }
    return this.micStream?.getAudioTracks()[0] ?? null;
  }

  private async applyTrackMicLevel(track: MediaStreamTrack): Promise<void> {
    const applyConstraints = (track as MediaStreamTrack & {
      applyConstraints?: (constraints?: MediaTrackConstraints) => Promise<void>;
    }).applyConstraints;
    if (typeof applyConstraints !== "function") {
      return;
    }
    try {
      const volumeConstraints = {
        advanced: [{ volume: this.micLevel }],
      } as unknown as MediaTrackConstraints;
      await applyConstraints.call(track, {
        ...volumeConstraints,
      });
    } catch {
      // Ignore unsupported volume constraints on some browsers/devices.
    }
  }

  /**
   * Update state and notify callback
   */
  private setState(state: MicState, deviceLabel?: string): void {
    if (this.currentState === state) return;

    this.currentState = state;
    console.log(`[Microphone] State changed: ${state}${deviceLabel ? ` (${deviceLabel})` : ""}`);

    if (this.onStateChangeCallback) {
      this.onStateChangeCallback({ state, deviceLabel });
    }
  }
}
