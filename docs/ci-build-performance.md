# CI build performance

The Qt workflows reuse complete Cargo target directories, including vendored native
libraries, build-script outputs, and Cargo fingerprints. They do not remove local
crates before saving the cache. The peer-probe target is cached separately from the
embedded streamer, matching its CMake ownership.

Linux cache hydration also restores a tracked native source file's earlier
timestamp when its bytes and permissions are unchanged. Changed and new files
retain fresh timestamps. This avoids recompiling unchanged vendored C sources
after every checkout without hiding source changes from Cargo or CMake.

## Persistent caches

Linux jobs use one Blacksmith Sticky Disk for Cargo registry downloads, Git
dependencies, and the target directories listed by each caller of
`.github/actions/rust-build-cache`. The cache does not include Cargo credentials,
configuration, installed tools, or release signing material.

Cache identities separate operating systems, architectures, caller namespaces,
Rust and native toolchains, and ABI settings. Cargo still checks source changes,
features, profiles, and dependency versions. A cache hit is not permission to skip
a build or a test.

The committed vendor tree is also part of the cache identity. A vendored FFmpeg
recipe or str0m source change selects a new snapshot, so native sentinel files
and the cached peer-probe executable cannot bypass that change. Ordinary
application source edits keep the same compatible cache.

The current Blacksmith Sticky Disk action requires Linux block-device and ext4
tools. Windows and macOS therefore restore complete target archives. Their keys
include the source commit, with compatible environment and lockfile fallbacks.
Nightly packages and release candidates retain matching cache namespaces.

Flatpak uses a separate Sticky Disk for the user SDK installation, vendored source
downloads, and flatpak-builder state. Its build output and state directories stay
on the same filesystem, as flatpak-builder requires. SDK updates still run, and
the application is built offline and verified before its bundle is uploaded.

## Snapshot trust

Keep Sticky Disk Branch Protection enabled in Blacksmith. Client-side checks are
not a security boundary against a pull request that edits the workflow itself.
The Blacksmith host policy must prevent untrusted jobs from publishing snapshots.

The workflow additionally requests writes only for push or manual runs on `dev`
or `main`. Rust cache writers must have the triggering commit checked out.
Pull requests and manual runs on other branches are read-only consumers. Failed
or cancelled Sticky Disk jobs retain the previous snapshot instead of saving
partial state.

Blacksmith currently allows protected snapshots to be committed only from the
repository's default branch. If the default is `main`, runs on `dev` cannot warm
protected disks, even when the workflow requests a write. Do not disable this
protection to make `dev` runs faster. Seed compatible caches from the trusted
default-branch workflow instead.

## Concurrent checks

Qt and native dependencies are installed before Cargo compilation. The native
check runner uses independent core, streamer, and Qt lanes, with a bounded CPU
budget and separate logs. Core tests finish before the signed-helper integration
test cleans and rebuilds their shared target. That integration test retains its
single-threaded execution and ephemeral signing key.

Each Qt build directory has one CTest scheduler. Existing serial tests, acceptance
durations, filters, timeouts, doctests, and workspace coverage remain unchanged.
Standalone Linux package tests use eight workers on the existing eight-core
runners. Apple Silicon package tests use three workers, and Intel macOS retains
two. Full package performance acceptance does not overlap external Rust workloads.

## Compare runs

Use two consecutive runs on the same commit to distinguish a cold cache from a
warm cache. Compare a subsequent source edit to check incremental invalidation.
Changes to vendored sources must rebuild the affected native library.

The `native-check-diagnostics-*` artifacts contain separate lane logs, elapsed
times, the Qt JUnit report, and Cargo timing reports from test compilation.
Compare those phase times before claiming a speedup. A workflow's creation time
and its first job's start time also reveal delays before compilation begins.
Those delays are separate from Cargo and test performance.

The manual `qt-ci` workflow accepts `macos_intel_host=arm64` for an Intel
cross-build measurement on the existing six-core Apple Silicon runner. The
default remains `intel`. The experiment builds x86_64 libraries and applications
with native ARM compiler tools, then runs target Rust tests and packaged-app
checks through Rosetta. All existing package tests and relocated-bundle checks
still run. This measures build performance, not Intel-specific GPU behavior.
Selecting `arm64` disables nightly signing and publication even if
`publish_nightly` is enabled. Measurement packages remain unsigned artifacts.

The same manual workflow accepts `linux_arm64_host=x64` to benchmark an additional
ARM64 Linux release and all Qt test executables on the existing eight-core x64
runner alongside the unchanged native ARM64 baseline. The default is `native`.
A separate Ubuntu 24.04 ARM64 development
sysroot is downloaded and extracted without installing ARM programs on the host.
Native x64 Qt 6.8.3 tools generate code for the matching ARM64 Qt libraries.
The ARM64 SDL3 build and Rust native dependencies use the same cross toolchain.

The existing eight-core ARM runner restores the checked build/test tree and SDL
prefix without rebuilding, verifies the revision, version, paths, file hashes,
ELF architecture and CTest inventory, and runs the unchanged complete CTest,
AppImage, bundled-DEB and smoke checks. No tests run under emulation. This option
also disables nightly signing and publication. Flatpak is not cross-compiled.

Run with `linux_arm64_host=x64`, `publish_nightly=false` and
`macos_intel_host=arm64`. Compare the native ARM64 package job with the additional
cross-compilation and ARM validation jobs from that same run. Include SDK setup,
compilation, archive upload/download, native tests and packaging in the ARM64
comparison. Other platform builds do not wait for the experiment. Its packages
are uploaded separately and never replace the native package inventory.

References: [Cargo build timings](https://doc.rust-lang.org/cargo/reference/timings.html),
[GitHub cache matching](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching),
and [Blacksmith Sticky Disks](https://docs.blacksmith.sh/blacksmith-caching/dependencies-sticky-disks).
