# Vendor Bundle Human Summary

## What This Folder Is

This is a production-built GeForce NOW web application bundle. It is not normal source code.

The files in `/Users/jayian/Downloads/vendor` are minified JavaScript, CSS, and PWA metadata emitted by a Webpack build. The app identifies itself as `gfn_mall`, and the extracted structure strongly indicates an Angular application using Angular Material, RxJS, Zone.js, and Webpack code splitting.

## What To Look At First

Read these in this order:

1. `VENDOR_HUMAN_SUMMARY.md`
2. `VENDOR_INDEX.md`
3. `VENDOR_REPORT.md`
4. `VENDOR_FEATURES.md`

Only use the `VENDOR_STRINGS_*.md` files when searching for exact strings or identifiers. They are intentionally noisy because they preserve extracted minified bundle strings.

## Most Important Files

| File | What It Is | Why It Matters |
| --- | --- | --- |
| `main.81a2d6165286131d.js` | Main application logic bundle | Best place to search for GeForce NOW behavior, routes, services, auth, streaming, catalog, account, telemetry, and UI logic. |
| `vendor.0087fab5da9f1091.js` | Third-party/library bundle | Contains framework/library/runtime code plus some shared SDK-style logic. Usually noisier than `main`. |
| `runtime.de302b1b971bfb57.js` | Webpack runtime | Maps lazy-loaded chunk IDs to chunk filenames. Useful for understanding what files are dynamically loaded. |
| `styles.b163082243582a97.css` | Global CSS bundle | Useful for UI class names, Angular Material styling, layout rules, and design tokens/classes. |
| `manifest.webmanifest` | PWA manifest | Confirms the app name, icon set, launch behavior, and web app metadata. |

## What The App Appears To Include

The bundle contains evidence of these feature areas:

| Area | Evidence In Extraction |
| --- | --- |
| Authentication and identity | OAuth, login, logout, auth tokens, client tokens, user/account identifiers, account menu, linked accounts. |
| GeForce NOW streaming | Session start/resume/stop, CloudMatch, server info, queue/session handling, stream profiles, bitrate/FPS/resolution settings. |
| WebRTC/realtime networking | SDP, ICE candidates, signaling, latency/network tests, connection and resume handling. |
| Input and media | Microphone, keyboard layout, mouse/gamepad/touch references, audio/video/codec controls. |
| Store/library/catalog | Game catalog, library, ownership, platform/store linking, entitlement/subscription related strings. |
| Telemetry/logging | NVIDIA telemetry endpoints, event names, launch timing, error/exception tracking, GDPR consent fields. |
| Settings/preferences | Streaming profiles, language/locale, quality, resolution, color quality, HDR/L4S/Reflex/VRR/G-Sync related strings. |

## Useful Searches

Use the viewer search box or search within the Markdown/JSON files for these terms:

| Goal | Search Terms |
| --- | --- |
| Login/session persistence | `client_token`, `Get_Client_Token`, `GetOAuthURL`, `SetAuthToken`, `authToken`, `idpId` |
| OAuth flow | `OAuth`, `connectOauth`, `JARVIS_Get_Login_Token`, `JARVIS_Get_Session_Token` |
| Stream launch | `cloudmatch`, `Start`, `Resume`, `session`, `serverInfo`, `GetActiveSessions` |
| WebRTC details | `webrtc`, `sdp`, `ice`, `candidate`, `signaling` |
| Quality settings | `streamingProfiles`, `resolution`, `fps`, `bitrate`, `codec`, `colorQuality` |
| Account linking | `accountConnection`, `LinkAccount`, `AccountLinked`, `ownership` |
| Telemetry | `telemetry`, `events.telemetry.data.nvidia.com`, `Ragnarok`, `LaunchTime` |

## Important Domains Found

The extraction found references to these notable domains:

| Domain | Likely Purpose |
| --- | --- |
| `prod.cloudmatchbeta.nvidiagrid.net` | GeForce NOW CloudMatch/session backend. |
| `api.gdn.nvidia.com` | NVIDIA API/content/service endpoint. |
| `events.telemetry.data.nvidia.com` | Production telemetry event collection. |
| `feedbacks.telemetry.data.nvidia.com` | Production telemetry feedback collection. |
| `telemetry.gfe.nvidia.com` | GeForce Experience/GeForce telemetry endpoint. |
| `prod.otel.kaizen.nvidia.com` | OpenTelemetry/observability endpoint. |
| `www.nvidia.com` | NVIDIA public web content/help/account links. |
| `nvidia.custhelp.com` | NVIDIA customer support/help links. |

## How To Use The Viewer

Open `index.html` in this folder.

Use the left sidebar to pick files. The viewer has:

| Control | Purpose |
| --- | --- |
| Search box | Searches filenames and embedded file contents. |
| Reports filter | Shows summary/report Markdown files. |
| Strings filter | Shows exhaustive extracted string catalogs. |
| JSON filter | Shows the full structured extraction JSON. |
| Rendered button | Shows Markdown in a more readable format. |
| Raw button | Shows exact file text. |
| Copy button | Copies the current embedded document content. |
| Open file | Opens the underlying Markdown/JSON file directly. |

## What Not To Expect

This extraction does not reconstruct clean TypeScript source files.

The source was already minified and bundled before scanning. Names may be shortened, code order is bundle-oriented rather than source-oriented, and some extracted strings are fragments of compiled JavaScript. Treat this as a map for investigation, not as original source.

## Best Next Step

If you are trying to copy behavior into OpenNOW, start with `VENDOR_FEATURES.md` and search for the exact behavior area. Then use `vendor_extraction.json` or the raw string files only when you need deeper evidence.

For auth/session persistence specifically, start with:

1. `client_token`
2. `Get_Client_Token`
3. `SetAuthToken`
4. `processUpdatedAuthToken`
5. `JARVIS_Get_Session_Token`
