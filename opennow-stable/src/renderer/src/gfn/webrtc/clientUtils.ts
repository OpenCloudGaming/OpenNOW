import type { ColorQuality, IceServer, VideoCodec } from "@shared/gfn";
import {
  PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
  PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
} from "../inputProtocol";

export interface OfferSettings {
  codec: VideoCodec;
  colorQuality: ColorQuality;
  resolution: string;
  fps: number;
  maxBitrateKbps: number;
}

export interface RiInputCapabilities {
  partialReliableThresholdMs: number | null;
  hidDeviceMask: number;
  enablePartiallyReliableTransferGamepad: number;
  enablePartiallyReliableTransferHid: number;
}

interface DualRumbleEffectOptions {
  startDelay: 0;
  duration: number;
  weakMagnitude: number;
  strongMagnitude: number;
}

interface GamepadHapticActuatorLike {
  readonly type?: string;
  playEffect(effectType: "dual-rumble", options: DualRumbleEffectOptions): Promise<unknown>;
}

interface LegacyGamepadHapticActuatorLike {
  pulse(value: number, duration: number): Promise<unknown>;
}

type GamepadWithOptionalHaptics = Gamepad & {
  readonly vibrationActuator?: GamepadHapticActuatorLike | null;
  readonly hapticActuators?: readonly (LegacyGamepadHapticActuatorLike | null | undefined)[] | null;
};

export interface GamepadRumbleApi {
  playEffectActuator: GamepadHapticActuatorLike | null;
  pulseActuator: LegacyGamepadHapticActuatorLike | null;
}

export interface ConnectedRumbleGamepad {
  index: number;
  gamepad: Gamepad;
  api: GamepadRumbleApi | null;
}

export function hevcPreferredProfileId(colorQuality: ColorQuality): 1 | 2 {
  return colorQuality.startsWith("10bit") ? 2 : 1;
}

export function timestampUs(sourceTimestampMs?: number): bigint {
  const base =
    typeof sourceTimestampMs === "number" && Number.isFinite(sourceTimestampMs) && sourceTimestampMs >= 0
      ? sourceTimestampMs
      : performance.now();
  return BigInt(Math.floor(base * 1000));
}

function parsePartialReliableThresholdMs(sdp: string): number | null {
  const match = sdp.match(/a=ri\.partialReliableThresholdMs:(\d+)/i);
  if (!match?.[1]) {
    return null;
  }
  const parsed = Number.parseInt(match[1], 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }
  return Math.max(1, Math.min(5000, parsed));
}

function parseRiIntegerAttribute(sdp: string, attribute: string, fallback: number): number {
  const escapedAttribute = attribute.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = sdp.match(new RegExp(`a=${escapedAttribute}:([^\\r\\n]+)`, "i"));
  const raw = match?.[1]?.trim();
  if (!raw) {
    return fallback;
  }
  const normalized = raw.toLowerCase();
  const parsed = normalized.startsWith("0x")
    ? Number.parseInt(normalized.slice(2), 16)
    : Number.parseInt(normalized, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function parseRiInputCapabilities(sdp: string): RiInputCapabilities {
  return {
    partialReliableThresholdMs: parsePartialReliableThresholdMs(sdp),
    hidDeviceMask: parseRiIntegerAttribute(sdp, "ri.hidDeviceMask", PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL),
    enablePartiallyReliableTransferGamepad: parseRiIntegerAttribute(
      sdp,
      "ri.enablePartiallyReliableTransferGamepad",
      PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
    ),
    enablePartiallyReliableTransferHid: parseRiIntegerAttribute(
      sdp,
      "ri.enablePartiallyReliableTransferHid",
      PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
    ),
  };
}

export function clampRumbleMagnitude(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(1, value));
}

export function isXboxLikeGamepad(gamepad: Gamepad): boolean {
  return /xbox|xinput/i.test(gamepad.id);
}

export function getGamepadRumbleApi(gamepad: Gamepad): GamepadRumbleApi | null {
  const hapticGamepad = gamepad as GamepadWithOptionalHaptics;
  const playEffectActuator = hapticGamepad.vibrationActuator;
  const pulseActuator = hapticGamepad.hapticActuators?.[0];
  const api: GamepadRumbleApi = {
    playEffectActuator:
      playEffectActuator && typeof playEffectActuator.playEffect === "function" ? playEffectActuator : null,
    pulseActuator: pulseActuator && typeof pulseActuator.pulse === "function" ? pulseActuator : null,
  };
  return api.playEffectActuator || api.pulseActuator ? api : null;
}

export function parseResolution(resolution: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = resolution.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }

  return { width, height };
}

export function toRtcIceServers(iceServers: IceServer[]): RTCIceServer[] {
  return iceServers.map((server) => ({
    urls: server.urls,
    username: server.username,
    credential: server.credential,
  }));
}

export async function toBytes(data: string | Blob | ArrayBuffer): Promise<Uint8Array> {
  if (typeof data === "string") {
    return new TextEncoder().encode(data);
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  const arrayBuffer = await data.arrayBuffer();
  return new Uint8Array(arrayBuffer);
}

export function detectGpuType(): string {
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
    if (!gl) {
      return "Unknown";
    }

    const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
    if (debugInfo) {
      const vendor = gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
      const renderer = gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);

      let gpuName = renderer;

      gpuName = gpuName
        .replace(/\(R\)/g, "")
        .replace(/\(TM\)/g, "")
        .replace(/NVIDIA /i, "")
        .replace(/AMD /i, "")
        .replace(/Intel /i, "")
        .replace(/Microsoft Corporation - /i, "")
        .replace(/D3D12 /i, "")
        .replace(/Direct3D11 /i, "")
        .replace(/OpenGL Engine/i, "")
        .trim();

      if (gpuName.length > 30) {
        gpuName = gpuName.substring(0, 27) + "...";
      }

      return gpuName || vendor || "Unknown";
    }
    return "Unknown";
  } catch {
    return "Unknown";
  }
}

export function normalizeCodecName(codecId: string): string {
  const upper = codecId.toUpperCase();

  if (upper.startsWith("H264") || upper === "H264") {
    return "H264";
  }
  if (upper.startsWith("H265") || upper === "H265" || upper.startsWith("HEVC")) {
    return "H265";
  }
  if (upper.startsWith("AV1")) {
    return "AV1";
  }
  if (upper.startsWith("VP9") || upper.startsWith("VP09")) {
    return "VP9";
  }
  if (upper.startsWith("VP8")) {
    return "VP8";
  }

  return codecId;
}
