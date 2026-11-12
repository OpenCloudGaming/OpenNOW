#!/usr/bin/env python3
"""Fill <!-- GEN:name --> ... <!-- /GEN:name --> blocks in ../cloudmatch-fields.md and ../sdp-attributes.md from extracted/*.tsv.

Run after extract.py and compare_opennow.py. Reads only already-redacted extracted files.
"""
import collections
import argparse
import csv
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
EXT = os.path.join(HERE, "extracted")

FIELD_USE = {
    "appId": "CMS app id of the title to launch",
    "externalAppId": "store-side app id; null in request, echoed as string in response",
    "internalTitle": "internal title name (null)",
    "availableSupportedControllers[]": "controller types the client supports (2)",
    "preferredController": "preferred controller type (2); response echoes 0",
    "networkTestSessionId": "id of the pre-launch network test; null in request, filled in response",
    "parentSessionId": "parent session for sub-sessions (null)",
    "clientIdentification": "client family string",
    "deviceHashId": "stable device id (secret)",
    "clientVersion": "CloudMatch client API version",
    "sdkVersion": "Bifrost SDK version",
    "streamerVersion": "NVST streamer protocol major version (14 => RTSPS/WSS path)",
    "clientPlatformName": "OS name",
    "monitorId": "monitor index",
    "positionX": "monitor origin X",
    "positionY": "monitor origin Y",
    "widthInPixels": "requested stream width",
    "heightInPixels": "requested stream height",
    "dpi": "monitor DPI percent (100 on Windows); response monitorSettings echoes 0",
    "framesPerSecond": "requested fps",
    "sdrHdrMode": "0 = SDR, 1 = HDR",
    "displayData": "HDR display metadata (null for SDR)",
    "hdr10PlusGamingData": "HDR10+ gaming metadata (null)",
    "useOps": "request OPS (true in request, false echoed)",
    "audioMode": "2 = stereo per Geronimo 'AudioMode used for Start stream session: 2'",
    "metaData[].key": "free-form key/value list",
    "metaData[].value": "value for metaData key",
    "clientDisplayHdrCapabilities": "HDR caps of the display (null for SDR)",
    "surroundAudioInfo": "top-level 0; metaData carries surroundAudioInfo=2",
    "remoteControllersBitmap": "bitmap of attached remote controllers (1)",
    "clientTimezoneOffset": "ms offset from UTC (UTC+2 here)",
    "enhancedStreamMode": "0 on NVIDIA seats",
    "appLaunchMode": "1 = DEFAULT ('App Launch Mode: DEFAULT')",
    "secureRTSPSupported": "client can do RTSPS",
    "partnerCustomData": "partner blob; null in request, '***' masked in response",
    "accountLinked": "store account linked",
    "enablePersistingInGameSettings": "persist in-game settings (true in request, false echoed)",
    "requestedAudioFormat": "0 = default",
    "userAge": "account holder age (PII, redacted)",
    "reflex": "request NVIDIA Reflex",
    "bitDepth": "0 = 8-bit, 1 = 10-bit",
    "cloudGsync": "cloud G-SYNC / VRR",
    "enabledL4S": "L4S ECN congestion control",
    "mouseMovementFlags": "mouse movement mode flags",
    "trueHdr": "RTX HDR (false)",
    "supportedHidDevices": "HID device bitmap (0)",
    "profile": "streaming profile id (0)",
    "fallbackToLogicalResolution": "allow logical-resolution fallback",
    "hidDevices": "explicit HID list (null)",
    "chromaFormat": "0 = 4:2:0, 1 = 4:4:4 (S6 decoded as 4:4:4, DXGI 0x65)",
    "prefilterMode": "encoder prefilter mode; server proposed 2, client finalized 0",
    "prefilterSharpness": "prefilter sharpness",
    "prefilterNoiseReduction": "prefilter denoise",
    "hudStreamingMode": "HUD streaming mode",
    "qosPolicy": "QoS policy (0)",
    "touchSupport": "touch input support",
    "srMode": "DLSS super-resolution override",
    "srModel": "DLSS SR model override",
    "srScale": "DLSS SR scale percent",
    "rrModel": "DLSS ray-reconstruction model",
    "fgMode": "DLSS frame-generation mode",
    "fgModel": "DLSS FG model",
    "fgMultiplier": "DLSS FG multiplier",
    "transport": "transport override (null)",
    "sessionAds": "ad payload (null)",
    "protocol": "transport protocol enum (0 for control HTTPS, 1 for RTSPS)",
    "appLevelProtocol": "5 = CloudMatch HTTPS session control, 6 = RTSPS",
    "ip": "host; sessionControlInfo.ip is the per-zone CloudMatch host, connectionInfo.ip is null",
    "resourcePath": "session-control path, or rtsps:// URL once the seat is assigned (null before)",
    "usage": "JSON port usage: 10 = session control, 14 = SIGNALING; native RTSPS is 10",
    "port": "TCP port",
    "rigId": "rig id (null)",
    "requestedStreamingFeatures": "echoed as null in response",
    "finalSelectedScreenResolution": "null in all responses",
    "finalizedStreamingFeatures": "null until the ready response",
    "adProvider": "ad provider id (1)",
    "sessionAdsRequired": "ads gate (false)",
    "errorCode": "1 = success in all responses",
    "sessionId": "CloudMatch session id (secret)",
    "queuePosition": "queue position (0)",
    "seatSetupStep": "0 = allocating, 3 = configuring seat",
    "seatSetupEta": "ETA in ms",
    "status": "1 = setting up, 2 = ready",
    "clientIp": "client public IP (redacted)",
    "gpuType": "seat GPU class, empty until assigned",
    "monitorSettings": "null until ready; then the finalized monitor list",
    "userIdleWarningTimeoutInMs": "idle warning (0; idle timer arrived via NVST control instead)",
    "countryCode": "null",
    "unifiedErrorCode": "0 = no error",
    "requestId": "per-request id (secret)",
    "serverId": "zone/server id (NP-AMS-08 style)",
    "statusDescription": "SUCCESS_STATUS",
    "statusCode": "1 = success",
}

SDP_SIDE_NOTE = {
    "server(DESCRIBE)": "server",
}


def read_tsv(name):
    with open(os.path.join(EXT, name), newline="") as f:
        return list(csv.reader(f, delimiter="\t"))


def esc(s):
    return s.replace("|", "\\|")


def field_use(path):
    leaf = path.rsplit(".", 1)[-1]
    return FIELD_USE.get(leaf) or FIELD_USE.get(leaf.replace("[]", "")) or ""


def gen_cloudmatch_fields():
    rows = read_tsv("cloudmatch_fields.tsv")[1:]
    out = ["| dir | field path | type | example (redacted) | use |", "|---|---|---|---|---|"]
    for d, path, ty, ex in rows:
        if path.startswith("otherUserSessions[]."):
            continue
        out.append(f"| {d} | `{path}` | {ty} | {esc(ex[:90])} | {field_use(path)} |")
    other = sum(1 for r in rows if r[1].startswith("otherUserSessions[]."))
    out.append("")
    out.append(f"`otherUserSessions[]` repeats the `session` object shape ({other} paths, same values as the pending session); it is omitted above.")
    return "\n".join(out)


def gen_sdp_server():
    rows = read_tsv("sdp_server_xnv.tsv")[1:]
    groups = collections.OrderedDict()
    for _t, side, key, val in rows:
        if key == "<sdp-line>":
            groups.setdefault("SDP header lines", []).append((val, ""))
            continue
        k = key[len("x-nv-"):]
        grp = re.match(r"([a-zA-Z]+(?:\[\d+\])?)", k).group(1)
        groups.setdefault(grp, []).append((k, val))
    out = []
    for g, items in groups.items():
        out.append(f"**{g}** ({len(items)})")
        out.append("")
        out.append("| key | value |")
        out.append("|---|---|")
        for k, v in items:
            out.append(f"| `{esc(k)}` | {esc(v)} |")
        out.append("")
    return "\n".join(out).rstrip()


def gen_compare():
    rows = read_tsv("xnv_compare.tsv")
    hdr, rows = rows[0], rows[1:]
    out = ["| key | Windows server DESCRIBE | Windows final config | OpenNOW ANNOUNCE literal | OpenNOW source |", "|---|---|---|---|---|"]
    for r in rows:
        r = (r + [""] * 5)[:5]
        src = r[4].replace("native/opennow-streamer/crates/opennow-streamer-core/src/", "")
        out.append(f"| `{esc(r[0])}` | {esc(r[1])} | {esc(r[2])} | {esc(r[3])} | {esc(src)} |")
    return "\n".join(out)


def gen_config_groups():
    rows = read_tsv("nvsc_config.tsv")[1:]
    groups = collections.Counter()
    for r in rows:
        groups[re.match(r"([a-zA-Z]+)", r[0]).group(1)] += 1
    out = ["| group | keys in final NvscClientConfig dump |", "|---|---|"]
    for g, n in sorted(groups.items(), key=lambda x: -x[1]):
        out.append(f"| `{g}` | {n} |")
    out.append(f"| total | {sum(groups.values())} |")
    return "\n".join(out)


GENS = {
    "cloudmatch_fields": gen_cloudmatch_fields,
    "sdp_server": gen_sdp_server,
    "xnv_compare": gen_compare,
    "config_groups": gen_config_groups,
}


def fill(path):
    text = open(path).read()
    for name, fn in GENS.items():
        pat = re.compile(rf"(<!-- GEN:{name} -->\n).*?(<!-- /GEN:{name} -->)", re.S)
        text = pat.sub(lambda m: m.group(1) + fn() + "\n" + m.group(2), text)
    open(path, "w").write(text)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--extracted", default=EXT)
    parser.add_argument("documents", nargs="*", default=["cloudmatch-fields.md", "sdp-attributes.md"])
    args = parser.parse_args()
    EXT = args.extracted
    for p in args.documents:
        fill(os.path.join(HERE, "..", p))
