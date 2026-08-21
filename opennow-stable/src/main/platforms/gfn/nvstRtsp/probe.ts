/**
 * Classic NVST RTSPS-over-WSS handshake.
 *
 * Runs OPTIONS → DESCRIBE → SETUP → ANNOUNCE. Official cloud (`nativeRtcOnBundlePort=1`)
 * SETUPs video only with an empty Transport, binds Mjolnir before SETUP and the ICE
 * bundle after SETUP, then ANNOUNCEs `clientPorts.*=0` + `clientBundlePort`. First
 * bundle STUN is after ANNOUNCE; Mjolnir NATT starts just before PLAY. PLAY waits
 * for WebRtcTransport. The legacy path still SETUPs every advertised stream.
 */

import { createSocket } from "node:dgram";
import { networkInterfaces } from "node:os";
import { createHmac, randomBytes } from "node:crypto";

import type { NvstSrtpProfile, NvstVideoSession } from "@shared/gfn";

import {
  extractVideoPeer,
  header,
  RtspOverWssClient,
  type ParsedRtspResponse,
} from "./rtspClient";
import {
  buildAnnounceSdp,
  extractHmacSeed,
  extractNvstIceCredentials,
  extractNvstSdpAttribute,
  extractMediaControl,
  extractRuntimeEncryptionKey,
  generateClientEncryptionKey,
  generateNvstIceCredentials,
  packSrtpMasterKeySalt,
  redactKey,
} from "./sdp";
import {
  deriveSrtpSaltHex,
  extractAdvertisedSrtpProfileFromHeaders,
  extractAdvertisedSrtpProfileFromSdp,
} from "./srtp";

const DEFAULT_PROBE_TIMEOUT_MS = 20_000;

export interface NvstRtspProbeInput {
  sessionId: string;
  rtspsEndpoints: string[];
  resolution?: string;
  fps?: number;
  codec?: string;
  timeoutMs?: number;
  onLog?: (message: string) => void;
  /** Official Bifrost has MjolnirVideoReceiver reading before later SETUP/ANNOUNCE/PLAY. */
  onVideoReady?(videoSession: NvstVideoSession): Promise<void>;
  /** Official starts WebRtcTransport after ANNOUNCE and waits for DTLS before PLAY. */
  onAnnounceReady?(videoSession: NvstVideoSession): Promise<void>;
}

export interface NvstRtspClient {
  connect(sessionId?: string): Promise<void>;
  request(
    method: string,
    uri: string,
    extraHeaders?: Record<string, string>,
    body?: string,
  ): Promise<ParsedRtspResponse>;
  close(): void;
}

export interface NvstUdpPortReservation {
  port: number;
  /**
   * Port of the dedicated NATT-only Mjolnir video socket reserved alongside the
   * bundle. Set only when the native streamer owns both sockets (nvst-bind); the
   * probe must not bind or NATT a separate Mjolnir socket in that case.
   */
  mjolnirPort?: number;
  localAddress?: string;
  fd?: number;
  iceUsernameFragment?: string;
  icePassword?: string;
  /** SHA-256 colon hex of the local DTLS cert that owns this socket. */
  dtlsFingerprint?: string;
  send?(payload: Buffer, peerHost: string, peerPort: number): Promise<void>;
  onMessage?(
    handler: (payload: Buffer, peer: { address: string; port: number }) => void,
  ): void;
  release(): Promise<void>;
}

export interface NvstRtspNegotiationDependencies {
  createClient(
    host: string,
    port: number,
    timeoutMs: number,
    onLog?: (message: string) => void,
  ): NvstRtspClient;
  /** Mjolnir / extra SETUP sockets. Official cloud uses this for video NATT. */
  reserveUdpPort(peerHost?: string, peerPort?: number): Promise<NvstUdpPortReservation>;
  /** ICE/DTLS bundle socket. Official binds this after video SETUP. */
  reserveBundlePort?(peerHost?: string, peerPort?: number): Promise<NvstUdpPortReservation>;
}

export interface NvstSrtpMaterial {
  /** 64-char hex AES-256 key (runtime.encryptionKey). */
  aesKeyHex: string;
  /** Unsigned key id used for salt packing (runtime.encryptionKeyId as u32). */
  keyId: number;
  /** 88-char hex libsrtp master key||salt (AES-256 || 12-byte salt). */
  masterKeySaltHex: string;
  /** 24-char hex salt derived from runtime.encryptionKeyId. */
  saltHex: string;
  /** Present only when DESCRIBE or SETUP explicitly advertises a known profile. */
  profile?: NvstSrtpProfile;
  /** True when OpenNOW generated the key for ANNOUNCE (DESCRIBE lacked it). */
  clientGenerated: boolean;
}

export interface NvstRtspProbeResult {
  ok: boolean;
  endpoint: string;
  session?: string;
  hmacSeedPresent: boolean;
  videoPeer?: { ip: string; port: number };
  clientUdpPort?: number;
  srtp?: NvstSrtpMaterial;
  pingPayload?: string;
  pingVersion?: number;
  /** Handoff for native UDP video (shared NvstVideoSession shape). */
  videoSession?: NvstVideoSession;
  steps: string[];
  error?: string;
}

export type NvstRtspNegotiationErrorCode =
  | "missing-rtsps-endpoint"
  | "missing-video-control"
  | "missing-audio-control"
  | "missing-control-stream"
  | "missing-video-peer"
  | "missing-ice-credentials"
  | "conflicting-srtp-profile"
  | "negotiation-failed";

export class NvstRtspNegotiationError extends Error {
  constructor(
    readonly code: NvstRtspNegotiationErrorCode,
    message: string,
    options?: ErrorOptions,
    readonly steps: string[] = [],
  ) {
    super(message, options);
    this.name = "NvstRtspNegotiationError";
  }
}

export interface NvstRtspSession {
  endpoint: string;
  session: string;
  hmacSeedPresent: boolean;
  videoPeer: { ip: string; port: number };
  clientUdpPort: number;
  srtp: NvstSrtpMaterial;
  pingPayload?: string;
  pingVersion?: number;
  videoSession: NvstVideoSession;
  steps: string[];
  videoUdpFd?: number;
  /** Releases the video UDP reservation after native has rebound clientUdpPort. */
  handoffVideoUdp(): Promise<void>;
  release(reason?: string): Promise<void>;
}

function log(onLog: NvstRtspProbeInput["onLog"], message: string): void {
  console.log(`[NvstRtspProbe] ${message}`);
  onLog?.(message);
}

const CRC32_TABLE = Array.from({ length: 256 }, (_, value) => {
  let crc = value;
  for (let bit = 0; bit < 8; bit += 1) {
    crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return crc >>> 0;
});

function appendStunAttribute(packet: Buffer[], type: number, value: Buffer): void {
  const header = Buffer.alloc(4);
  header.writeUInt16BE(type, 0);
  header.writeUInt16BE(value.length, 2);
  packet.push(header, value);
  const padding = value.length % 4;
  if (padding) {
    packet.push(Buffer.alloc(4 - padding));
  }
}

export function buildNvstStunBindingRequest(
  localUsernameFragment: string,
  remoteUsernameFragment: string,
  remotePassword: string,
  transactionId: Buffer = randomBytes(12),
): Buffer {
  if (transactionId.length !== 12) {
    throw new Error("STUN transaction ID must be 12 bytes");
  }
  const header = Buffer.alloc(20);
  header.writeUInt16BE(0x0001, 0);
  header.writeUInt32BE(0x2112a442, 4);
  transactionId.copy(header, 8);
  const parts = [header];
  appendStunAttribute(
    parts,
    0x0006,
    Buffer.from(`${remoteUsernameFragment}:${localUsernameFragment}`, "utf8"),
  );
  let packet = Buffer.concat(parts);
  header.writeUInt16BE(packet.length - 20 + 24, 2);
  packet = Buffer.concat(parts);
  appendStunAttribute(parts, 0x0008, createHmac("sha1", remotePassword).update(packet).digest());
  packet = Buffer.concat(parts);
  header.writeUInt16BE(packet.length - 20 + 8, 2);
  packet = Buffer.concat(parts);
  let crc = 0xffffffff;
  for (const byte of packet) {
    crc = (crc >>> 8) ^ CRC32_TABLE[(crc ^ byte) & 0xff]!;
  }
  const fingerprint = Buffer.alloc(4);
  fingerprint.writeUInt32BE(((crc ^ 0xffffffff) ^ 0x5354554e) >>> 0);
  appendStunAttribute(parts, 0x8028, fingerprint);
  return Buffer.concat(parts);
}

export function selectPrimaryRtspsEndpoint(endpoints: string[]): string | null {
  const normalized = endpoints
    .map((value) => value.trim())
    .filter((value) => /^rtsps?:\/\//i.test(value));
  return normalized[0] ?? null;
}

/**
 * Logging identity only (`wss://host:port`, no path). Real `:322` connect uses
 * {@link connectNvstWss} with absolute-form upgrade cascade (rtsps/wss/https).
 * See docs/research/nvst-rtsps-wss-connect.md and `_tmp-bifrost2-ws-400-followup.txt`.
 */
export function rtspsUrlToWssUrl(rtspsUrl: string): string {
  const parsed = new URL(rtspsUrl.replace(/^rtsps:/i, "https:").replace(/^rtsp:/i, "http:"));
  const port = parsed.port || "322";
  return `wss://${parsed.hostname}:${port}`;
}

export function collectRtspsEndpoints(
  connections: Array<{
    usage?: number;
    port?: number;
    appLevelProtocol?: number;
    resourcePath?: string | null;
  }>,
  fallbackHost?: string | null,
): string[] {
  const endpoints: string[] = [];
  const seen = new Set<string>();

  for (const conn of connections) {
    const resourcePath = typeof conn.resourcePath === "string" ? conn.resourcePath.trim() : "";
    if (/^rtsps?:\/\//i.test(resourcePath)) {
      if (!seen.has(resourcePath)) {
        seen.add(resourcePath);
        endpoints.push(resourcePath);
      }
      continue;
    }
    if (conn.usage !== 16 && conn.appLevelProtocol !== 6) {
      continue;
    }
    if (!fallbackHost || !conn.port) {
      continue;
    }
    const synthesized = `rtsps://${fallbackHost}:${conn.port}`;
    if (!seen.has(synthesized)) {
      seen.add(synthesized);
      endpoints.push(synthesized);
    }
  }

  return endpoints;
}

export function resolveRtspControlUri(baseUri: string, control: string): string {
  if (/^rtsps?:\/\//i.test(control)) {
    return control;
  }

  const base = new URL(baseUri.replace(/^rtsps:/i, "https:").replace(/^rtsp:/i, "http:"));
  const scheme = baseUri.toLowerCase().startsWith("rtsp:") ? "rtsp:" : "rtsps:";
  if (control.startsWith("/")) {
    return `${scheme}//${base.host}${control}`;
  }
  return `${baseUri.replace(/\/+$/, "")}/${control.replace(/^\/+/, "")}`;
}

/** Official SETUP uses `streamid=video/0/0` when DESCRIBE advertised `streamid=video/0`. */
export function officialVideoSetupControl(control: string): string {
  if (/^streamid=video\/\d+$/i.test(control)) {
    return `${control}/0`;
  }
  return control;
}

/** Official bundle ICE remote ufrag is SETUP ping + 1 (`…998` → `…999`). */
export function incrementNvstPingUfrag(payload: string): string | null {
  if (!/^[0-9a-fA-F]+$/.test(payload) || payload.toUpperCase() === "PING") {
    return null;
  }
  const next = (BigInt(`0x${payload}`) + 1n).toString(16);
  return next.padStart(payload.length, "0");
}

/**
 * Official NattHolePunch STUN remote ufrag:
 * hex SETUP ping → ping+1 on the ICE bundle; otherwise the ping-string itself
 * (`PING` when "Old server only supports PING"). Never fall back to DESCRIBE V2
 * while a SETUP ping payload is present — that keepalive identity is what the
 * server answers.
 */
export function resolveNvstIceRemoteUfrag(
  pingPayload: string | undefined,
  describeUfrag?: string,
  pingVersion?: number,
): string | undefined {
  if (pingPayload) {
    const incremented = incrementNvstPingUfrag(pingPayload);
    if (incremented) {
      return incremented;
    }
    if (pingPayload.toUpperCase() === "PING" || pingVersion === 6) {
      return pingPayload;
    }
  }
  return describeUfrag;
}

function xorMappedIPv4(host: string, port: number): Buffer | null {
  const parts = host.split(".").map((part) => Number.parseInt(part, 10));
  if (parts.length !== 4 || parts.some((part) => !Number.isFinite(part) || part < 0 || part > 255)) {
    return null;
  }
  const value = Buffer.alloc(8);
  value.writeUInt8(0, 0);
  value.writeUInt8(1, 1);
  value.writeUInt16BE(port ^ 0x2112, 2);
  value[4] = (parts[0] ?? 0) ^ 0x21;
  value[5] = (parts[1] ?? 0) ^ 0x12;
  value[6] = (parts[2] ?? 0) ^ 0xa4;
  value[7] = (parts[3] ?? 0) ^ 0x42;
  return value;
}

/** Official ping-version 6 PONG: STUN Binding Success for an inbound request. */
export function buildNvstStunBindingSuccess(
  localPassword: string,
  transactionId: Buffer,
  mappedHost: string,
  mappedPort: number,
): Buffer | null {
  if (transactionId.length !== 12) {
    return null;
  }
  const mapped = xorMappedIPv4(mappedHost, mappedPort);
  if (!mapped) {
    return null;
  }
  const header = Buffer.alloc(20);
  header.writeUInt16BE(0x0101, 0);
  header.writeUInt32BE(0x2112a442, 4);
  transactionId.copy(header, 8);
  const parts = [header];
  appendStunAttribute(parts, 0x0020, mapped);
  let packet = Buffer.concat(parts);
  header.writeUInt16BE(packet.length - 20 + 24, 2);
  packet = Buffer.concat(parts);
  appendStunAttribute(parts, 0x0008, createHmac("sha1", localPassword).update(packet).digest());
  packet = Buffer.concat(parts);
  header.writeUInt16BE(packet.length - 20 + 8, 2);
  packet = Buffer.concat(parts);
  let crc = 0xffffffff;
  for (const byte of packet) {
    crc = (crc >>> 8) ^ CRC32_TABLE[(crc ^ byte) & 0xff]!;
  }
  const fingerprint = Buffer.alloc(4);
  fingerprint.writeUInt32BE(((crc ^ 0xffffffff) ^ 0x5354554e) >>> 0);
  appendStunAttribute(parts, 0x8028, fingerprint);
  return Buffer.concat(parts);
}

function pickLocalIpv4(): string | undefined {
  for (const addresses of Object.values(networkInterfaces())) {
    for (const address of addresses ?? []) {
      if (address.family === "IPv4" && !address.internal) {
        return address.address;
      }
    }
  }
  return undefined;
}

export function createNvstNegotiationDependencies(
  overrides: Partial<NvstRtspNegotiationDependencies> = {},
): NvstRtspNegotiationDependencies {
  return {
    ...DEFAULT_NEGOTIATION_DEPENDENCIES,
    ...overrides,
  };
}

export async function bindEphemeralUdp(peerHost?: string, peerPort?: number): Promise<NvstUdpPortReservation> {
  const socket = createSocket({ type: "udp4", reuseAddr: true });
  try {
    await new Promise<void>((resolve, reject) => {
      socket.once("error", reject);
      socket.bind(0, "0.0.0.0", () => {
        socket.off("error", reject);
        resolve();
      });
    });
  } catch (error) {
    try {
      socket.close();
    } catch {}
    throw error;
  }
  if (peerHost && peerPort) {
    await new Promise<void>((resolve, reject) => {
      socket.once("error", reject);
      socket.connect(peerPort, peerHost, () => {
        socket.off("error", reject);
        resolve();
      });
    });
  }
  const address = socket.address();
  if (typeof address === "string") {
    socket.close();
    throw new Error("Unexpected UDP socket address shape");
  }
  let released = false;
  if (peerHost && peerPort) {
    socket.disconnect();
  }
  const handle = (socket as unknown as { _handle?: { fd?: number } })._handle;
  const fd = typeof handle?.fd === "number" && handle.fd >= 0 ? handle.fd : undefined;
  return {
    port: address.port,
    localAddress: address.address === "0.0.0.0" ? pickLocalIpv4() : address.address,
    fd,
    send: async (payload, host, port) => {
      await new Promise<void>((resolve, reject) => {
        socket.send(payload, port, host, (error) => error ? reject(error) : resolve());
      });
    },
    onMessage: (handler) => {
      socket.on("message", (message, rinfo) => {
        handler(Buffer.isBuffer(message) ? message : Buffer.from(message), {
          address: rinfo.address,
          port: rinfo.port,
        });
      });
    },
    release: async () => {
      if (released) {
        return;
      }
      released = true;
      await new Promise<void>((resolve) => socket.close(resolve));
    },
  };
}

const DEFAULT_NEGOTIATION_DEPENDENCIES: NvstRtspNegotiationDependencies = {
  createClient: (host, port, timeoutMs, onLog) =>
    new RtspOverWssClient(host, port, timeoutMs, onLog),
  reserveUdpPort: bindEphemeralUdp,
};

async function teardownAndClose(
  client: NvstRtspClient,
  endpoint: string,
  session: string | null,
  reason: string,
  onLog?: (message: string) => void,
): Promise<void> {
  try {
    if (session) {
      const response = await client.request("TEARDOWN", endpoint, { Session: session });
      if (response.statusCode === 200) {
        log(onLog, `TEARDOWN ok (${reason})`);
      } else {
        log(onLog, `TEARDOWN returned ${response.statusCode} ${response.statusText} (${reason})`);
      }
    }
  } catch (error) {
    log(onLog, `TEARDOWN failed (${reason}): ${error instanceof Error ? error.message : String(error)}`);
  } finally {
    client.close();
  }
}

export async function negotiateNvstRtspSession(
  input: NvstRtspProbeInput,
  dependencies: NvstRtspNegotiationDependencies = DEFAULT_NEGOTIATION_DEPENDENCIES,
): Promise<NvstRtspSession> {
  const steps: string[] = [];
  const endpoint = selectPrimaryRtspsEndpoint(input.rtspsEndpoints);
  if (!endpoint) {
    throw new NvstRtspNegotiationError(
      "missing-rtsps-endpoint",
      "No rtsps:// endpoints available on the session",
    );
  }

  const timeoutMs = input.timeoutMs ?? DEFAULT_PROBE_TIMEOUT_MS;
  const wssUrl = rtspsUrlToWssUrl(endpoint);
  const parsedEndpoint = new URL(endpoint.replace(/^rtsps:/i, "https:").replace(/^rtsp:/i, "http:"));
  const client = dependencies.createClient(
    parsedEndpoint.hostname,
    Number(parsedEndpoint.port || "322"),
    timeoutMs,
    (message) => log(input.onLog, message),
  );
  let udp: NvstUdpPortReservation | null = null;
  let mjolnirUdp: NvstUdpPortReservation | null = null;
  // Port of the native-owned Mjolnir video socket (undefined when the probe owns
  // the fallback Mjolnir socket itself).
  let nativeMjolnirPort: number | undefined;
  let audioUdp: NvstUdpPortReservation | null = null;
  const auxiliaryUdp: NvstUdpPortReservation[] = [];
  const holePunchTimers: NodeJS.Timeout[] = [];
  let videoHolePunchTimer: NodeJS.Timeout | null = null;
  let session: string | null = null;

  const reserveBundle = (): Promise<NvstUdpPortReservation> =>
    (dependencies.reserveBundlePort ?? dependencies.reserveUdpPort)();

  try {
    log(
      input.onLog,
      `Connecting RTSPS WSS ${wssUrl} via raw-TLS Bifrost-shaped upgrade (GET /rtsp) (session ${input.sessionId})`,
    );
    await client.connect(input.sessionId);
    steps.push("wss-open");

    const rtspTarget = `rtsps://${parsedEndpoint.host}`;
    const commonHeaders: Record<string, string> = {
      "X-GS-Version": "14.2",
      Host: parsedEndpoint.host,
    };
    if (input.sessionId.trim()) {
      commonHeaders["x-nv-sessionid"] = input.sessionId.trim();
    }
    const options = await client.request("OPTIONS", rtspTarget, commonHeaders);
    if (options.statusCode !== 200) {
      throw new Error(`OPTIONS failed: ${options.statusCode} ${options.statusText}`);
    }
    steps.push("options");
    log(input.onLog, `OPTIONS ok (X-GS-Version=${header(options.headers, "x-gs-version") ?? "n/a"})`);

    const describe = await client.request("DESCRIBE", rtspTarget, {
      ...commonHeaders,
      Accept: "application/sdp",
      // Official Bifrost sends x-nv-abtesting on DESCRIBE; the seat keys the modern
      // ping/HMAC media context off it. Omitting it is treated as a legacy client.
      "x-nv-abtesting": "2",
    });
    if (describe.statusCode !== 200) {
      throw new Error(`DESCRIBE failed: ${describe.statusCode} ${describe.statusText}`);
    }
    steps.push("describe");
    session = header(describe.headers, "session")?.split(";")[0]?.trim() ?? null;
    if (!session) {
      throw new Error("DESCRIBE response missing Session header");
    }
    const videoControl = extractMediaControl(describe.body, "video");
    if (!videoControl) {
      throw new NvstRtspNegotiationError(
        "missing-video-control",
        "DESCRIBE response did not advertise a video media control URI",
      );
    }
    const audioControl = extractMediaControl(describe.body, "audio");
    if (!audioControl) {
      throw new NvstRtspNegotiationError(
        "missing-audio-control",
        "DESCRIBE response did not advertise an audio media control URI",
      );
    }
    const describedControls = [...describe.body.matchAll(/^a=control:(.+)$/gm)]
      .map((match) => match[1]?.trim())
      .filter((control): control is string => Boolean(control));
    const controlStream = describedControls.find((control) => /(?:^|[=/])control\/0(?:\/|$)/i.test(control));
    if (!controlStream) {
      throw new NvstRtspNegotiationError(
        "missing-control-stream",
        "DESCRIBE response did not advertise the primary control/0 stream",
      );
    }
    log(input.onLog, `DESCRIBE media controls: ${describedControls.join(", ") || "none"}`);
    const videoControlUri = videoControl;
    const hmacSeed = extractHmacSeed(describe.body);
    const iceCredentials = extractNvstIceCredentials(describe.body);
    const legacyIceUsername = extractNvstSdpAttribute(describe.body, "general.iceUsernameFragment");
    const legacyIcePassword = extractNvstSdpAttribute(describe.body, "general.iceUsernamePwd");
    const v2IceUsername = extractNvstSdpAttribute(describe.body, "general.iceUserNameFragmentV2");
    const v2IcePassword = extractNvstSdpAttribute(describe.body, "general.icePasswordV2");
    const describedSrtpProfile = extractAdvertisedSrtpProfileFromSdp(describe.body);
    const describedKey = extractRuntimeEncryptionKey(describe.body);
    const dtlsFingerprint = extractNvstSdpAttribute(describe.body, "general.dtlsFingerprintV2")
      ?? extractNvstSdpAttribute(describe.body, "general.dtlsFingerprint");
    const serverTransport = extractNvstSdpAttribute(describe.body, "general.serverTransport");
    const describedClientTransport = extractNvstSdpAttribute(describe.body, "general.clientTransport");
    const useNewIceInfo = extractNvstSdpAttribute(describe.body, "general.useNewIceInfo");
    const describedPingVersion = extractNvstSdpAttribute(describe.body, "general.pingVersion");
    const disablePlay = extractNvstSdpAttribute(describe.body, "general.disablePlay");
    const nativeRtcOnBundlePort = extractNvstSdpAttribute(describe.body, "general.nativeRtcOnBundlePort");
    log(
      input.onLog,
      `DESCRIBE transport metadata: dtlsFingerprintBytes=${dtlsFingerprint?.length ?? 0}, serverTransport=${serverTransport ?? "absent"}, clientTransport=${describedClientTransport ?? "absent"}, useNewIceInfo=${useNewIceInfo ?? "absent"}, pingVersion=${describedPingVersion ?? "absent"}, disablePlay=${disablePlay ?? "absent"}, nativeRtcOnBundlePort=${nativeRtcOnBundlePort ?? "absent"}, legacyIce=${legacyIceUsername?.length ?? 0}/${legacyIcePassword?.length ?? 0}, v2Ice=${v2IceUsername?.length ?? 0}/${v2IcePassword?.length ?? 0}, iceVariantsMatch=${legacyIceUsername === v2IceUsername && legacyIcePassword === v2IcePassword}`,
    );
    let encryptionKeyHex: string | undefined;
    let encryptionKeyId: number | undefined;
    let clientGenerated = false;
    if (describedKey) {
      encryptionKeyHex = describedKey.aesKeyHex;
      encryptionKeyId = describedKey.keyId;
      log(
        input.onLog,
        `DESCRIBE ok (Session=${session}, videoControl=${videoControl}, HMAC ${hmacSeed ? "present" : "missing"}, ICE credentials ${iceCredentials ? `present (ufragBytes=${iceCredentials.usernameFragment.length}, pwdBytes=${iceCredentials.password.length})` : "missing"}, encryptionKey ${redactKey(encryptionKeyHex)} from server)`,
      );
    } else {
      // Official Bifrost ALWAYS client-generates runtime.encryptionKey and sends it in
      // ANNOUNCE — even when a DTLS fingerprint is present. The video SRTP path (the
      // separate non-DTLS socket) is keyed by this runtime key, not by DTLS-SRTP. If we
      // skip generating it, the server never keys video for us and no video is sent.
      const generated = generateClientEncryptionKey();
      encryptionKeyHex = generated.aesKeyHex;
      encryptionKeyId = generated.keyId;
      clientGenerated = true;
      log(
        input.onLog,
        `DESCRIBE ok (Session=${session}, videoControl=${videoControl}, HMAC ${hmacSeed ? "present" : "missing"}, ICE credentials ${iceCredentials ? `present (ufragBytes=${iceCredentials.usernameFragment.length}, pwdBytes=${iceCredentials.password.length})` : "missing"}, encryptionKey absent — client-generated ${redactKey(encryptionKeyHex)} for ANNOUNCE)`,
      );
    }

    const officialCloudPath = nativeRtcOnBundlePort === "1";
    // Official Bifrost binds the ICE/bundle socket on 0.0.0.0 and never connects
    // it to the RTSPS host. Connecting to :322 would create the wrong NAT mapping.
    // Official: Mjolnir first (empty Transport line), then ICE bundle after SETUP.
    const videoSetupUri = officialCloudPath
      ? officialVideoSetupControl(videoControlUri)
      : videoControlUri;
    if (officialCloudPath) {
      // Official two-socket model: the native streamer reserves BOTH the ICE/DTLS
      // bundle and the dedicated NATT-only Mjolnir video socket in one nvst-bind.
      // Reserve the bundle now so its mjolnirPort is known before video SETUP; the
      // probe must not bind its own Mjolnir socket when the native streamer owns it.
      udp = await reserveBundle();
      if (udp.mjolnirPort === undefined) {
        // Older native streamer without a Mjolnir reservation: keep the probe-owned
        // fallback socket so NATT keepalive still runs somewhere.
        mjolnirUdp = await dependencies.reserveUdpPort();
      } else {
        nativeMjolnirPort = udp.mjolnirPort;
      }
    } else {
      udp = await reserveBundle();
    }
    const setupHeaders: Record<string, string> = {
      ...commonHeaders,
      Session: session,
      // Official Bifrost advertises its ping-protocol version on SETUP. The seat only
      // returns a hex X-Nv-Ping-Payload (modern NATT/ICE identity) when this is present;
      // without it the seat falls back to the literal "PING" keepalive and never arms
      // the media relay to answer STUN. Echo the DESCRIBE-advertised version.
      "x-nv-ping": describedPingVersion ?? "6",
    };
    if (officialCloudPath) {
      setupHeaders.Transport = "";
    } else if (udp) {
      setupHeaders.Transport = `unicast;X-GS-ClientPort=${udp.port}-${udp.port + 1}`;
    }
    const setup = await client.request("SETUP", videoSetupUri, setupHeaders);
    if (setup.statusCode !== 200) {
      throw new Error(`SETUP video failed: ${setup.statusCode} ${setup.statusText}`);
    }
    steps.push("setup-video");
    session = header(setup.headers, "session")?.split(";")[0]?.trim() ?? session;
    const setupSrtpProfile = extractAdvertisedSrtpProfileFromHeaders(setup.headers);
    if (
      describedSrtpProfile
      && setupSrtpProfile
      && describedSrtpProfile !== setupSrtpProfile
    ) {
      throw new NvstRtspNegotiationError(
        "conflicting-srtp-profile",
        `DESCRIBE advertised ${describedSrtpProfile} but SETUP advertised ${setupSrtpProfile}`,
      );
    }
    const srtpProfile = setupSrtpProfile ?? describedSrtpProfile ?? undefined;
    const videoPeer = extractVideoPeer(header(setup.headers, "transport"));
    if (!videoPeer) {
      throw new NvstRtspNegotiationError(
        "missing-video-peer",
        "SETUP did not return video peer (X-GS-ServerPort/source)",
      );
    }
    const pingPayload = header(setup.headers, "x-nv-ping-payload");
    const pingVersionRaw = header(setup.headers, "x-nv-ping");
    const pingVersion = pingVersionRaw ? Number(pingVersionRaw) : undefined;
    if (pingVersion === 6 && (!iceCredentials || !pingPayload)) {
      throw new NvstRtspNegotiationError(
        "missing-ice-credentials",
        "SETUP selected ping version 6 but its username payload or DESCRIBE password was missing",
      );
    }
    if (!udp) {
      throw new NvstRtspNegotiationError(
        "negotiation-failed",
        "NVST ICE/bundle UDP socket was not reserved",
      );
    }
    const clientPort = udp.port;
    const iceRemoteUfrag = resolveNvstIceRemoteUfrag(
      pingPayload,
      iceCredentials?.usernameFragment,
      Number.isFinite(pingVersion) ? pingVersion : undefined,
    );
    const reservedIce = udp.iceUsernameFragment && udp.icePassword
      ? { usernameFragment: udp.iceUsernameFragment, password: udp.icePassword }
      : null;
    const localIceCredentials = iceCredentials
      ? reservedIce ?? generateNvstIceCredentials()
      : reservedIce;
    const localDtlsFingerprint = udp.dtlsFingerprint;
    const startAuthenticatedHolePunch = (
      reservation: NvstUdpPortReservation,
      peer: { ip: string; port: number },
      options?: { nattUsername?: string; iceBurst?: boolean },
    ): NodeJS.Timeout | null => {
      if (!localIceCredentials || !iceCredentials || !reservation.send) {
        return null;
      }
      const iceBurst = options?.iceBurst !== false;
      const nattUsername = options?.nattUsername;
      let nonStunInbound = 0;
      reservation.onMessage?.((payload, from) => {
        if (payload.equals(Buffer.from("PING"))) {
          void reservation.send?.(Buffer.from("PONG"), from.address, from.port).catch(() => undefined);
          return;
        }
        if (payload.length >= 20 && payload.readUInt16BE(0) === 0x0001) {
          const transactionId = payload.subarray(8, 20);
          const pong = buildNvstStunBindingSuccess(
            localIceCredentials.password,
            transactionId,
            from.address,
            from.port,
          );
          if (pong) {
            void reservation.send?.(pong, from.address, from.port).catch(() => undefined);
          }
          return;
        }
        // Non-PING, non-STUN inbound is candidate video/SRTP. Log it (throttled) so we can
        // confirm which socket the seat actually delivers video to (bundle vs Mjolnir).
        nonStunInbound += 1;
        if (nonStunInbound <= 5 || nonStunInbound % 200 === 0) {
          log(
            input.onLog,
            `hole-punch inbound (non-STUN) port=${reservation.port} count=${nonStunInbound} bytes=${payload.length} firstByte=0x${payload.readUInt8(0).toString(16).padStart(2, "0")} from=${from.address}:${from.port}`,
          );
        }
      });
      const sendPing = (): void => {
        // Official first burst is three ICE Binding Requests, then NATT
        // ping-string PING ("Old server only supports PING") as keepalive.
        if (iceBurst && iceRemoteUfrag) {
          for (let burst = 0; burst < 3; burst += 1) {
            const ice = buildNvstStunBindingRequest(
              localIceCredentials.usernameFragment,
              iceRemoteUfrag,
              iceCredentials.password,
            );
            void reservation.send?.(ice, peer.ip, peer.port).catch(() => undefined);
          }
        }
        if (nattUsername) {
          const natt = buildNvstStunBindingRequest(
            localIceCredentials.usernameFragment,
            nattUsername,
            iceCredentials.password,
          );
          void reservation.send?.(natt, peer.ip, peer.port).catch(() => undefined);
        }
      };
      sendPing();
      const timer = setInterval(sendPing, 20);
      timer.unref();
      holePunchTimers.push(timer);
      return timer;
    };
    if (iceCredentials && localIceCredentials && !officialCloudPath) {
      videoHolePunchTimer = startAuthenticatedHolePunch(
        udp,
        videoPeer,
        { nattUsername: "PING" },
      );
    }
    const setupHeaderNames = Object.keys(setup.headers).sort();
    const setupCredentialHeaders = setupHeaderNames
      .filter((name) => /(ice|stun|credential|password|user)/i.test(name))
      .map((name) => `${name}[${header(setup.headers, name)?.length ?? 0}]`);
    log(
      input.onLog,
      `SETUP ${videoSetupUri} ok (official=${officialCloudPath}, bundlePort=${clientPort}${nativeMjolnirPort !== undefined ? `, mjolnirPort=${nativeMjolnirPort} (native)` : mjolnirUdp ? `, mjolnirPort=${mjolnirUdp.port}` : ""}, transport=${officialCloudPath ? "empty" : setupHeaders.Transport}, peer=${videoPeer.ip}:${videoPeer.port}, srtpProfile=${srtpProfile ?? "legacy-default"}, pingVersion=${Number.isFinite(pingVersion) ? pingVersion : "legacy"}, pingPayload=${pingPayload === undefined ? "absent" : JSON.stringify(pingPayload)}, iceRemote=${iceRemoteUfrag ?? "absent"}, pingPayloadBytes=${pingPayload ? Buffer.byteLength(pingPayload, "utf8") : 0}, headers=${setupHeaderNames.join(",")}, credentialHeaders=${setupCredentialHeaders.join(",") || "none"})`,
    );

    const handoffKeyHex = encryptionKeyHex ?? "00".repeat(32);
    const handoffKeyId = encryptionKeyId ?? 0;
    const saltHex = deriveSrtpSaltHex(handoffKeyId);
    const srtp: NvstSrtpMaterial = {
      aesKeyHex: handoffKeyHex,
      keyId: handoffKeyId,
      masterKeySaltHex: packSrtpMasterKeySalt(handoffKeyHex, handoffKeyId),
      saltHex,
      profile: srtpProfile,
      clientGenerated,
    };
    const videoSession: NvstVideoSession = {
      clientUdpPort: clientPort,
      // Native-owned Mjolnir video socket: the native streamer reads raw-SRTP video
      // here while the bundle DTLS socket carries control/audio.
      mjolnirUdpPort: nativeMjolnirPort,
      videoPeerIp: videoPeer.ip,
      videoPeerPort: videoPeer.port,
      srtpAesKeyHex: srtp.aesKeyHex,
      srtpKeyId: srtp.keyId,
      srtpSaltHex: srtp.saltHex,
      srtpProfile: srtp.profile,
      pingPayload,
      pingVersion: Number.isFinite(pingVersion) ? pingVersion : undefined,
      localIceUsernameFragment: localIceCredentials?.usernameFragment,
      localIcePassword: localIceCredentials?.password,
      remoteIceUsernameFragment: iceRemoteUfrag
        ?? (pingVersion === 6 ? pingPayload : undefined),
      remoteIcePassword: iceCredentials?.password,
      localDtlsFingerprint,
      remoteDtlsFingerprint: dtlsFingerprint ?? undefined,
      codec: input.codec,
      timeoutMs: 60_000,
    };
    if (input.onVideoReady) {
      // Official binds Mjolnir before SETUP but does not send STUN until after ANNOUNCE.
      log(
        input.onLog,
        officialCloudPath
          ? `Video SETUP ready; deferring STUN until after ANNOUNCE (clientUdp ${clientPort})`
          : `Video SETUP ready; keeping STUN hole-punch through remaining SETUP/ANNOUNCE (clientUdp ${clientPort})`,
      );
      await input.onVideoReady(videoSession);
      steps.push("native-receive-armed");
    }

    const setupTransport = (port: number): string =>
      `unicast;X-GS-ClientPort=${port}-${port + 1}`;
    let audioClientPort = clientPort;
    let controlClientPort: number | undefined;
    if (!officialCloudPath) {
      audioUdp = await dependencies.reserveUdpPort();
      audioClientPort = audioUdp.port;
      const audioSetup = await client.request("SETUP", audioControl, {
        ...commonHeaders,
        Session: session,
        Transport: setupTransport(audioClientPort),
      });
      if (audioSetup.statusCode !== 200) {
        throw new Error(`SETUP audio failed: ${audioSetup.statusCode} ${audioSetup.statusText}`);
      }
      steps.push("setup-audio");
      session = header(audioSetup.headers, "session")?.split(";")[0]?.trim() ?? session;
      log(input.onLog, `SETUP ${audioControl} ok (clientPort=${audioClientPort})`);
      const audioPeer = extractVideoPeer(header(audioSetup.headers, "transport"));
      if (audioPeer && audioUdp && audioUdp !== udp) {
        startAuthenticatedHolePunch(audioUdp, audioPeer, {
          nattUsername: header(audioSetup.headers, "x-nv-ping-payload") ?? pingPayload,
        });
      }

      for (const control of describedControls) {
        if (control === videoControl || control === audioControl) {
          continue;
        }
        const reservation = await dependencies.reserveUdpPort();
        if (reservation !== udp) {
          auxiliaryUdp.push(reservation);
        }
        if (control === controlStream) {
          controlClientPort = reservation.port;
        }
        const auxiliarySetup = await client.request("SETUP", control, {
          ...commonHeaders,
          Session: session,
          Transport: setupTransport(reservation.port),
        });
        if (auxiliarySetup.statusCode !== 200) {
          throw new Error(`SETUP ${control} failed: ${auxiliarySetup.statusCode} ${auxiliarySetup.statusText}`);
        }
        steps.push(`setup-${control}`);
        session = header(auxiliarySetup.headers, "session")?.split(";")[0]?.trim() ?? session;
        const auxiliaryPeer = extractVideoPeer(header(auxiliarySetup.headers, "transport"));
        if (auxiliaryPeer && reservation !== udp) {
          startAuthenticatedHolePunch(reservation, auxiliaryPeer, {
            nattUsername: header(auxiliarySetup.headers, "x-nv-ping-payload") ?? pingPayload,
          });
        }
        log(
          input.onLog,
          `SETUP ${control} ok (clientPort=${reservation.port}, peer=${auxiliaryPeer ? `${auxiliaryPeer.ip}:${auxiliaryPeer.port}` : "absent"}, pingVersion=${header(auxiliarySetup.headers, "x-nv-ping") ?? "legacy"})`,
        );
      }
    } else {
      log(
        input.onLog,
        "Official cloud path: skipping audio/mic/control SETUP (WebRtcTransport owns those streams)",
      );
    }

    const localIpv4 = udp.localAddress ?? pickLocalIpv4();
    const clientTransport = officialCloudPath
      ? undefined
      : (localIpv4 ? `${localIpv4}:${clientPort}` : undefined);
    const announce = await client.request(
      "ANNOUNCE",
      officialCloudPath ? rtspTarget : "/",
      {
        ...commonHeaders,
        Session: session,
        "Content-Type": "application/sdp",
      },
      buildAnnounceSdp(officialCloudPath
        ? {
          resolution: input.resolution,
          fps: input.fps,
          // Official always advertises the runtime encryptionKey in ANNOUNCE (it keys the
          // video SRTP on the separate non-DTLS socket). Now that we always generate it,
          // send it unconditionally rather than dropping it when a DTLS fingerprint exists.
          encryptionKeyHex,
          encryptionKeyId,
          iceCredentials: localIceCredentials ?? undefined,
          includeNvscLegacyIce: false,
          includeNvscLegacyDtls: false,
          videoPort: videoPeer.port,
          clientPorts: {
            video: 0,
            audio: 0,
            mic: 0,
            control: 0,
            bundle: 0,
            session: 0,
            localAddress: localIpv4,
          },
          clientBundlePort: clientPort,
          nativeRtcOnBundlePort: "1",
          rtcVideoOnNativeBundle: false,
          rtcAudioOnNativeBundle: true,
          rtcMicOnNativeBundle: true,
          rtcDataChannelOnNativeBundle: true,
          enableUnifiedSocket: false,
          // The native streamer opens the `rtcp1` SCTP data channel on the bundle and
          // sends RTCP Receiver Reports / PLI over it, so advertise RTCP-over-SCTP.
          rtcpOnSctp: true,
          dtlsFingerprint: localDtlsFingerprint,
        }
        : {
          resolution: input.resolution,
          fps: input.fps,
          encryptionKeyHex: describedKey || !dtlsFingerprint ? encryptionKeyHex : undefined,
          encryptionKeyId: describedKey || !dtlsFingerprint ? encryptionKeyId : undefined,
          iceCredentials: localIceCredentials ?? undefined,
          videoPort: videoPeer.port,
          clientPorts: {
            video: clientPort,
            audio: audioClientPort,
            control: controlClientPort,
            localAddress: localIpv4,
          },
          clientTransport,
          dtlsFingerprint: localDtlsFingerprint,
        }),
    );
    if (announce.statusCode !== 200) {
      throw new Error(`ANNOUNCE failed: ${announce.statusCode} ${announce.statusText}`);
    }
    steps.push("announce");
    log(
      input.onLog,
      `ANNOUNCE ok (allowlist${encryptionKeyHex && (describedKey || !dtlsFingerprint) ? " + encryptionKey" : ""}${localIceCredentials ? ` + ICE V2 credentials (local ufragBytes=${localIceCredentials.usernameFragment.length}, pwdBytes=${localIceCredentials.password.length})` : ""}${localDtlsFingerprint ? ` + dtlsFingerprintBytes=${localDtlsFingerprint.length}` : ""}${localIpv4 ? ` + localAddress=${localIpv4}` : ""}${clientTransport ? ` + clientTransport=${clientTransport}` : ""}${localIpv4 && localIceCredentials ? ` + host candidate ${localIpv4}:${clientPort}` : ""})`,
    );
    if (officialCloudPath && iceCredentials && localIceCredentials) {
      log(
        input.onLog,
        `Starting official bundle STUN after ANNOUNCE (clientUdp ${clientPort}, iceRemote=${iceRemoteUfrag ?? "absent"})`,
      );
      videoHolePunchTimer = startAuthenticatedHolePunch(
        udp,
        videoPeer,
        { nattUsername: "PING" },
      );
    }
    if (input.onAnnounceReady) {
      log(
        input.onLog,
        `Starting native WebRtcTransport after ANNOUNCE (clientUdp ${clientPort})`,
      );
      await input.onAnnounceReady(videoSession);
      steps.push("native-announce-armed");
    }
    if (officialCloudPath && iceCredentials && localIceCredentials && mjolnirUdp) {
      // Probe-owned fallback only. When the native streamer owns the Mjolnir socket
      // (nativeMjolnirPort set), its raw-SRTP receiver already runs this NATT
      // keepalive — running a second one here would fight over the same socket.
      // Official RtpSourceQueue on 49005 starts ~1ms before PLAY, after DTLS.
      log(
        input.onLog,
        `Starting official Mjolnir NATT before PLAY (mjolnirPort=${mjolnirUdp.port})`,
      );
      startAuthenticatedHolePunch(mjolnirUdp, videoPeer, {
        iceBurst: false,
        nattUsername: pingPayload && pingPayload !== "PING" ? pingPayload : "PING",
      });
    } else if (officialCloudPath && nativeMjolnirPort !== undefined) {
      log(
        input.onLog,
        `Mjolnir NATT owned by native streamer (mjolnirPort=${nativeMjolnirPort}); native raw-SRTP receiver keeps it alive`,
      );
    }
    if (officialCloudPath && disablePlay === "0") {
      // Official waits for DTLS after setupWebRtcTransport, then PLAY returns 200.
      await new Promise((resolve) => {
        setTimeout(resolve, 400);
      });
    }

    if (disablePlay === "0") {
      try {
        const play = await client.request("PLAY", officialCloudPath ? rtspTarget : "/", {
          ...commonHeaders,
          Session: session,
        });
        if (play.statusCode === 200) {
          steps.push("play");
          log(input.onLog, "PLAY / ok");
        } else if (play.statusCode === 455) {
          steps.push("play-455");
          log(
            input.onLog,
            `PLAY / returned 455 ${play.statusText} — treating as Bifrost ANNOUNCE-only`,
          );
        } else {
          steps.push("play-failed");
          log(
            input.onLog,
            `PLAY / returned ${play.statusCode} ${play.statusText}; continuing after ANNOUNCE`,
          );
        }
      } catch (error) {
        steps.push("play-timeout");
        log(
          input.onLog,
          `PLAY / failed (${error instanceof Error ? error.message : String(error)}); continuing after ANNOUNCE`,
        );
      }
    } else {
      steps.push("play-skipped");
      log(input.onLog, "PLAY skipped because DESCRIBE disabled it after ANNOUNCE");
    }
    log(
      input.onLog,
      `ANNOUNCE complete — NVST video handoff ready with video UDP still bound (peer ${videoPeer.ip}:${videoPeer.port}, clientUdp ${clientPort}, clientTransport=${clientTransport ?? "absent"})`,
    );

    let released = false;
    const releaseVideoUdp = async (): Promise<void> => {
      // Keep STUN hole-punch on the native-owned bundle socket until the RTSP
      // session is fully released. Official treats punch-receive failure as
      // non-fatal; stopping sends as soon as native starts drops inbound DTLS.
      await udp?.release().catch(() => undefined);
      udp = null;
    };
    return {
      endpoint,
      session,
      hmacSeedPresent: Boolean(hmacSeed),
      videoPeer,
      clientUdpPort: clientPort,
      srtp,
      pingPayload,
      pingVersion: Number.isFinite(pingVersion) ? pingVersion : undefined,
      videoSession,
      steps,
      videoUdpFd: udp.fd,
      handoffVideoUdp: async () => {
        if (released || !udp) {
          return;
        }
        log(input.onLog, `Releasing Electron video UDP copy after native inherited the socket (clientUdp ${clientPort})`);
        await releaseVideoUdp();
      },
      release: async (reason = "NVST session released") => {
        if (released) {
          return;
        }
        released = true;
        for (const timer of holePunchTimers.splice(0)) {
          clearInterval(timer);
        }
        videoHolePunchTimer = null;
        await udp?.release().catch(() => undefined);
        udp = null;
        await mjolnirUdp?.release().catch(() => undefined);
        mjolnirUdp = null;
        await audioUdp?.release().catch(() => undefined);
        audioUdp = null;
        await Promise.all(
          auxiliaryUdp.splice(0).map((reservation) => reservation.release().catch(() => undefined)),
        );
        await teardownAndClose(client, endpoint, session, reason, input.onLog);
      },
    };
  } catch (error) {
    for (const timer of holePunchTimers.splice(0)) {
      clearInterval(timer);
    }
    await udp?.release().catch(() => undefined);
    await mjolnirUdp?.release().catch(() => undefined);
    await audioUdp?.release().catch(() => undefined);
    await Promise.all(auxiliaryUdp.map((reservation) => reservation.release().catch(() => undefined)));
    await teardownAndClose(client, endpoint, session, "failed negotiation", input.onLog);
    if (error instanceof NvstRtspNegotiationError) {
      throw new NvstRtspNegotiationError(error.code, error.message, {
        cause: error,
      }, [...steps]);
    }
    const message = error instanceof Error ? error.message : String(error);
    throw new NvstRtspNegotiationError("negotiation-failed", message, {
      cause: error,
    }, [...steps]);
  }
}

export async function runNvstRtspHandshakeProbe(
  input: NvstRtspProbeInput,
  dependencies?: NvstRtspNegotiationDependencies,
): Promise<NvstRtspProbeResult> {
  const endpoint = selectPrimaryRtspsEndpoint(input.rtspsEndpoints) ?? "";
  try {
    const negotiated = await negotiateNvstRtspSession(input, dependencies);
    const result: NvstRtspProbeResult = {
      ok: true,
      endpoint: negotiated.endpoint,
      session: negotiated.session,
      hmacSeedPresent: negotiated.hmacSeedPresent,
      videoPeer: negotiated.videoPeer,
      clientUdpPort: negotiated.clientUdpPort,
      srtp: negotiated.srtp,
      pingPayload: negotiated.pingPayload,
      pingVersion: negotiated.pingVersion,
      videoSession: negotiated.videoSession,
      steps: negotiated.steps,
    };
    await negotiated.release("handshake probe complete");
    return result;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    log(input.onLog, `Probe failed: ${message}`);
    return {
      ok: false,
      endpoint,
      hmacSeedPresent: false,
      steps: error instanceof NvstRtspNegotiationError ? error.steps : [],
      error: message,
    };
  }
}
