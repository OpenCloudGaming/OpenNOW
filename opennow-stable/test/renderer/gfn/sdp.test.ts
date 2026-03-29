import { describe, expect, it } from "vitest";

import {
  extractIceCredentials,
  extractIceUfragFromOffer,
  extractPublicIp,
  fixServerIp,
  rewriteH265LevelIdByProfile,
  rewriteH265TierFlag,
} from "../../../src/renderer/src/gfn/sdp";
import { buildOfferSdp } from "../../fixtures/sdp";

describe("sdp helpers", () => {
  it("extracts public ip from dotted ips and dash hostnames", () => {
    expect(extractPublicIp("203.0.113.7")).toBe("203.0.113.7");
    expect(extractPublicIp("80-250-97-40.cloudmatchbeta.nvidiagrid.net")).toBe("80.250.97.40");
    expect(extractPublicIp("161-248-11-132.bpc.geforcenow.nvidiagrid.net")).toBe("161.248.11.132");
    expect(extractPublicIp("not-an-ip.example.com")).toBeNull();
  });

  it("replaces all connection lines and candidate ips when a valid public ip is available", () => {
    const source = buildOfferSdp("\r\n");
    const fixed = fixServerIp(source, "80-250-97-40.cloudmatchbeta.nvidiagrid.net");

    expect(fixed).toContain("c=IN IP4 80.250.97.40\r\n");
    expect(fixed).not.toContain("c=IN IP4 0.0.0.0");
    expect(fixed).toContain("a=candidate:1 1 udp 2113937151 80.250.97.40 54400 typ host");
    expect(fixed).toContain("a=candidate:2 1 udp 2113937151 80.250.97.40 54402 typ host");
    expect(fixed).toContain("\r\nm=video");
  });

  it("returns the original sdp when no valid public ip can be extracted", () => {
    const source = buildOfferSdp("\n");
    expect(fixServerIp(source, "invalid-host")).toBe(source);
  });

  it("extracts ice credentials and fingerprint from offers", () => {
    const source = buildOfferSdp("\r\n");

    expect(extractIceUfragFromOffer(source)).toBe("offerUfrag");
    expect(extractIceCredentials(source)).toEqual({
      ufrag: "offerUfrag",
      pwd: "offerPwd",
      fingerprint: "AA:BB:CC:DD",
    });
  });

  it("rewrites only h265 payload fmtp lines and preserves line endings", () => {
    const source = buildOfferSdp("\r\n");
    const { sdp, replacements } = rewriteH265TierFlag(source, 0);

    expect(replacements).toBe(2);
    expect(sdp).toContain("a=fmtp:98 profile-id=1;level-id=120;tier-flag=0\r\n");
    expect(sdp).toContain("a=fmtp:100 profile-id=2;level-id=150;tier-flag=0\r\n");
    expect(sdp).toContain("a=fmtp:96 profile-level-id=42e01f;tier-flag=1\r\n");
    expect(sdp).toContain("a=fmtp:111 minptime=10;useinbandfec=1;tier-flag=1;profile-id=1;level-id=93\r\n");
  });

  it("rewrites h265 level-id only when payload profile and level conditions match", () => {
    const source = buildOfferSdp("\n");
    const { sdp, replacements } = rewriteH265LevelIdByProfile(source, { 1: 93, 2: 120 });

    expect(replacements).toBe(2);
    expect(sdp).toContain("a=fmtp:98 profile-id=1;level-id=93;tier-flag=1\n");
    expect(sdp).toContain("a=fmtp:100 profile-id=2;level-id=120;tier-flag=1\n");
    expect(sdp).toContain("a=fmtp:96 profile-level-id=42e01f;tier-flag=1\n");
  });

  it("does not rewrite when payloads do not match conditions", () => {
    const source = [
      "v=0",
      "m=video 9 UDP/TLS/RTP/SAVPF 98",
      "a=rtpmap:98 H265/90000",
      "a=fmtp:98 profile-id=1;level-id=93;tier-flag=1",
      "",
    ].join("\n");

    expect(rewriteH265LevelIdByProfile(source, { 1: 93 })).toEqual({ sdp: source, replacements: 0 });
    expect(rewriteH265TierFlag(source, 1)).toEqual({ sdp: source, replacements: 0 });
  });
});
