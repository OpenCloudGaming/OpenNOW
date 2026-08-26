import type { NvstSrtpProfile } from "@shared/gfn";

const SRTP_PROFILES: readonly NvstSrtpProfile[] = [
  "AEAD_AES_128_GCM",
  "AEAD_AES_256_GCM",
  "AES_CM_128_HMAC_SHA1_32",
  "AES_CM_128_HMAC_SHA1_80",
  "AES_CM_256_HMAC_SHA1_32",
  "AES_CM_256_HMAC_SHA1_80",
];
const SRTP_PROFILE_SET = new Set<string>(SRTP_PROFILES);

function findSrtpProfile(value: string): NvstSrtpProfile | null {
  for (const token of value.toUpperCase().match(/[A-Z][A-Z0-9_]*/g) ?? []) {
    if (SRTP_PROFILE_SET.has(token)) {
      return token as NvstSrtpProfile;
    }
  }
  return null;
}

export function extractAdvertisedSrtpProfileFromSdp(
  sdp: string,
): NvstSrtpProfile | null {
  for (const rawLine of sdp.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (/^a=crypto:\d+\s+/i.test(line)) {
      const profile = findSrtpProfile(line);
      if (profile) {
        return profile;
      }
      continue;
    }

    const attribute = /^a=([^:]+):(.*)$/i.exec(line);
    const attributeName = attribute?.[1] ?? "";
    if (
      !attribute
      || !/(?:srtp|crypto)/i.test(attributeName)
      || !/(?:profile|suite)/i.test(attributeName)
      || /(?:supported|capabilit)/i.test(attributeName)
    ) {
      continue;
    }
    const profile = findSrtpProfile(attribute[2] ?? "");
    if (profile) {
      return profile;
    }
  }
  return null;
}

export function extractAdvertisedSrtpProfileFromHeaders(
  headers: Record<string, string>,
): NvstSrtpProfile | null {
  for (const [name, value] of Object.entries(headers)) {
    if (
      name.toLowerCase() !== "transport"
      && (
        !/(?:srtp|crypto)/i.test(name)
        || !/(?:profile|suite)/i.test(name)
        || /(?:supported|capabilit)/i.test(name)
      )
    ) {
      continue;
    }
    const profile = findSrtpProfile(value);
    if (profile) {
      return profile;
    }
  }
  return null;
}

export function deriveSrtpSaltHex(keyId: number): string {
  return (keyId >>> 0).toString(16).toUpperCase().padStart(24, "0");
}
