#!/usr/bin/env python3
"""Extract and redact streaming-session evidence from a GeForce NOW Windows log folder.

Usage: extract.py [--src GFN_DIR] [--out OUT_DIR]

Reads geronimo.log(.bak), console.log(.bak), CxNative_GeForceNOW*.log, logs/gameStreamClientAgent.log*,
logs/gfn_reliability_monitor.log*, and writes redacted summaries to OUT_DIR. All session ids are
replaced by labels (S1, S2, ...) assigned in chronological order; nothing user-specific is embedded
in this script. Times are local wall-clock time as printed by the client (the geronimo/agent
"=HH:MM:SS=" column, the console/CxNative timestamps).
"""
import argparse
import collections
import datetime as dt
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from redact import SENSITIVE_KEYS, Redactor  # noqa: E402

UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", re.I)
GER_LINE = re.compile(
    r"^\[(\d{4}-\d\d-\d\d) (\d\d:\d\d:\d\d),(\d{3})\]=(\d\d:\d\d:\d\d)=\s+(\w+) \[([^\]]+)\] \{([0-9A-F]+)\} - ?(.*)$")
CON_LINE = re.compile(r"^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3}) (\w+)\s+(\S+)\s+(.*)$")
CX_LINE = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3})\[(\w)\]\s*(.*)$")


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


class Entry:
    __slots__ = ("t", "level", "tag", "tid", "msg", "src", "line")

    def __init__(self, t, level, tag, tid, msg, src, line):
        self.t, self.level, self.tag, self.tid, self.msg, self.src, self.line = t, level, tag, tid, msg, src, line


def parse_geronimo_style(path):
    """geronimo.log and gameStreamClientAgent.log share the log4cxx-style layout."""
    out = []
    if not os.path.exists(path):
        return out
    for n, raw in enumerate(read(path).splitlines(), 1):
        m = GER_LINE.match(raw)
        if m:
            d, utc, ms, local, level, tag, tid, msg = m.groups()
            day = dt.date.fromisoformat(d)
            u = dt.datetime.combine(day, dt.time.fromisoformat(utc))
            loc = dt.datetime.combine(day, dt.time.fromisoformat(local))
            if loc - u < dt.timedelta(hours=-12):
                loc += dt.timedelta(days=1)
            out.append(Entry(loc + dt.timedelta(milliseconds=int(ms)), level, tag.strip(), tid, msg, os.path.basename(path), n))
        elif out and raw.strip():
            out[-1].msg += "\n" + raw
    return out


def parse_console(path):
    out = []
    if not os.path.exists(path):
        return out
    for n, raw in enumerate(read(path).splitlines(), 1):
        m = CON_LINE.match(raw)
        if m:
            ts, level, comp, msg = m.groups()
            out.append(Entry(dt.datetime.strptime(ts, "%Y-%m-%d %H:%M:%S.%f"), level, comp, "", msg, os.path.basename(path), n))
        elif out:
            out[-1].msg += "\n" + raw
    return out


def parse_cx(path, keep):
    out = []
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8", errors="replace") as f:
        for n, raw in enumerate(f, 1):
            if not keep.search(raw):
                continue
            m = CX_LINE.match(raw)
            if m:
                out.append(Entry(dt.datetime.fromisoformat(m.group(1)), m.group(2), "", "", m.group(3).rstrip(), os.path.basename(path), n))
    return out


def json_after(msg, marker):
    i = msg.find(marker)
    if i < 0:
        return None
    s = msg[i + len(marker):].strip()
    j = s.find("{")
    if j < 0:
        return None
    try:
        return json.JSONDecoder().raw_decode(s[j:])[0]
    except ValueError:
        return None


def fmt(t):
    return t.strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]


def flatten(obj, prefix="", out=None):
    if out is None:
        out = collections.OrderedDict()
    if isinstance(obj, dict):
        if not obj:
            out.setdefault(prefix or "$", []).append(("object", "{}"))
        for k, v in obj.items():
            flatten(v, f"{prefix}.{k}" if prefix else k, out)
    elif isinstance(obj, list):
        if not obj:
            out.setdefault(prefix + "[]", []).append(("array", "[]"))
        for v in obj:
            flatten(v, prefix + "[]", out)
    else:
        t = {bool: "bool", int: "int", float: "float", str: "string", type(None): "null"}[type(obj)]
        out.setdefault(prefix, []).append((t, json.dumps(obj)))
    return out


class Labeler:
    def __init__(self):
        self.ids = {}

    def __call__(self, sid):
        sid = sid.lower()
        if sid not in self.ids:
            self.ids[sid] = f"S{len(self.ids) + 1}"
        return self.ids[sid]

    def known(self, sid):
        return self.ids.get(sid.lower())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, help="unzipped GFN install + logs folder")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "extracted"))
    a = ap.parse_args()
    src, outdir = a.src, a.out
    os.makedirs(outdir, exist_ok=True)
    P = lambda *p: os.path.join(src, *p)  # noqa: E731

    red = Redactor()
    for f in ("logs/gfn_reliability_monitor.log", "logs/gameStreamClientAgent.log", "geronimo.log.bak", "console.log"):
        if os.path.exists(P(f)):
            red.learn(read(P(f))[:2_000_000])

    agent = parse_geronimo_style(P("logs/gameStreamClientAgent.log.bak")) + parse_geronimo_style(P("logs/gameStreamClientAgent.log"))
    ger = parse_geronimo_style(P("geronimo.log.bak")) + parse_geronimo_style(P("geronimo.log"))
    con = parse_console(P("console.log.bak")) + parse_console(P("console.log"))
    cx_keep = re.compile(r"onStreamStart|onStreamStop|SessionRequestTask|CefPlugin\.cpp|MessageBusConnector|PluginModuleBase\.cpp:3(15|23)|StreamingStarted|NotifyGameStart should")
    cx = parse_cx(P("CxNative_GeForceNOW.1.log"), cx_keep) + parse_cx(P("CxNative_GeForceNOW.log"), cx_keep)
    label = Labeler()

    def W(name, text):
        with open(os.path.join(outdir, name), "w", encoding="utf-8") as f:
            f.write(red(text))

    # ---------------- sessions from the GameStream ClientAgent IPC stream ----------------
    sessions = []
    cur = None
    ipc_re = re.compile(r"IPCMessage: (\S+) (\S+) (IPC_[A-Z_]+) (.*)$", re.S)
    for e in agent:
        m = ipc_re.search(e.msg)
        if not m:
            continue
        ev, payload = m.group(3), m.group(4)
        try:
            pj = json.loads(payload)
        except ValueError:
            pj = {}
        if ev in ("IPC_STREAMING_PREPARE_EVENT", "IPC_STREAMING_STARTING_EVENT") and (cur is None or cur.get("closed")):
            cur = {"launch": e.t, "events": [], "zone": pj.get("serverAddress", "")}
            sessions.append(cur)
        if cur is None:
            continue
        cur["events"].append((e.t, ev, pj))
        if ev == "IPC_STREAMING_SESSION_SETUP_EVENT":
            cur["sid"] = pj.get("sessionId", "")
            cur["label"] = label(cur["sid"])
            cur["setup"] = e.t
        elif ev == "IPC_STREAMING_STARTED_EVENT":
            cur["started"] = e.t
        elif ev == "IPC_STREAMING_TERMINATED_EVENT":
            cur["terminated"] = e.t
            cur["term"] = pj
        elif ev == "IPC_STREAMING_FAILURE_EVENT":
            cur.setdefault("failures", []).append((e.t, pj.get("event"), pj.get("result", {})))
        elif ev == "IPC_STREAMING_MODE_EXIT_EVENT":
            cur["exit"] = e.t
            cur["closed"] = True

    # console-side per-session facts, keyed by time window
    con_facts = []
    for e in con:
        if e.tag == "streamingService" and e.msg.startswith("Received Codec"):
            j = json_after(e.msg, "Received Codec")
            con_facts.append((e.t, "codec", (j or {}).get("codec")))
        elif e.tag == "streamingService" and e.msg.startswith("start {"):
            j = json_after(e.msg, "start") or {}
            sp = j.get("streamingProfile", {})
            con_facts.append((e.t, "profile", f"{sp.get('width')}x{sp.get('height')}@{sp.get('fps')} bitDepth={sp.get('bitDepth')} chromaFormat={sp.get('chromaFormat')} hdr={sp.get('hdrStreamingMode')} codecPref={sp.get('codec')} audioMode={sp.get('audioMode')} l4s={sp.get('l4sEnabled')} reflex={sp.get('reflex')} app={j.get('appName')} zone={j.get('zoneName')}"))
        elif e.tag == "gfn/loadingUi" and "SessionSetupProgress" in e.msg:
            j = json_after(e.msg, "EventInfo") or {}
            con_facts.append((e.t, "queue", f"queuePosition={j.get('queuePosition')} eta={j.get('eta')} state={j.get('state')} isAdsRequired={j.get('isAdsRequired')}"))
        elif e.tag == "gfn/loadingUi" and e.msg.startswith("StreamingSessionId:"):
            m = UUID.search(e.msg)
            if m:
                con_facts.append((e.t, "sid", m.group(0)))
        elif e.tag == "streamingService" and e.msg.startswith(("onFrameLossWarning", "onFrameLossCleared")):
            con_facts.append((e.t, "frameloss", e.msg.split()[0]))
        elif "stream client message event" in e.msg:
            j = json_after(e.msg, "event:") or {}
            msg = j.get("message", {})
            con_facts.append((e.t, "clientmsg", f"{msg.get('messageType')} {json.dumps(msg.get('timerData', {}))}"))
        elif e.tag == "MicrophoneService" and e.msg.startswith("Current mic state"):
            con_facts.append((e.t, "mic", e.msg.split(":", 1)[1].strip()))

    def window(s):
        end = s.get("exit") or s.get("terminated") or (s["launch"] + dt.timedelta(hours=8))
        return s["launch"] - dt.timedelta(seconds=5), end + dt.timedelta(seconds=5)

    cx_starts = [(e.t, e.msg) for e in cx if "onStreamStart" in e.msg]
    cx_stops = [(e.t, e.msg) for e in cx if "onStreamStop" in e.msg]

    rows = []
    hdr = ["launch#", "session", "launch(local)", "setup", "started", "terminated", "exit", "queue/setup_s", "stream_min",
           "codec(js)", "profile(js)", "term_reason", "extendedCode", "resumable", "sessionAlive", "failures", "app(cx)", "geronimo_log"]
    for i, s in enumerate(sessions, 1):
        lo, hi = window(s)
        facts = [f for f in con_facts if lo <= f[0] <= hi]
        codec = ",".join(sorted({str(f[2]) for f in facts if f[1] == "codec"}))
        prof = next((f[2] for f in facts if f[1] == "profile"), "")
        term = s.get("term", {})
        st, tm = s.get("started"), s.get("terminated")
        setup_s = f"{(s['setup'] - s['launch']).total_seconds():.1f}" if s.get("setup") else ""
        dur = f"{(tm - st).total_seconds() / 60:.1f}" if st and tm else ""
        fails = ";".join(f"{f[1]}(unifiedErrorCode={f[2].get('unifiedErrorCode', '')},statusCode={f[2].get('statusCode', '')})" for f in s.get("failures", []))
        app = ""
        for t, m in cx_starts:
            if st and abs((t - st).total_seconds()) < 120:
                mm = re.search(r"drsAppName:(.*?) drsProfileName", m)
                app = mm.group(1) if mm else ""
        g = [e for e in ger if lo <= e.t <= hi]
        rows.append([str(i), s.get("label", "-"), fmt(s["launch"]), fmt(s["setup"]) if s.get("setup") else "", fmt(st) if st else "",
                     fmt(tm) if tm else "", fmt(s["exit"]) if s.get("exit") else "", setup_s, dur, codec, prof,
                     str(term.get("reason", "")), f"0x{term['extendedCode']:08X}" if "extendedCode" in term else "",
                     str(term.get("isResumable", "")), str(term.get("isSessionAlive", "")), fails, app,
                     f"{len(g)} lines" if g else "not covered"])
        s["facts"] = facts
    W("sessions.tsv", "\t".join(hdr) + "\n" + "\n".join("\t".join(r) for r in rows) + "\n")

    # per-launch IPC + console + cx sequence
    lines = []
    for i, s in enumerate(sessions, 1):
        lines.append(f"=== launch #{i} session {s.get('label', '-')} zone {s.get('zone')}")
        t0 = s["launch"]
        seq = [(t, "IPC", ev + (" " + json.dumps({k: v for k, v in pj.items() if k not in ('udsData',)}) if pj else "")) for t, ev, pj in s["events"]]
        lo, hi = window(s)
        seq += [(t, "JS", f"{k}: {v}") for t, k, v in s["facts"] if k != "sid"]
        seq += [(t, "CX", m[:160]) for t, m in cx_starts + cx_stops if lo <= t <= hi]
        if "term" in s and "udsData" in s["term"]:
            seq.append((s["terminated"], "IPC", "udsData " + json.dumps(s["term"]["udsData"])))
        for t, kind, txt in sorted(seq, key=lambda x: x[0]):
            lines.append(f"  +{(t - t0).total_seconds():8.3f}s {fmt(t)} {kind:3} {txt}")
    W("launch_sequences.txt", "\n".join(lines) + "\n")

    # ---------------- geronimo per-session timelines ----------------
    starts = [k for k, e in enumerate(ger) if "nvbStartSession(" in e.msg]
    bounds = []
    if ger and (not starts or starts[0] > 0):
        bounds.append((0, starts[0] if starts else len(ger), True))
    for n, k in enumerate(starts):
        bounds.append((k, starts[n + 1] if n + 1 < len(starts) else len(ger), False))
    key_re = re.compile("|".join([
        r"nvbStartSession\(", r"nvbStartStreaming\(", r"Serialized session request", r"Session request URL", r"Polling for session",
        r"ConnectionInfo\[\d\]", r"Received session info event", r"Resolution selected for streaming", r"Preferred streaming settings",
        r"Will request", r"createDx11SwapChain", r"Initializing render client", r"Channel count", r"Using Secure WebSockets",
        r"Connecting to host", r"WSS (Options|Describe|Setup|Announce|Play)(:|\s)", r"Rtsp\w+ took", r"Header version",
        r"Server RTP extension|RTP extension header", r"Final Selected ControlProtocol", r"Selected \w+ codec", r"Video config:",
        r"Setting up VIDEO", r"Setup a stream", r"Server transport header", r"ping-version|ping-string|Old server only supports PING",
        r"Client early ICE socket", r"Bypassing 'Mjolnir'", r"Successfully (read|appended) \d+ NvscClientConfig", r"Skipping field",
        r"RtspConnectLatency", r"Remote input stream", r"onStreamerConnected", r"Setup dtls", r"DTLS handshake", r"Retrying DTLS",
        r"Peer address changed", r"Channel \w+ is created with stream-id", r"Initializing SctpDataChannelThreadSafe",
        r"Unified socket", r"SRTP for video", r"libsrtp", r"Audio redundancy|Mic redundancy", r"Opus encoder payloadSize \d+ ,",
        r"NACK waiting queue", r"Max allowed pending NACKS", r"qosFeedbackVersion|qosTimingsVersion", r"Nack DJB interaction",
        r"Dynamic FEC", r"Blob stats version", r"Successfully sent PLAY", r"Successfully started streaming", r"received first payload",
        r"UDP RTP Source Connect", r"Stream (resolution|colorspace|chroma format|bit depth) changed", r"Sending DJB Config",
        r"frameRate =", r"HdrMode update", r"Sending haptics state", r"Configuring server supported Hid devices",
        r"Server sent a timer notification", r"APPROACHING_IDLE", r"termination notification", r"onStreamerStopped",
        r"ExtendedTermination", r"Resumable: \d", r"Received old frame", r"Activating Rtp Nack|Deactivating Rtp Nack",
        r"RTP NACK Stats|RTP Stats", r"Packet Loss %", r"FirstFrameLatency|rtspHandshakeLatency", r"Signal WSS disconnect",
        r"Disconnection reason", r"signalDisconnect\+", r"percPacketsDropped", r"SRTP encrypt failures", r"IceTransport::~IceTransport",
        r"last PING (sent|received)", r"Frame duration is HIGH", r"Total video frames dropped", r"Total missed Vsyncs",
        r"Prefilter::|HUD::", r"Converted NVB profile", r"maxNumOfPorts", r"Number of streams", r"Server/Client provided stream settings",
        r"Nvst selected stream settings", r"Initializing streamer connection with values", r"connectionInfoCount",
        r"Successfully destroyed NvstClient", r"Setup for seat configuration only", r"general\.serverEndpoints:",
        r"Encryption key in RTSPS", r"Updating standard STUN", r"Packetizer State", r"Server SDP (audio|mic)",
        r"Adding support for protocol", r"updateControlProtocol", r"packetSize of stream", r"QEC feature",
        r"Client requests|Client supports intra", r"Multi-ref|multi-reference", r"Mouse|haptics", r"Gamepad handling",
        r"Enabled feature type|Disabled feature type|Update feature type", r"MaxBitRate for", r"Streaming quality\[",
        r"Mic capturer|echoCancellationEnabled|Voice data", r"nvbSendMicFrame", r"Sending NVB_SN",
        r"Server sent .*notification", r"Unable to read server command", r"NVST_DISCONN", r"SessionTerminationReason",
        r"GOING FULLSCREEN", r"DPISCALE", r"View port changed", r"setVsyncEnabled", r"AV1|HEVC|H264",
    ]))
    skip_re = re.compile(r"NvscClientConfig\]|getMaxDecoderCapability|format supported for chroma|supports (long|short) slice|maxDecoderResolution|Transcoding  supported|Skipping resolution|Requested codec AV1 is disabled|Maximum decoder supported")
    tl = []
    for n, (a0, b0, partial) in enumerate(bounds, 1):
        seg = ger[a0:b0]
        if not seg:
            continue
        sid = None
        for e in seg:
            m = re.search(r"Received session info event: sessionid (" + UUID.pattern + ")", e.msg)
            if m:
                sid = m.group(1)
                break
        if sid is None:
            m = next((UUID.search(e.msg) for e in seg if "Streamer_Exit" in e.msg and "\"sessionId\"" in e.msg), None)
            mm = re.search(r'"sessionId":"(' + UUID.pattern + ')"', next((e.msg for e in seg if "Streamer_Exit" in e.msg), ""))
            sid = mm.group(1) if mm else None
        lab = label.known(sid) if sid else None
        lab = lab or (label(sid) if sid else "?")
        tl.append(f"=== geronimo segment {n} session {lab}{' (log starts mid-session)' if partial else ''}: {fmt(seg[0].t)} .. {fmt(seg[-1].t)} ({len(seg)} entries)")
        t0 = seg[0].t
        seen = collections.Counter()
        for e in seg:
            first = e.msg.split("\n", 1)[0]
            if skip_re.search(e.tag + "] " + first) or not key_re.search(first):
                continue
            tmpl = re.sub(r"\d+", "N", UUID.sub("<uuid>", e.tag + first))[:120]
            seen[tmpl] += 1
            if seen[tmpl] > 6:
                continue
            body = e.msg if re.search(r"WSS \w+: response|response header|Announce Parameter", first) else first
            body = re.sub(r"\n\s*\n", "\n", body).replace("\n", " | ")
            tl.append(f"  +{(e.t - t0).total_seconds():9.3f}s {fmt(e.t)} [{e.tag}] {body[:400]}")
        suppressed = {k: v for k, v in seen.items() if v > 6}
        if suppressed:
            tl.append("  (repeated templates, count beyond first 6 suppressed):")
            for k, v in sorted(suppressed.items(), key=lambda x: -x[1]):
                tl.append(f"    {v:6d}x {k}")
    W("geronimo_timeline.txt", "\n".join(tl) + "\n")

    # ---------------- runtime statistics ----------------
    st = []
    for n, (a0, b0, partial) in enumerate(bounds, 1):
        seg = ger[a0:b0]
        q, fs, rt, sctp = [], [], [], []
        for e in seg:
            if e.tag == "StatsInterface" and "Streaming quality" in e.msg:
                m = re.search(r"Q-score: (\S+)\s+BW-score: (\S+), Loss-score: (\S+), Latency-score: (\S+), Avg Game FPS: (\S+)", e.msg)
                if m:
                    q.append(tuple(float(x.rstrip(",")) for x in m.groups()))
            elif e.tag == "GeronimoStatsHandler" and e.msg.startswith("FrameStats"):
                fs.append(e.msg)
            elif "Current epoch real time metrics" in e.msg:
                rt.append(e.msg)
            elif e.tag == "NVST:SctpTransport" and "SCTP stats" in e.msg:
                sctp.append(e.msg)
        st.append(f"=== geronimo segment {n} ({fmt(seg[0].t)} .. {fmt(seg[-1].t)})")
        if q:
            cols = list(zip(*q))
            names = ["Q", "BW", "Loss", "Latency", "AvgGameFPS"]
            st.append("  Streaming quality samples: %d" % len(q))
            for nm, c in zip(names, cols):
                st.append(f"    {nm:10} min {min(c):7.1f} avg {sum(c) / len(c):7.1f} max {max(c):7.1f}")
        if fs:
            drops = [int(m.group(1)) for m in (re.search(r"dropped: (\d+)", x) for x in fs) if m]
            st.append(f"  FrameStats samples: {len(fs)}; dropped per-sample max {max(drops) if drops else 'n/a'} sum {sum(drops) if drops else 'n/a'}")
            st.append("    first: " + fs[0][:260])
            st.append("    last:  " + fs[-1][:260])
        for x in rt:
            st.append("  RT metrics: " + x[:300])
        if sctp:
            st.append("  SCTP stats last: " + sctp[-1][:300])
        for e in seg:
            if re.search(r"RTP NACK Stats|RTP Stats|percPacketsDropped|SRTP encrypt failures|Total video frames dropped|Total missed Vsyncs|Received old frame|Activating Rtp Nack|Deactivating Rtp Nack|Frame duration is HIGH|Nvst_RealTimeMetrics|retried due to buffer being full|last PING|stun binding", e.msg):
                st.append(f"  {fmt(e.t)} [{e.tag}] {e.msg.splitlines()[0][:420]}")
    W("runtime_stats.txt", "\n".join(st) + "\n")

    # ---------------- CloudMatch JSON ----------------
    cm = []
    for e in ger:
        if e.tag == "BifrostClient: GridServer" and ("Serialized session request" in e.msg or "server response:" in e.msg):
            m = re.search(r"(?:Data|server response): '(.*)'\s*$", e.msg, re.S)
            if m:
                try:
                    cm.append((e.t, "request" if "Serialized" in e.msg else "response", json.loads(m.group(1))))
                except ValueError:
                    pass
        elif e.tag == "BifrostClient: GridServer" and ("URL" in e.msg or "request url" in e.msg or "Polling for session" in e.msg or "ConnectionInfo[" in e.msg):
            cm.append((e.t, "note", e.msg))
    fields = collections.OrderedDict()
    for t, kind, j in cm:
        if kind == "note":
            continue
        for path, vals in flatten(j).items():
            d = fields.setdefault((kind, path), {"types": collections.Counter(), "examples": collections.Counter()})
            leaf = re.sub(r"\[\]", "", path).rsplit(".", 1)[-1]
            for ty, v in vals:
                d["types"][ty] += 1
                d["examples"]["<redacted>" if leaf in SENSITIVE_KEYS and v != "null" else v] += 1
    rowsf = ["direction\tpath\ttypes\tdistinct_values(redacted, up to 4)"]
    for (kind, path), d in fields.items():
        ex = list(d["examples"].keys())[:4]
        rowsf.append(f"{kind}\t{path}\t{','.join(d['types'])}\t{' | '.join(ex)[:300]}")
    W("cloudmatch_fields.tsv", "\n".join(rowsf) + "\n")
    cmlog = []
    for t, kind, j in cm:
        if kind == "note":
            cmlog.append(f"{fmt(t)} NOTE {j.splitlines()[0][:300]}")
            continue
        s = j.get("session", {}) if kind == "response" else {}
        brief = (f"status={s.get('status')} errorCode={s.get('errorCode')} seat={json.dumps(s.get('seatSetupInfo'))} "
                 f"connInfo={[(c.get('usage'), c.get('port'), c.get('protocol'), c.get('appLevelProtocol'), bool(c.get('resourcePath'))) for c in s.get('connectionInfo') or []]} "
                 f"requestStatus={json.dumps(j.get('requestStatus', {}).get('statusDescription'))}") if kind == "response" else "request"
        cmlog.append(f"{fmt(t)} {kind.upper()} {brief}")
    W("cloudmatch_exchange.txt", "\n".join(cmlog) + "\n")
    req = next((j for t, k, j in cm if k == "request"), None)
    resp = [j for t, k, j in cm if k == "response"]
    if req:
        W("cloudmatch_request.json", json.dumps(req, indent=1))
    if resp:
        W("cloudmatch_response_first.json", json.dumps(resp[0], indent=1))
        W("cloudmatch_response_ready.json", json.dumps(resp[-1], indent=1))

    # ---------------- RTSP exchange and SDP ----------------
    rt = []
    for e in ger:
        if e.tag.startswith("NVST:RtspSession") or (e.tag == "NVST:SignalingHandler" and ("transport header" in e.msg or "new features" in e.msg)):
            body = re.sub(r"\n\s*\n", "\n", e.msg)
            if "new features" in body:
                body = body.split("\n")[0] + f" ... ({body.count('a=')} SDP attribute lines, see sdp_server_xnv.tsv)"
            rt.append(f"{fmt(e.t)} [{e.tag}] {body}")
    W("rtsp_exchange.txt", "\n".join(rt) + "\n")
    sdp = []
    for e in ger:
        if "new features:" in e.msg:
            for ln in e.msg.splitlines():
                ln = ln.strip()
                m = re.match(r"a=(x-nv-[^:]+):(.*)$", ln)
                if m:
                    sdp.append(f"{fmt(e.t)}\tserver(DESCRIBE)\t{m.group(1)}\t{m.group(2)}")
                elif re.match(r"^[a-z]=", ln):
                    sdp.append(f"{fmt(e.t)}\tserver(DESCRIBE)\t<sdp-line>\t{ln}")
    W("sdp_server_xnv.tsv", "time\tside\tkey\tvalue\n" + "\n".join(sdp) + "\n")
    cfg = collections.OrderedDict()
    for e in ger:
        if e.tag in ("NVST:NvscClientConfig", "NVST:NvscClientConfigSerialize"):
            m = re.match(r"\s*(?:config\.)?([\w\[\]\.]+):\s*(.*)$", e.msg)
            if m:
                secret = re.search(r"(?i)encryptionKey|ice(User|Pass)|dtlsFingerprint|hmac", m.group(1))
                cfg.setdefault(m.group(1), collections.OrderedDict())[fmt(e.t)[:16]] = "<redacted>" if secret else m.group(2).strip()
    W("nvsc_config.tsv", "key\tvalues(by dump time)\n" + "\n".join(f"{k}\t{' | '.join(f'{t}={v}' for t, v in d.items())}" for k, d in cfg.items()) + "\n")

    # ---------------- app-side flow ----------------
    af = ["# CEF plugins (CxNative)"]
    seen = set()
    for e in cx:
        m = re.search(r"Creating Plugin instance (\w+) for modules: (\S+)", e.msg)
        if m and m.group(0) not in seen:
            seen.add(m.group(0))
            af.append("  " + m.group(0))
        m = re.search(r"JOIN (\S+)", e.msg)
        if m and m.group(0) not in seen:
            seen.add(m.group(0))
            af.append("  MessageBus " + m.group(0))
    af.append("# NetworkTest / SessionRequestTask (CxNative, first 20 distinct)")
    nn = 0
    for e in cx:
        if "SessionRequestTask" in e.msg:
            k = re.sub(r"\d+", "N", e.msg[30:120])
            if k in seen:
                continue
            seen.add(k)
            af.append(f"  {fmt(e.t)} {e.msg[e.msg.find('SessionRequestTask'):][:400]}")
            nn += 1
            if nn >= 20:
                break
    af.append("# IPC event counts (gameStreamClientAgent)")
    cnt = collections.Counter(re.search(r"(IPC_[A-Z_]+)", e.msg).group(1) for e in agent if "IPCMessage:" in e.msg)
    for k, v in cnt.most_common():
        af.append(f"  {v:4d} {k}")
    af.append("# IPC routes (sender -> receiver)")
    for k, v in collections.Counter(re.search(r"IPCMessage: (\S+ \S+)", e.msg).group(1) for e in agent if "IPCMessage:" in e.msg).items():
        af.append(f"  {v:4d} {k}")
    af.append("# Geronimo-side IPC/agent lines")
    for e in ger:
        if re.search(r"AgentPluginHandler|Enabled GameStream AgentPlugin|GeronimoUDSHandler\] onStreaming|UDS", e.tag + "] " + e.msg) and len(af) < 400:
            af.append(f"  {fmt(e.t)} [{e.tag}] {e.msg.splitlines()[0][:200]}")
    af.append("# JS StreamerManagerService state machine (first launch in console.log)")
    n_states = 0
    for e in con:
        if e.tag == "gfn/StreamerManagerService" and e.msg.startswith("Advancing to state"):
            af.append(f"  {fmt(e.t)} {e.msg.strip()}")
            n_states += 1
            if n_states >= 14:
                break
    af.append("# JS game-launch step durations (all launches)")
    for e in con:
        if e.tag == "gameSessionAnalyticsService" and e.msg.startswith("Game launch duration for"):
            af.append(f"  {fmt(e.t)} {e.msg.strip()}")
    W("app_flow.txt", "\n".join(af) + "\n")

    print(f"agent entries {len(agent)}, geronimo entries {len(ger)}, console entries {len(con)}, cx kept {len(cx)}")
    print(f"launches {len(sessions)}, geronimo segments {len(bounds)}, cloudmatch messages {len(cm)}, nvsc keys {len(cfg)}, server x-nv lines {len(sdp)}")
    print(f"outputs in {outdir}")


if __name__ == "__main__":
    main()
