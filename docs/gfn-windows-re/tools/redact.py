"""Redaction helpers for GeForce NOW Windows client logs.

Nothing here embeds a secret: user-specific values (Windows username, ids)
are discovered from the input at runtime and replaced, never stored.
"""
import re

SENSITIVE_KEYS = {
    "userId", "externalUserId", "idpId", "deviceId", "clientId", "sessionId", "subSessionId",
    "SubSessionId", "networkSessionId", "rtspSessionId", "streamSessionId", "streamSubSessionId",
    "streamingSessionId", "parentSessionId", "networkTestSessionId", "requestId", "xRequestId",
    "hmacKey", "accessToken", "FirebaseToken", "apiKey", "fcmMessageId", "mac", "Mac", "rigId",
    "systemInfoGuid", "gsSystemInfoGuid", "deviceHashId", "userDisplayName", "traceId", "sender_id",
    "firebaseAppId", "subscriptionId", "networkName", "NetworkName", "Fingerprint", "localId",
    "LoginId", "USER_ID", "DEVICE_ID", "serverId", "token", "Token", "idToken", "refreshToken",
    "jarvisIdpId", "email", "Email", "ssid", "SSID", "serialNumber", "app_identity_key", "spanId",
    "parentSpanId", "integrationId", "partnerSecureData", "partnerData", "sessionKey", "userAge",
    "encryptionKey", "certificate", "ufrag", "pwd", "iceUfrag", "icePwd", "fingerprint",
    "Get_Access_Token",
    "deviceName", "computerName", "hostName", "hostname", "username", "userName", "UserName",
    "accountLinkingToken", "nonce", "signature", "authorization", "Authorization", "cookie",
    "x-device-id", "nv-device-id", "subscriptionToken", "uniqueId", "machineId", "hwid",
}

_KEYS_ALT = "|".join(sorted((re.escape(k) for k in SENSITIVE_KEYS), key=len, reverse=True))

RULES = [
    (re.compile(r"eyJ[A-Za-z0-9_\-]{5,}(?:\.[A-Za-z0-9_\-]*){0,2}"), "<redacted-jwt>"),
    (re.compile(r"(?i)(GFNJWT|Bearer|GFNPartnerJWT)\s+[A-Za-z0-9_\-\.=+/]+"), r"\1 <redacted>"),
    (re.compile(r"(?i)HMAC:[^\s\"',;]+"), "HMAC:<redacted>"),
    (re.compile(r"(?i)(a=fingerprint:\S+\s+)\S+"), r"\1<redacted>"),
    (re.compile(r"(?i)(a=ice-(?:ufrag|pwd):)\S+"), r"\1<redacted>"),
    (re.compile(r"(?i)\b(sha-?(?:1|256|384|512))\s+([0-9A-F]{2}(?::[0-9A-F]{2}){8,})"), r"\1 <redacted>"),
    (re.compile(r"\b[0-9A-F]{2}(?::[0-9A-F]{2}){15,}\b", re.I), "<redacted-fp>"),
    (re.compile(r"[\w.+-]+@[\w-]+\.[\w.-]+"), "<redacted-email>"),
    (re.compile(r"(?i)([A-Z]:[\\/]{1,2}Users[\\/]{1,2})[^\\/\s\"']+"), r"\1<redacted>"),
    (re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b", re.I), "<redacted-uuid>"),
    (re.compile(r"\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\}", re.I), "<redacted-guid>"),
    (re.compile(r"\b(?:[0-9A-F]{2}[:-]){5}[0-9A-F]{2}\b", re.I), "<redacted-mac>"),
    (re.compile(r"\b\d{1,3}-\d{1,3}-\d{1,3}-\d{1,3}(?=\.)"), "<redacted-ip>"),
    (re.compile(r"\b(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}\b"), "<redacted-ip>"),
    (re.compile(r"\b(?:[0-9a-f]{1,4}:){4,7}[0-9a-f]{1,4}\b", re.I), "<redacted-ip6>"),
    (re.compile(r"\bXNV\d+\b"), "XNV<redacted>"),
    (re.compile(r'("(?:' + _KEYS_ALT + r')"\s*:\s*)("(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?)'), r'\1"<redacted>"'),
    (re.compile(r"\b(" + _KEYS_ALT + r")(\s*[=:]\s*)(?![\"{\[<])([^\s,;)\]}]+)"), r"\1\2<redacted>"),
    (re.compile(r"\b[0-9a-f]{32,}\b", re.I), "<redacted-hex>"),
    (re.compile(r"(?<![A-Za-z0-9_/.-])(?=(?:[A-Za-z_\-]*\d){5})(?=[A-Za-z0-9_\-]*[A-Za-z])[A-Za-z0-9_\-]{32,}={0,2}(?![A-Za-z0-9_/.-])"), "<redacted-token>"),
]


PRE_RULES = [
    (re.compile(r'("(?:key|name)"\s*:\s*"ot-tracer-\w+"\s*,\s*"value"\s*:\s*)"[^"]*"'), r'\1"<redacted>"'),
    (re.compile(r"(traceparent\"?\s*[:=]\s*\"?)[0-9a-f-]+"), r"\1<redacted>"),
    (re.compile(r"(?i)(HMAC seed:|\bkey:|master key/salt to)\s*[0-9A-F]{2,8}\.\.\.[0-9A-F]{2,8}"), r"\1 <redacted>"),
    (re.compile(r"(?i)\b(key ID)\s+\d+"), r"\1 <redacted>"),
    (re.compile(r"(?i)\b(encryptionKeyId|encryptionKey|iceUsernameFragment|iceUserNameFragment\w*|icePassword\w*|iceUsernamePwd|dtlsFingerprint\w*)([ \t:=\"',]{1,6})(?!<)[^\s|]+"), r"\1\2<redacted>"),
    (re.compile(r"(?i)(X-Nv-Ping-Payload:\s*)\S+"), r"\1<redacted>"),
    (re.compile(r"(?i)(STUN \(RFC 5389\) on port \d+,)\s*\S+,\s*\S+"), r"\1 <redacted>, <redacted>"),
    (re.compile(r"(?i)\b(Initial TSN from SCTP)\s+\d+"), r"\1 <redacted>"),
    (re.compile(r"(?i)(user:\s*)\S+?(?=,|\s|$)"), r"\1<redacted>"),
    (re.compile(r"(?i)\b(hmacKey|password|secret|token)(\s+)(?![=:<])\S+"), r"\1\2<redacted>"),
    (re.compile(r"(?<![A-Za-z0-9+/])(?=(?:[A-Za-z+/]*\d){5})[A-Za-z0-9+/]{32,}={0,2}(?![A-Za-z0-9+/=])"), "<redacted-b64>"),
]

_VERSION = re.compile(r"(?i)((?:\bversion\b|\bver\b|client(?:App)?Version\"?|\bVersion:|NV-Client-Version\"?,\s*\"headerValue)\W{0,6})(\d+\.\d+\.\d+\.\d+)")
_HEX16 = re.compile(r"\b[0-9a-f]{16}\b")
_UUID_LOOSE = re.compile(r"(?<![0-9a-fA-F])[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}(?![0-9a-fA-F])", re.I)


class Redactor:
    def __init__(self):
        self.extra = []

    def learn(self, text):
        """Discover user-specific literals (Windows user name) from the input itself."""
        for m in re.finditer(r"(?i)[A-Z]:[\\/]{1,2}Users[\\/]{1,2}([^\\/\s\"']+)", text):
            name = m.group(1)
            if name.lower() not in {"public", "default", "<redacted>", "all users"} and len(name) >= 3:
                pat = re.compile(r"(?i)\b" + re.escape(name) + r"\b")
                if all(p.pattern != pat.pattern for p in self.extra):
                    self.extra.append(pat)

    def __call__(self, s):
        for rx, rep in PRE_RULES:
            s = rx.sub(rep, s)
        versions = []

        def keep(m):
            versions.append(m.group(2))
            return f"{m.group(1)}\x00V{len(versions) - 1}\x00"

        s = _VERSION.sub(keep, s)
        s = _UUID_LOOSE.sub("<redacted-uuid>", s)
        for rx, rep in RULES:
            s = rx.sub(rep, s)
        s = _HEX16.sub("<redacted-hex>", s)
        s = re.sub(r"\x00V(\d+)\x00", lambda m: versions[int(m.group(1))], s)
        for pat in self.extra:
            s = pat.sub("<redacted-user>", s)
        return s
