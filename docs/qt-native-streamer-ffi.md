# Qt native streamer lifecycle events

ABI version 13 delivers these events through the generic JSON event callback
without a new C ABI layout.
The native streamer owns wire parsing and termination classification. Qt does not
infer resumability from a wire code or reason.

## Session timers

```json
{"type":"session-timer","startId":"start-42","timerType":4,"secondsLeft":60,"active":true}
```

`timerType` is 4 or 5, `secondsLeft` is an unsigned 32-bit integer, and `active`
is a boolean. `NativeStreamRuntime` validates these fields and emits
`sessionTimerReceived(quint8, quint32, bool)` on the Qt thread. Malformed timer
events do not reach QML. `startId` must match the current start-command generation.
Timer events from a replaced or stopped session, or a previous runtime instance,
are discarded.

`ShellStore.acceptSessionTimer` owns warning state for the current stream. Type 4
with `active:true` starts a display-only countdown from the server-provided seconds.
The countdown clamps at zero and never clears the warning or ends the session.
Type 4 with `active:false` clears the warning after the transport confirms successful input. Type 5 clears
the warning. Qt neither clears on local input submission nor ends a session when
a timer arrives. Session end, replacement, native restart, and terminal native
status also clear the warning.

Both desktop and console compose `StreamIdleWarning` above the same
`StreamVideoItem`. It has no input handlers, focus requests, or overlay route, so
it does not pause gameplay input or recreate the session. A refreshed server timer
resets the display-only countdown.

## Server termination

Terminal error and status events can include this object:

```json
{"source":"nvst-server","code":"server-terminated","startId":"start-42","wireCode":2147680291,"shutdownReason":260,"reason":56,"sessionAlive":false,"extendedCode":2147680263,"resumable":false,"disposition":"idle"}
```

`wireCode` is the raw result from command `0x109`. For the extended teardown
envelope `0x80030023`, the transport walks the big-endian shutdown records that
follow it: type 0 is a tag and a `SessionTerminationReason`, and type 1 is a tag,
a blob length, and a skipped blob. The first type-0 record supplies
`shutdownReason`. An unknown record type stops the walk because its length is
unknown, so it hides later records; a truncated record or overlong blob also ends
the walk. Without a type-0 record `shutdownReason` is null. `reason` is the
derived notification reason, `sessionAlive` the native liveness flag, and
`extendedCode` the translated result, following the official client's
`onStreamerStopped` selection and `SessionTerminationReasonToNvstResult` table.
A missing reason yields `0x35`/not alive/`0xC0F52003` and an unlisted reason
`0x35`/not alive/the table result, `0x80030000` when unmapped. Other wire codes
pass through as `extendedCode` with `sessionAlive:true`. These fields are always
present and `resumable` is always a boolean for `nvst-server`. `disposition` is
`normal` for notification `0x34` (an intended stop), `idle` for `0x38`,
`time-limit` for `0x39`, and `error` otherwise.

When a nonresumable server termination ends the session, `ShellStore` sets
`streamEndNotice` to `{kind, code, codeText}` for every disposition except
`normal`. `code` is the u32 `extendedCode` and `codeText` its `0xXXXXXXXX` form. A
user quit, an intended server stop, and a CloudMatch-only ending leave it null.
`dismissStreamEndNotice()` and the next launch clear it. The evidence is
`notes/windows-re/visible-error-codes.md` in the reverse-engineering corpus.
The native streamer retains the numeric values through the existing JSON callback.
Qt rejects server termination events whose `startId` does not match the current
start-command generation, before invalidating input or presentation. The matching
error and stopped pair can both arrive after presentation has been invalidated.
Qt treats `nvst-server` with `code:"server-terminated"` and `resumable:false` as an
authoritative session end, cancels pending recovery, and never auto-resumes that
session. It retains the server message in `lastError` and the accessibility
announcement, and the full termination object in the streamer snapshot.
True or unknown classification follows the existing bounded
CloudMatch reconciliation path. Existing authoritative CloudMatch status 7 and
HTTP 404 handling remains unchanged.

The evidence baseline is `reports/opennow-gfn-parity/audio-input-lifecycle.md`,
rows L3 through L5, and Windows `transport-feedback.md`, section 8. Offline Qt
callback, orchestration, and smoke tests cover delivery and passive composition;
they do not connect to GFN or execute GFN binaries.

## Text retry feedback

`text-input-error` carries the current `startId` and a fixed `reason`, with no
paste contents. C++ rejects stale generations before forwarding it through the
existing `eventReceived` signal. ShellStore announces localized retry feedback,
without changing streamer state, input ownership, error telemetry, or incidents.
Desktop and console display it for five seconds in the existing passive input
notice, including when the Guide is closed. The normal stream video item and
gameplay focus remain unchanged. Temporary SCTP deferral queues one bounded paste
behind older input; only rejected local paste admission needs this notice.
