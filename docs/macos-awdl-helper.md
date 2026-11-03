# macOS AWDL helper

OpenNOW can keep `awdl0` down during streaming through an optional root LaunchDaemon.
The setting is off by default. Installing the helper requires administrator approval,
and installing it does not itself start suppression. Setup and streaming remain
available when the helper is absent or fails.

AWDL supports AirDrop and other Continuity features. Suppression can interrupt
AirDrop, AirPlay, Sidecar, and Universal Control for every user on the Mac.
Keeping the interface down does not prove that all AWDL radio traffic has stopped,
and does not guarantee lower latency. Compare streaming with the setting off and on.

## Installation contract

The application uses an explicit administrator-approved install, not `SMAppService`
or `SMJobBless`. This supports ad-hoc builds as well as Developer ID releases.
It also means macOS does not manage the helper's version or uninstall it with the app.
The client snapshots the bundled helper and captures its SHA-256 before requesting
approval. The privileged install copies that snapshot into root-owned staging,
checks the captured hash, and generates a fixed plist for the system paths below.
It registers the plist with `launchctl bootstrap system`.

| Item | Path or value |
| --- | --- |
| Cargo package and binary | `opennow-awdl-helper` |
| Bundled executable | `OpenNOW.app/Contents/Helpers/opennow-awdl-helper` |
| Bundled LaunchDaemon plist | `OpenNOW.app/Contents/Resources/opennow-awdl.plist` |
| Installed executable | `/Library/PrivilegedHelperTools/io.github.opencloudgaming.OpenNOW.awdl-helper` |
| Installed plist | `/Library/LaunchDaemons/io.github.opencloudgaming.OpenNOW.awdl.plist` |
| LaunchDaemon label | `io.github.opencloudgaming.OpenNOW.awdl` |
| Socket | `/var/run/opennow-awdl/control.sock` |
| Recovery journal | `/var/db/opennow-awdl/restore` |

The installed executable is `root:wheel`, mode `0755`. The installed plist is
`root:wheel`, mode `0644`. Its only `ProgramArguments` entry is the executable path.
The daemon rejects arguments and requires root. `RunAtLoad` and `KeepAlive` start
the service at boot and restart it after failure, including when recovery is pending.
The service remains idle without an authorized lease.

The app can be user-owned, including a Finder drag-and-drop installation.
Hash verification prevents a changed snapshot from being installed after approval.
It does not authenticate the app's publisher. The administrator approves the helper
bytes selected by the running app, so install only from a copy of OpenNOW you trust.
The installed paths and their parent directories must remain root-owned, non-symlink,
and non-group-writable and non-world-writable. The installer rejects unsafe destinations.
It uses fixed system command paths and starts its installer shell with an empty
environment and a fixed `PATH`, rather than inheriting shell or Perl startup overrides.

The staging copy is capped at 32 MiB and must match the captured size and hash.
The fixed copy program uses the system Perl, which is also used by the system
SHA-256 tool. It opens the snapshot with `O_NOFOLLOW` and `O_NONBLOCK`, verifies
that the opened descriptor is a regular file, and has a five-second copy deadline.
A same-user process replacing the snapshot with a symlink or FIFO cannot redirect
or block the copy. Content changes still fail size or hash verification.

In **Settings → Network**, select **Install helper…**, approve the administrator
prompt, then enable **Suppress AWDL while streaming**. Cancelling the prompt leaves
suppression unavailable without preventing a session. Installation and removal are
explicit client actions. An app update does not silently replace the installed
privileged executable. End streaming, restore the interface, then explicitly reinstall
the helper from the trusted updated bundle.

## Signing and build contract

`NativeRuntime.cmake` builds only the macOS helper package from the streamer Cargo
workspace. It uses the configured Qt architecture and Qt configuration's Cargo
profile, rather than the streamer's always-Release profile. The crate needs only
the standard library and `libc`; the installed executable must not depend on Qt,
SDL, FFmpeg, the app bundle, or a development library path.

The helper has signing identifier `io.github.opencloudgaming.OpenNOW.awdl-helper`.
The ad-hoc packaging path signs the helper before sealing the outer app and runs
`codesign --verify --deep --strict`. Ad-hoc signatures protect bundle integrity but
do not identify a publisher or provide notarization. Administrator approval is the
trust decision for those builds.

`macos_release.py` discovers nested Mach-O files, verifies the selected ARM64 or x86_64 architecture,
and signs them inside-out with Developer ID, hardened runtime, and a timestamp.
The helper receives no app entitlements. The existing JIT and microphone entitlements
remain limited to the outer application. No `SMPrivilegedExecutables` declaration,
service-management entitlement, or sandbox exception is added to `Info.plist`.
The signed application and DMG follow the existing notarization and stapling flow.

## Lease and recovery behavior

The Qt platform controller acquires a lease after the native runtime first reports
streaming. The native lifecycle sender carries the originating start-request ID,
so queued events from an earlier session cannot acquire or release the current
session's lease. Paused, connecting, and ready states within that session do not
release it. Accepted stop or replacement commands, terminal errors, and runtime
shutdown do release it. Local overlays and focus changes do not change this lifetime.

The helper writes and syncs
the recovery journal before changing an originally-up interface to down. It observes
route changes and periodically checks the interface so macOS re-enabling `awdl0`
does not end suppression while a lease is active.

Explicit release, socket disconnect, app crash, and console-user changes remove
leases. The last lease restores an interface that the helper originally found up.
If `awdl0` was already down, release leaves it down. Multiple clients share ownership,
so one client ending its session does not restore the interface under another lease.

On normal service shutdown, the helper attempts restoration. On a crash or reboot,
launchd restarts it and the helper processes the journal before accepting new leases.
A failed restoration keeps the journal for another attempt. An unreadable or unsafe
journal is an error, not permission to assume the interface was originally down.

Uninstall must release leases and confirm recovery before removing the service.
If restoration fails, retain the executable, plist, and journal so launchd can retry.
Never delete `/var/db/opennow-awdl/restore` to dismiss an error. An app removal does
not remove these system files, so select **Remove helper…** in **Settings → Network** first.

## Security boundary

The daemon uses a local Unix socket, not a network listener. The runtime directory
is root-owned, mode `0755`; the socket is root-owned, mode `0666`. The journal
directory is root-owned, mode `0700`, and the journal is mode `0600`.
The client validates the socket's root ownership. The daemon checks peer credentials
against the current non-root console user and rejects other users, including root clients.

The protocol accepts one-byte `A` acquire and `R` release requests, answered with
`+` or `-`. The daemon allows at most 32 connections and one lease per connection,
checks route notifications, and uses a one-second idle poll timeout. It accepts no command,
interface name, executable, path, or shell argument. The root process changes only
the `IFF_UP` bit on the fixed `awdl0` interface. It does not process account credentials
or streaming packets.

Peer-user authentication is not application authentication. Another process running
as the current console user can request suppression. This can disrupt Continuity,
but does not provide an arbitrary root command. Ad-hoc support makes a stable
Developer ID client requirement unavailable. Treat the installed helper as a
user-approved system-wide capability, not as a private channel available only to OpenNOW.

## Diagnose recovery

End every OpenNOW session before troubleshooting. Inspect the daemon and interface:

```sh
	sudo /bin/launchctl print system/io.github.opencloudgaming.OpenNOW.awdl
	/sbin/ifconfig awdl0
	sudo /usr/bin/stat -f '%Su:%Sg %Lp %N' /var/db/opennow-awdl/restore
```

The journal should be absent after successful recovery. A missing `awdl0` interface
is not proof that a pending restoration succeeded. If a valid journal remains,
restart the installed service so it can retry recovery before taking new leases:

```sh
	sudo /bin/launchctl kickstart -k system/io.github.opencloudgaming.OpenNOW.awdl
```

If recovery still fails, keep the journal and the installed service. Do not run a
blanket `ifconfig awdl0 up` command on a Mac where the interface was already down.
Manual restoration needs an administrator to confirm the saved state and the cause
of failure. After restoration succeeds, retry uninstall through OpenNOW.

See [macOS AWDL acceptance](qt-acceptance.md#macos-awdl-helper) for the real-device
checks required before shipping this helper.
