# Providers and alliance partners on Windows (GFN 2.0.89.141)

Short answer: the supplied Windows logs contain only NVIDIA-operated (NIMBUS) sessions. Nothing in them shows how a partner (alliance) session differs on the wire. The Windows binaries do expose the partner hooks (`partnerCustomData`, `partner.nvgs.nvidia.com`, `NVB_SERVER_PASSTHRU`), and they let us fill in the numeric server-type map that the macOS research left open.

Tags: **S** static, **D** disassembled (GeForceNOW.exe base 0x140000000, Geronimo.dll base 0x180000000), **L** log, **I** inferred, **U** unknown. Identifiers in logs are `<redacted>`.

## 1. What the Windows logs show

- Every session is NVIDIA-hosted: console.log `streamingService prepare params` has `"serverType": 1` (10×, **L**); debug.log `Pre-NVb-conversion ServerType : 1` (**L**); Bifrost `Converted Nvb server type 0 to nvsc Server type 2` (geronimo.log.bak 19:45:11, **L**).
- Login provider: console.log `providerCode` is "NVIDIA" in every occurrence (16×, **L**). `idpId` appears 126× (values redacted; some are the literal string "undefined" before login, **L**).
- No partner payload: console.log `streamingService No partnerData and partnerSecureData present` (5×, e.g. 2026-10-12 20:31:56.948, **L**). The CloudMatch request has `partnerCustomData` null and `accountLinked` false (geronimo.log.bak 19:45:11, **L**).
- Zone selection is in the JS layer: `defaultZone` (prod.cloudmatchbeta.nvidiagrid.net), `zoneName`, `zoneOverride`, `remoteZoneOverride`, `overrideZone`, `regionIds`, `isAffiliateRegion`, `supportedProviders`, `AuthenticationProvider`, `registrationProvider` are key names in console.log (**L**; values not copied). The session ran in zone eu-netherlands-north, VPC NP-AMS-08.
- CloudMatch shape for this NVIDIA seat: JSON `sessionControlInfo` usage 10 on 443, JSON `connectionInfo` usage 14 (SIGNALING, `appLevelProtocol` 6) on 322 and 48322, no usage 2/16/17 (handshake.md §3, **L**). Native RTSPS is a different enum value, 10.

## 2. Partner hooks in the binaries

| Hook | Where | Evidence |
|---|---|---|
| `partnerCustomData` read from the JS prepare/start query | GeForceNOW.exe literal 0x1402a1a18, used by query handler 0x1401a69c0 (same function reads `address`, `serverType`, `deviceId`, `token`, `tokenType`, `clientHeaders{headerName,headerValue}`, `appId`, `clientAppVersion`) | **S/D** |
| `partner.nvgs.nvidia.com` | GeForceNOW.exe literal 0x140290570, referenced from the startup function containing 0x140082d0d; the JS config also has the pattern `https://(accounts\|partner).nvgs.nvidia.com` | **S**, **L** (console.log). Use as an allowed web origin is **I**. |
| `NVB_SERVER_PASSTHRU` | GeForceNOW.exe 0x1401a2c00: JS `serverType` 1001 → Bifrost 0x33 | **D** |
| `updateUserInfo: userId=%s, externalUserId=%s, idpId=%s` | Geronimo.dll string | **S** |
| Proxy switches `nv-proxy-server`, `nv-proxy-bypass-list`, `nv-proxy-pac-url` (and CEF `proxy-*`) | GeForceNOW.exe strings | **S**; same set as macOS |
| `QUERY_GFN_START`, `QUERY_GFN_RESUME`, `QUERY_GFN_PREPARE` | GeForceNOW.exe query list from 0x14028d820 | **S** |

The matching Windows Bifrost now supplies native auth and endpoint conversion evidence in [cloudmatch-native.md](cloudmatch-native.md). Negative searches in the shell and Geronimo did not establish the absence of native partner support. The supplied logs still contain no partner session, so static support must not be treated as an exercised partner flow.

## 3. Server types (Windows fills a macOS gap)

GeForceNOW.exe 0x1401a2c00 (**D**):

| JS `serverType` | Bifrost value | Name |
|---|---|---|
| 1 | 0 | NVB_SERVER_NIMBUS |
| 2 | 1 | NVB_SERVER_GAMESTREAM |
| 3 | 2 | NVB_SERVER_ROAMING_PROXY |
| 4 | 3 | NVB_SERVER_CLOUD_STATION |
| 5 | 4 | NVB_SERVER_QUADRO_STATION |
| 1001 | 0x33 | NVB_SERVER_PASSTHRU |
| 0, 6, other | -1 | NVB_SERVER_UNKNOWN |

macOS providers.md lists the same names but says the numeric inputs were not recovered. Bifrost then maps NIMBUS (0) to NVSC server type 2 (**L**). Which type a partner seat uses is **U**: no partner log exists. Partner seats returning NIMBUS with different endpoints, or a distinct type such as PASSTHRU, are both possible (**U**).

## 4. Hosts seen in the logs

Counts over console.log(+.bak), CxNative, debug.log(+.bak) and geronimo.log(+.bak) (**L**; per-seat IP hostnames counted as `<redacted-ip>.cloudmatchbeta.nvidiagrid.net`):

| Host | Count | Role (from surrounding log lines) |
|---|---|---|
| eu-netherlands-north.cloudmatchbeta.nvidiagrid.net | 403 | zone CloudMatch endpoint (`prepare params` `address`) |
| prod.cloudmatchbeta.nvidiagrid.net | 164 | `defaultZone` |
| `<redacted-ip>`.cloudmatchbeta.nvidiagrid.net | 68 | seat RTSPS endpoints (322 / 48322) |
| np-ams-08.cloudmatchbeta.nvidiagrid.net | 21 | `sessionControlInfo` host (`/v2/session/<redacted>`) |
| pcs.geforcenow.com | 7 | service URLs (OpenNOW uses `/v1/serviceUrls` here too) |
| login.nvidia.com, static-login.nvidia.com | 7, 2 | login |
| accounts.nvgs.nvidia.com | 2 | account / partner origin pattern |
| mes, uds, pns, als, uas, play, developer .geforcenow.com | 45, 8, 3, 2, 1, 11, 38 | GFN web services (roles beyond the name are **U**) |
| gx-target-rconfig-frontend-api.gx.nvidia.com | 3 | GXT remote config (configuration-fields.md §4a) |
| events.telemetry.data.nvidia.com | 4 | telemetry |

## 5. Comparison with macOS and OpenNOW

- Provider identity is separate from endpoint selection. The shell passes `partnerCustomData`, address and server type. The native conversion now has independent Windows evidence; no partner-session log establishes which combination an alliance seat uses.
- OpenNOW `native/opennow-core/src/cloudmatch.rs` always sends `"partnerCustomData": null` (~1717), like this Windows session. It routes alliance seats by `allianceWebrtcCompatibility` (~37, ~372, ~1725) and by `connectionInfo` usage 14/16 entries with `wss://`/`https://` resource paths (~1775–1800). Windows NVIDIA seats return usage 14 with `rtsps://` and `appLevelProtocol` 6 instead, which OpenNOW's media selection deliberately skips. Neither the alliance WebRTC path nor partner usage 16/17 can be checked against Windows evidence.
- OpenNOW keeps the provider's `idpId` per identity (`main.rs` ~473, ~1343). Windows Geronimo only logs it (`updateUserInfo`); no Windows code path was found that changes transport by provider (**I**, negative).
- OpenNOW has no equivalent of the server-type field; it does not need one for NIMBUS seats. If partner seats turn out to use PASSTHRU (1001→0x33), this map is where to look (**U**).
