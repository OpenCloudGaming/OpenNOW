export function parseInputProtocolVersion(bytes: Uint8Array): number | null {
  if (bytes.length < 2) {
    return null;
  }

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const firstWord = view.getUint16(0, true);
  if (firstWord === 0x020e) {
    const payloadLength = bytes.length >= 4 ? view.getUint16(2, true) : 0;
    if (payloadLength >= 2 && bytes.length >= 4 + payloadLength) {
      return view.getUint16(4, true);
    }
    return bytes.length >= 4 ? view.getUint16(2, true) : 2;
  }
  return bytes[0] === 0x0e ? firstWord : null;
}
