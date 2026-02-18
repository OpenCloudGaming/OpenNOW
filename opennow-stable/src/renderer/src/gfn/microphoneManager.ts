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
  private onStateChangeCallback: ((state: MicStateChange) => void) | null = null;
  private sampleRate: number = 48000; // Official client uses 48kHz

  // Track if we should auto-retry with different devices on failure
  private attemptedDevices: Set<string> = new Set();

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
    const track = this.micStream?.getAudioTracks()[0];
    if (!track) {
      await this.ensurePlaceholderSender();
      return;
    }
    await this.addTrackToPeerConnection(track);
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
      // Request permission first to get labels
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
      this.micStream = await navigator.mediaDevices.getUserMedia(constraints);
      const track = this.micStream.getAudioTracks()[0];

      if (!track) {
        throw new Error("No audio track available");
      }

      // Set up track ended handler
      track.onended = () => {
        console.log("[Microphone] Track ended");
        this.stop();
      };

      // Handle stream inactive
      this.micStream.addEventListener("inactive", () => {
        console.log("[Microphone] Stream inactive");
        this.attemptedDevices.clear();
        this.micStream = null;
      });

      // Add track to peer connection if available
      if (this.pc) {
        await this.addTrackToPeerConnection(track);
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
            this.micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
            const track = this.micStream.getAudioTracks()[0];
            if (this.pc && track) {
              await this.addTrackToPeerConnection(track);
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

    if (this.micStream) {
      this.micStream.getTracks().forEach(track => {
        track.onended = null;
        track.stop();
      });
      this.micStream = null;
    }

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
   * Get active microphone track if available
   */
  getTrack(): MediaStreamTrack | null {
    return this.micStream?.getAudioTracks()[0] ?? null;
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
