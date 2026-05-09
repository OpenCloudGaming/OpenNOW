import type { NativeStreamerInputPacket } from "@shared/nativeStreamer";

const MAX_NATIVE_INPUT_PACKET_BYTES = 4096;

export function normalizeNativeInputPacket(
  input: unknown,
): NativeStreamerInputPacket | null {
  if (!input || typeof input !== "object") {
    return null;
  }

  const packet = input as { payload?: unknown; partiallyReliable?: unknown };
  const rawPayload = packet.payload;
  let bytes: Uint8Array;

  if (rawPayload instanceof ArrayBuffer) {
    bytes = new Uint8Array(rawPayload);
  } else if (ArrayBuffer.isView(rawPayload)) {
    bytes = new Uint8Array(
      rawPayload.buffer,
      rawPayload.byteOffset,
      rawPayload.byteLength,
    );
  } else if (Array.isArray(rawPayload)) {
    if (
      rawPayload.length === 0 ||
      rawPayload.length > MAX_NATIVE_INPUT_PACKET_BYTES ||
      rawPayload.some(
        (byte) => !Number.isInteger(byte) || byte < 0 || byte > 255,
      )
    ) {
      return null;
    }
    bytes = Uint8Array.from(rawPayload);
  } else {
    return null;
  }

  if (
    bytes.byteLength === 0 ||
    bytes.byteLength > MAX_NATIVE_INPUT_PACKET_BYTES
  ) {
    return null;
  }

  return {
    payloadBase64: Buffer.from(
      bytes.buffer,
      bytes.byteOffset,
      bytes.byteLength,
    ).toString("base64"),
    partiallyReliable: packet.partiallyReliable === true,
  };
}
