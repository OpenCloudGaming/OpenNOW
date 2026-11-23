# OpenNOW-VN — upstream / fork record

## Fork base

- **Upstream:** `OpenCloudGaming/OpenNOW`, tag **v1.0.4**, commit **`bec4a0b`**
  (`Merge pull request #1433 from OpenCloudGaming/dev`).
- **Qt app version:** `1.0.4` (`opennow-qt/CMakeLists.txt:10`,
  `project(OpenNOWQt VERSION 1.0.4 ...)`). Note the Rust core crate version
  in `native/opennow-core/Cargo.toml` is `1.0.0`; the build-time
  `OPENNOW_BUILD_VERSION` overrides it, so the shipped version is 1.0.4.
- **Fork branch:** `vn-translation` (local branch in this clone; origin
  remote is the fork — see below).
- **What the fork adds:** in-client EN→VI screen-OCR translation for
  Wuthering Waves, implemented as an additive module on top of upstream
  (see `PROGRESS.md`). No upstream behaviour is changed except through the
  3 shim files listed below.

## Remotes

| Remote | URL | Purpose |
| --- | --- | --- |
| `origin` | `https://github.com/duyzoz/OpenNOW-ORC.git` | **The VN fork.** All VN work pushes here, branch `vn-translation`. |
| `upstream` | `https://github.com/OpenCloudGaming/OpenNOW.git` | Upstream source of truth. Fetch-only for merge purposes. **Never push to `upstream`.** |

Verify at any time with `git remote -v`. If a remote points anywhere else,
stop and fix it before fetching or pushing.

## Merge rules

1. **Update from upstream:** `git fetch upstream`, then merge upstream's
   `main` (or the tagged release) **into `vn-translation`**:
   `git checkout vn-translation && git merge upstream/main`.
   Never merge `vn-translation` into a local `main`, and never fast-forward
   `main` to upstream — `main` in this clone is upstream's history, not the
   fork's working branch.
2. **Conflicts are only allowed in the 3 shim files.** If a merge conflicts
   anywhere else, stop and resolve by keeping the VN side minimal — do not
   rewrite upstream files to fit the fork. The 3 shim files are the only
   places VN code is allowed to touch upstream-owned code:

   | Shim file | Why it is touched |
   | --- | --- |
   | `opennow-qt/src/app/ApplicationStartup.cpp` | Wires the translation service into app startup / QML context. |
   | `opennow-qt/qml/desktop/stream/DesktopStreamScreen.qml` | Hosts the translation overlay above `StreamVideoItem`; never touches the Rust streamer / NVST transport / FFI ABI. |
   | `opennow-qt/qml/desktop/settings/DesktopSettingsScreen.qml` | Adds the Translation settings entry/page. |

   Everything else VN is **additive**: new files under
   `opennow-qt/src/translation/` and new QML files (registered in
   `opennow-qt/cmake/Sources.cmake` / `opennow-qt/cmake/QmlModule.cmake` —
   see `CI.md` §3), plus `docs/vn-translation/` and the README section.
3. **Never edit `native/`.** That means no changes under
   `native/opennow-core/` (settings, updater, session orchestration) or
   `native/opennow-streamer/` (NVST transport, decode, FFI). The updater
   repoint (upstream → `duyzoz/OpenNOW-ORC` URLs + a new VN signing key) lives
   in `native/opennow-core/src/updater.rs` and is therefore **explicitly
   out of scope** until a separate decision is made — see `UPDATER.md` §4
   step 3. The QML-level updater guard (step 2) is the approved interim.
4. **Never rebase `main` history.** Upstream history is immutable. Rebase
   or history rewrites are only acceptable on the unpushed `vn-translation`
   branch itself, and only before it is shared; once pushed, merge forward.
5. **Upstream updates are preserved via the upstream remote.** Keep
   `upstream` configured and fetch before each merge. VN code must stay
   additive so an upstream merge is a clean fast-forward except for the 3
   shims. If an upstream change breaks the translation module, fix the
   module — do not fork the upstream file.

## Abandoned repo — do not merge

**`duyzoz/OpenNow-VN` (the old Electron remake) is ABANDONED, completely.**
Do not port features from it, do not merge it, do not copy architecture
from it, and do not link to it as the live project. The repo's own
`AGENTS.md` agrees: Electron implementations are legacy/out of scope.
The Qt fork (`duyzoz/OpenNOW-ORC`, branch `vn-translation`) is the only live
remake. If the old repo is ever deleted, that is a separate explicit step
by Ethan — it is not part of this fork's work.
