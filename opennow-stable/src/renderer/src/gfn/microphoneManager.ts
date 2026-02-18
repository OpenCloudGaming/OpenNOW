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
      this.micStream.oninactive = () => {
        console.log("[Microphone] Stream inactive");
        this.attemptedDevices.clear();
        this.micStream = null;
      };

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

    // Check if we already have a sender for this track type
    const senders = this.pc.getSenders();
    const existingAudioSender = senders.find(s =>
      s.track?.kind === "audio" && s.track?.id !== track.id
    );

    if (existingAudioSender) {
      // Replace the track
      console.log("[Microphone] Replacing existing audio track");
      await existingAudioSender.replaceTrack(track);
      this.micSender = existingAudioSender;
    } else {
      // Add new track
      console.log("[Microphone] Adding new audio track to peer connection");
      this.micSender = this.pc.addTrack(track, new MediaStream([track]));
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
        // Don't remove the sender, just replace with null track
        this.micSender.replaceTrack(null).catch(() => {});
      } catch {
        // Ignore errors
      }
    }

    if (this.micStream) {
      this.micStream.getTracks().forEach(track => {
        track.onended = null;
        track.stop();
      });
      this.micStream.oninactive = null;
      this.micStream = null;
    }

    this.micSender = null;
    this.attemptedDevices.clear();
    this.setState("stopped");
  }

  /**
   * Dispose and cleanup
   */
  dispose(): void {
    this.stop();
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
