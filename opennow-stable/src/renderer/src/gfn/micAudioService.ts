import type { MicMode, MicDeviceInfo, MicStatus } from "@shared/gfn";

const OPUS_SAMPLE_RATE = 48000;
const CAPTURE_CHANNEL_COUNT = 1;
const ANALYSER_FFT_SIZE = 256;
const ANALYSER_SMOOTHING = 0.3;
const LEVEL_POLL_MS = 50;

export interface MicAudioState {
  status: MicStatus;
  level: number;
  deviceId: string;
  deviceLabel: string;
}

export type MicStateListener = (state: MicAudioState) => void;

export class MicAudioService {
  private mode: MicMode = "off";
  private deviceId = "";
  private gain = 1.0;
  private noiseSuppression = true;
  private autoGainControl = true;
  private echoCancellation = true;

  private stream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private gainNode: GainNode | null = null;
  private analyserNode: AnalyserNode | null = null;
  private destinationNode: MediaStreamAudioDestinationNode | null = null;

  private outputTrack: MediaStreamTrack | null = null;
  private levelTimerId: ReturnType<typeof setInterval> | null = null;
  private analyserBuffer: Float32Array<ArrayBuffer> | null = null;

  private pttActive = false;
  private muted = false;

  private status: MicStatus = "off";
  private currentLevel = 0;
  private currentDeviceLabel = "";

  private listeners = new Set<MicStateListener>();
  private deviceChangeHandler: (() => void) | null = null;

  private rtcSender: RTCRtpSender | null = null;
  private peerConnection: RTCPeerConnection | null = null;

  getState(): MicAudioState {
    return {
      status: this.status,
      level: this.currentLevel,
      deviceId: this.deviceId,
      deviceLabel: this.currentDeviceLabel,
    };
  }

  onStateChange(listener: MicStateListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(): void {
    const state = this.getState();
    for (const listener of this.listeners) {
      try {
        listener(state);
      } catch { /* ignore */ }
    }
  }

  private setStatus(status: MicStatus): void {
    if (this.status !== status) {
      this.status = status;
      this.emit();
    }
  }

  async configure(opts: {
    mode: MicMode;
    deviceId: string;
    gain: number;
    noiseSuppression: boolean;
    autoGainControl: boolean;
    echoCancellation: boolean;
  }): Promise<void> {
    const modeChanged = this.mode !== opts.mode;
    const deviceChanged = this.deviceId !== opts.deviceId;
    const processingChanged =
      this.noiseSuppression !== opts.noiseSuppression ||
      this.autoGainControl !== opts.autoGainControl ||
      this.echoCancellation !== opts.echoCancellation;

    this.mode = opts.mode;
    this.deviceId = opts.deviceId;
    this.gain = opts.gain;
    this.noiseSuppression = opts.noiseSuppression;
    this.autoGainControl = opts.autoGainControl;
    this.echoCancellation = opts.echoCancellation;

    if (this.gainNode) {
      this.gainNode.gain.value = opts.gain;
    }

    if (this.mode === "off") {
      this.stopCapture();
      this.setStatus("off");
      return;
    }

    if (modeChanged || deviceChanged || processingChanged) {
      await this.startCapture();
    }

    this.updateMuteState();
  }

  setGain(value: number): void {
    this.gain = value;
    if (this.gainNode) {
      this.gainNode.gain.value = value;
    }
  }

  setPttActive(active: boolean): void {
    this.pttActive = active;
    this.updateMuteState();
  }

  toggleMute(): void {
    if (this.mode === "push-to-talk") {
      return;
    }
    this.muted = !this.muted;
    this.updateMuteState();
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    this.updateMuteState();
  }

  isMuted(): boolean {
    if (this.mode === "push-to-talk") {
      return !this.pttActive;
    }
    return this.muted;
  }

  getMode(): MicMode {
    return this.mode;
  }

  isActive(): boolean {
    return this.mode !== "off" && this.status === "active";
  }

  private updateMuteState(): void {
    if (!this.outputTrack) return;

    let shouldMute: boolean;
    if (this.mode === "push-to-talk") {
      shouldMute = !this.pttActive;
    } else if (this.mode === "on") {
      shouldMute = this.muted;
    } else {
      shouldMute = true;
    }

    this.outputTrack.enabled = !shouldMute;

    if (this.mode !== "off" && this.stream) {
      this.setStatus(shouldMute ? "muted" : "active");
    }
  }

  async startCapture(): Promise<void> {
    this.stopCapture();

    try {
      const constraints: MediaStreamConstraints = {
        audio: {
          deviceId: this.deviceId ? { exact: this.deviceId } : undefined,
          sampleRate: { ideal: OPUS_SAMPLE_RATE },
          channelCount: { exact: CAPTURE_CHANNEL_COUNT },
          noiseSuppression: { ideal: this.noiseSuppression },
          autoGainControl: { ideal: this.autoGainControl },
          echoCancellation: { ideal: this.echoCancellation },
        },
        video: false,
      };

      this.stream = await navigator.mediaDevices.getUserMedia(constraints);

      const audioTrack = this.stream.getAudioTracks()[0];
      if (!audioTrack) {
        this.setStatus("no-device");
        return;
      }

      this.currentDeviceLabel = audioTrack.label || "Microphone";

      this.audioContext = new AudioContext({
        sampleRate: OPUS_SAMPLE_RATE,
        latencyHint: "interactive",
      });

      this.sourceNode = this.audioContext.createMediaStreamSource(this.stream);

      this.gainNode = this.audioContext.createGain();
      this.gainNode.gain.value = this.gain;

      this.analyserNode = this.audioContext.createAnalyser();
      this.analyserNode.fftSize = ANALYSER_FFT_SIZE;
      this.analyserNode.smoothingTimeConstant = ANALYSER_SMOOTHING;
      this.analyserBuffer = new Float32Array(this.analyserNode.fftSize) as Float32Array<ArrayBuffer>;

      this.destinationNode = this.audioContext.createMediaStreamDestination();

      this.sourceNode.connect(this.gainNode);
      this.gainNode.connect(this.analyserNode);
      this.analyserNode.connect(this.destinationNode);

      this.outputTrack = this.destinationNode.stream.getAudioTracks()[0] ?? null;

      if (this.outputTrack) {
        this.updateMuteState();
        this.attachToPeerConnection();
      }

      this.startLevelMonitor();
      this.setStatus(this.isMuted() ? "muted" : "active");

      this.setupDeviceChangeListener();

      audioTrack.onended = () => {
        console.log("[Mic] Audio track ended (device disconnected)");
        this.handleDeviceDisconnect();
      };
    } catch (err: unknown) {
      console.error("[Mic] Failed to start capture:", err);
      if (err instanceof DOMException) {
        if (err.name === "NotAllowedError" || err.name === "PermissionDeniedError") {
          this.setStatus("permission-denied");
          return;
        }
        if (err.name === "NotFoundError" || err.name === "OverconstrainedError") {
          this.setStatus("no-device");
          return;
        }
      }
      this.setStatus("error");
    }
  }

  stopCapture(): void {
    this.stopLevelMonitor();

    if (this.rtcSender && this.peerConnection) {
      try {
        this.peerConnection.removeTrack(this.rtcSender);
      } catch { /* ignore */ }
      this.rtcSender = null;
    }

    if (this.sourceNode) {
      try { this.sourceNode.disconnect(); } catch { /* ignore */ }
      this.sourceNode = null;
    }
    if (this.gainNode) {
      try { this.gainNode.disconnect(); } catch { /* ignore */ }
      this.gainNode = null;
    }
    if (this.analyserNode) {
      try { this.analyserNode.disconnect(); } catch { /* ignore */ }
      this.analyserNode = null;
    }
    if (this.destinationNode) {
      try { this.destinationNode.disconnect(); } catch { /* ignore */ }
      this.destinationNode = null;
    }

    if (this.audioContext) {
      void this.audioContext.close().catch(() => { /* ignore */ });
      this.audioContext = null;
    }

    if (this.stream) {
      for (const track of this.stream.getTracks()) {
        track.stop();
      }
      this.stream = null;
    }

    this.outputTrack = null;
    this.analyserBuffer = null;
    this.currentLevel = 0;
    this.currentDeviceLabel = "";
  }

  setPeerConnection(pc: RTCPeerConnection | null): void {
    if (this.rtcSender && this.peerConnection) {
      try {
        this.peerConnection.removeTrack(this.rtcSender);
      } catch { /* ignore */ }
      this.rtcSender = null;
    }

    this.peerConnection = pc;

    if (pc && this.outputTrack) {
      this.attachToPeerConnection();
    }
  }

  private attachToPeerConnection(): void {
    if (!this.peerConnection || !this.outputTrack) return;

    if (this.rtcSender) {
      try {
        void this.rtcSender.replaceTrack(this.outputTrack);
        return;
      } catch { /* ignore */ }
    }

    try {
      this.rtcSender = this.peerConnection.addTrack(
        this.outputTrack,
        this.destinationNode!.stream,
      );

      if (this.rtcSender) {
        const params = this.rtcSender.getParameters();
        if (!params.encodings || params.encodings.length === 0) {
          params.encodings = [{}];
        }
        params.encodings[0].maxBitrate = 64_000;
        params.encodings[0].networkPriority = "high";
        params.encodings[0].priority = "high";
        void this.rtcSender.setParameters(params).catch(() => { /* ignore */ });
      }
    } catch (err) {
      console.error("[Mic] Failed to attach track to peer connection:", err);
    }
  }

  private startLevelMonitor(): void {
    this.stopLevelMonitor();
    this.levelTimerId = setInterval(() => {
      if (!this.analyserNode || !this.analyserBuffer) {
        this.currentLevel = 0;
        return;
      }

      this.analyserNode.getFloatTimeDomainData(this.analyserBuffer);
      let sum = 0;
      for (let i = 0; i < this.analyserBuffer.length; i++) {
        const v = this.analyserBuffer[i];
        sum += v * v;
      }
      const rms = Math.sqrt(sum / this.analyserBuffer.length);
      const db = 20 * Math.log10(Math.max(rms, 1e-10));
      const normalized = Math.max(0, Math.min(1, (db + 60) / 60));
      this.currentLevel = normalized;
      this.emit();
    }, LEVEL_POLL_MS);
  }

  private stopLevelMonitor(): void {
    if (this.levelTimerId !== null) {
      clearInterval(this.levelTimerId);
      this.levelTimerId = null;
    }
    this.currentLevel = 0;
  }

  private setupDeviceChangeListener(): void {
    this.removeDeviceChangeListener();
    this.deviceChangeHandler = () => {
      void this.handleDeviceChange();
    };
    navigator.mediaDevices.addEventListener("devicechange", this.deviceChangeHandler);
  }

  private removeDeviceChangeListener(): void {
    if (this.deviceChangeHandler) {
      navigator.mediaDevices.removeEventListener("devicechange", this.deviceChangeHandler);
      this.deviceChangeHandler = null;
    }
  }

  private async handleDeviceChange(): Promise<void> {
    if (this.mode === "off" || !this.stream) return;

    const devices = await MicAudioService.enumerateDevices();

    if (this.deviceId) {
      const stillExists = devices.some((d) => d.deviceId === this.deviceId);
      if (!stillExists) {
        console.log("[Mic] Selected device removed, falling back to default");
        this.deviceId = "";
        await this.startCapture();
        return;
      }
    }

    const currentTrack = this.stream.getAudioTracks()[0];
    if (!currentTrack || currentTrack.readyState === "ended") {
      console.log("[Mic] Track ended after device change, restarting");
      await this.startCapture();
    }
  }

  private async handleDeviceDisconnect(): Promise<void> {
    if (this.mode === "off") return;
    console.log("[Mic] Device disconnected, attempting recovery");
    this.deviceId = "";
    await this.startCapture();
  }

  static async enumerateDevices(): Promise<MicDeviceInfo[]> {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = devices.filter((d) => d.kind === "audioinput");

      const defaultDevice = audioInputs.find((d) => d.deviceId === "default");
      const defaultGroupId = defaultDevice?.groupId;

      return audioInputs
        .filter((d) => d.deviceId !== "")
        .map((d) => ({
          deviceId: d.deviceId,
          label: d.label || `Microphone (${d.deviceId.slice(0, 8)})`,
          isDefault: d.deviceId === "default" || (!!defaultGroupId && d.groupId === defaultGroupId && d.deviceId !== "default"),
        }));
    } catch (err) {
      console.error("[Mic] Failed to enumerate devices:", err);
      return [];
    }
  }

  dispose(): void {
    this.stopCapture();
    this.removeDeviceChangeListener();
    this.listeners.clear();
    this.peerConnection = null;
  }
}
