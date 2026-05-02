# IPC inventory (OpenNOW stable)

Cross-layer map: [`src/shared/ipc.ts`](../src/shared/ipc.ts) defines channel strings; main registers `ipcMain.handle`; preload exposes `window.openNow`; renderer calls `window.openNow.*`.

## Classification legend

- **E2E**: Declared in `IPC_CHANNELS`, handler in main, exposed (if applicable) in preload, used from renderer.
- **MainSend**: Main sends to renderer (`webContents.send`); renderer listens via preload subscription.
- **LegacyString**: Preload uses a string literal not listed in `IPC_CHANNELS` (should align over time).
- **Removed**: Intentionally dropped from the contract.

## Channels

| Channel | Class | Preload API | Notes |
|---------|-------|-------------|--------|
| `auth:get-session` | E2E | `getAuthSession` | |
| `auth:get-providers` | E2E | `getLoginProviders` | |
| `auth:get-regions` | E2E | `getRegions` | |
| `auth:login` | E2E | `login` | |
| `auth:logout` | E2E | `logout` | |
| `auth:logout-all` | E2E | `logoutAll` | |
| `auth:get-saved-accounts` | E2E | `getSavedAccounts` | |
| `auth:switch-account` | E2E | `switchAccount` | |
| `auth:remove-account` | E2E | `removeAccount` | |
| `gfn:ping-regions` | E2E | `pingRegions` | |
| `subscription:fetch` | E2E | `fetchSubscription` | |
| `games:fetch-main` | E2E | `fetchMainGames` | |
| `games:fetch-library` | E2E | `fetchLibraryGames` | |
| `games:browse-catalog` | E2E | `browseCatalog` | |
| `games:fetch-public` | E2E | `fetchPublicGames` | |
| `games:resolve-launch-id` | E2E | `resolveLaunchAppId` | |
| `gfn:create-session` | E2E | `createSession` | |
| `gfn:poll-session` | E2E | `pollSession` | |
| `gfn:report-session-ad` | E2E | `reportSessionAd` | |
| `gfn:stop-session` | E2E | `stopSession` | |
| `gfn:get-active-sessions` | E2E | `getActiveSessions` | |
| `gfn:claim-session` | E2E | `claimSession` | |
| `gfn:session-conflict-dialog` | E2E | `showSessionConflictDialog` | |
| `gfn:connect-signaling` | E2E | `connectSignaling` | |
| `gfn:disconnect-signaling` | E2E | `disconnectSignaling` | |
| `gfn:send-answer` | E2E | `sendAnswer` | |
| `gfn:send-ice-candidate` | E2E | `sendIceCandidate` | |
| `gfn:request-keyframe` | E2E | `requestKeyframe` | |
| `gfn:signaling-event` | MainSend | `onSignalingEvent` | |
| `window:toggle-fullscreen` | E2E | `toggleFullscreen` | |
| `window:set-fullscreen` | E2E | `setFullscreen` | |
| `window:toggle-pointer-lock` | E2E | `togglePointerLock` | |
| `app:quit` | E2E | `quitApp` | |
| `app-updater:*` | E2E / MainSend | `getUpdaterState`, etc. / `onUpdaterStateChanged` | |
| `settings:*` | E2E | `getSettings`, `setSetting`, `resetSettings` | |
| `microphone:permission:get` | E2E | `getMicrophonePermission` | |
| `logs:export` | E2E | `exportLogs` | |
| `screenshot:*` | E2E | `saveScreenshot`, etc. | |
| `recording:*` | E2E | `beginRecording`, etc. | |
| `cache:refresh-manual` | E2E | `refreshCache` | Refreshes scheduler-backed cache without wiping |
| `cache:status-update` | MainSend | (internal) | Sent on refresh events |
| `cache:delete-all` | E2E | `deleteCache` | |
| `community:get-thanks` | E2E | `getThanksData` | |
| `media:*` | E2E | `listMediaByGame`, etc. | |
| `printedwaste:*` | E2E | `fetchPrintedWasteQueue`, `fetchPrintedWasteServerMapping` | |
| `discord:clear-activity` | E2E | `clearDiscordActivity` | |

## Legacy string events (preload)

| Event | Class | Notes |
|-------|-------|--------|
| `app:toggle-fullscreen` | LegacyString | Main should use `IPC_CHANNELS` constant; renderer: `onToggleFullscreen` |
| `app:toggle-pointer-lock` | LegacyString | Sent from main on pointer-lock IPC |
| `app:trigger-screenshot` | **Removed** | Was never sent from main; API removed |

## Removed / cleaned

- `logs:get-renderer` — **Removed** from `IPC_CHANNELS` (no handler or caller).
