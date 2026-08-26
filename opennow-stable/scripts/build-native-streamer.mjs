import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import { nativeStreamerCargoArgs } from "./build-native-streamer-config.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(__dirname, "..");
const repoRoot = resolve(packageRoot, "..");
const workspaceRoot = join(repoRoot, "native", "opennow-streamer");
const manifestPath = join(workspaceRoot, "Cargo.toml");
const protocolSourcePath = join(
  workspaceRoot,
  "crates",
  "opennow-streamer-protocol",
  "src",
  "lib.rs",
);
const appProtocolSourcePath = join(packageRoot, "src", "shared", "nativeStreamer.ts");
const macInfoPlistSourcePath = join(workspaceRoot, "macos", "OpenNOWStreamer-Info.plist");
const workspaceManifestSource = readFileSync(manifestPath, "utf8");
const nativePackageVersion = workspaceManifestSource.match(
  /\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
)?.[1];
if (!nativePackageVersion) {
  throw new Error(`Unable to read native streamer package version from ${manifestPath}`);
}
const nativeTarget = process.env.OPENNOW_NATIVE_STREAMER_TARGET?.trim() || "";
const platformKey = process.env.OPENNOW_NATIVE_STREAMER_PLATFORM_KEY?.trim()
  || `${process.platform}-${process.arch}`;
// Production Linux media libraries are statically bundled. VA-API remains an
// opt-in developer backend because linking host libva would make the binary
// distro-dependent; Vulkan Video and NVDEC cover the packaged GPU paths.
const enableLinuxVaapi = process.env.OPENNOW_NATIVE_LINUX_VAAPI === "1";
const enableLinuxFfmpeg = process.env.OPENNOW_NATIVE_LINUX_FFMPEG !== "0";
const exeName = platformKey.startsWith("win32-") ? "opennow-streamer.exe" : "opennow-streamer";
const releaseDir = nativeTarget
  ? join(workspaceRoot, "target", nativeTarget, "release")
  : join(workspaceRoot, "target", "release");
const builtBinary = join(releaseDir, exeName);
const binRoot = join(workspaceRoot, "bin");
const platformDirectory = join(binRoot, platformKey);
const macBundle = join(platformDirectory, "OpenNOWStreamer.app");
const platformBinary = platformKey.startsWith("darwin-")
  ? join(macBundle, "Contents", "MacOS", exeName)
  : join(platformDirectory, exeName);
rmSync(join(binRoot, exeName), { force: true });

function readVersion(path, pattern, label) {
  const match = readFileSync(path, "utf8").match(pattern);
  if (!match) throw new Error(`Unable to read ${label} protocol version from ${path}`);
  return Number.parseInt(match[1], 10);
}

const appProtocolVersion = readVersion(
  appProtocolSourcePath,
  /export\s+const\s+NATIVE_STREAMER_PROTOCOL_VERSION\s*=\s*(\d+)\s*;/,
  "app",
);
const nativeProtocolVersion = readVersion(
  protocolSourcePath,
  /pub\s+const\s+PROTOCOL_VERSION\s*:\s*u64\s*=\s*(\d+)\s*;/,
  "native",
);
if (appProtocolVersion !== nativeProtocolVersion) {
  throw new Error(
    `Native streamer protocol mismatch: app sends ${appProtocolVersion}, native expects ${nativeProtocolVersion}.`,
  );
}

const cargoArgs = nativeStreamerCargoArgs({
  manifestPath,
  nativeTarget,
  platformKey,
  enableLinuxVaapi,
  enableLinuxFfmpeg,
});

const cargoEnvironment = {
  ...process.env,
  ...(platformKey.startsWith("linux-")
      ? {
        // cc-rs would otherwise add a dynamic libstdc++ for OpenH264. The
        // executable build script links the same runtime statically.
        CXXSTDLIB: "",
      }
    : {}),
};

const build = spawnSync("cargo", cargoArgs, {
  cwd: workspaceRoot,
  stdio: "inherit",
  env: cargoEnvironment,
});
if (build.status !== 0) process.exit(build.status ?? 1);
if (!existsSync(builtBinary)) throw new Error(`Native streamer build missing: ${builtBinary}`);

if (platformKey.startsWith("darwin-")) {
  if (!existsSync(macInfoPlistSourcePath)) {
    throw new Error(`Native streamer macOS Info.plist is missing: ${macInfoPlistSourcePath}`);
  }
  // WindowServer does not reliably composite windows owned by a bare
  // command-line process. Ship the streamer as a regular application; Electron
  // launches the bundle through LaunchServices and communicates over FIFOs.
  rmSync(join(platformDirectory, exeName), { force: true });
  rmSync(macBundle, { recursive: true, force: true });
  mkdirSync(dirname(platformBinary), { recursive: true });
  mkdirSync(join(macBundle, "Contents", "Resources"), { recursive: true });
  const macInfoPlist = readFileSync(macInfoPlistSourcePath, "utf8")
    .replaceAll("__OPENNOW_STREAMER_VERSION__", nativePackageVersion);
  writeFileSync(join(macBundle, "Contents", "Info.plist"), macInfoPlist);
  copyFileSync(builtBinary, platformBinary);
  chmodSync(platformBinary, 0o755);
  if (process.platform === "darwin") {
    // Copying the linker-signed Mach-O into an application bundle changes its
    // code-signing resource context. Re-sign inside-out so local development
    // and unsigned builds launch the same valid nested code shape as releases.
    for (const [target, extraArgs] of [
      [platformBinary, ["--identifier", "com.zortos.opennow.streamer.executable"]],
      [macBundle, []],
    ]) {
      const sign = spawnSync(
        "codesign",
        ["--force", "--sign", "-", ...extraArgs, target],
        { stdio: "inherit" },
      );
      if (sign.status !== 0) process.exit(sign.status ?? 1);
    }
  }
} else if (process.platform === "linux" && platformKey.startsWith("linux-")) {
  mkdirSync(dirname(platformBinary), { recursive: true });
  // A running ELF cannot be truncated in place (ETXTBSY). Stage the new
  // executable beside it and atomically replace the directory entry so an
  // active session can finish on the old inode while the next one uses this
  // build.
  const stagedBinary = `${platformBinary}.${process.pid}.tmp`;
  rmSync(stagedBinary, { force: true });
  try {
    copyFileSync(builtBinary, stagedBinary);
    chmodSync(stagedBinary, 0o755);
    renameSync(stagedBinary, platformBinary);
  } finally {
    rmSync(stagedBinary, { force: true });
  }
} else {
  mkdirSync(dirname(platformBinary), { recursive: true });
  copyFileSync(builtBinary, platformBinary);
  if (!platformKey.startsWith("win32-")) {
    chmodSync(platformBinary, 0o755);
  }
}

const hostPlatformKey = `${process.platform}-${process.arch}`;
if (!nativeTarget || platformKey === hostPlatformKey) {
  const input = [
    JSON.stringify({ id: "verify", type: "hello", protocolVersion: nativeProtocolVersion }),
    JSON.stringify({
      id: "verify-start",
      type: "start",
      context: {
        session: {
          sessionId: "build-verification",
          serverIp: "127.0.0.1",
          iceServers: [],
        },
        settings: { codec: "H264" },
        shortcuts: {},
      },
    }),
    JSON.stringify({ id: "stop", type: "stop", reason: "build verification" }),
    "",
  ].join("\n");
  const verify = spawnSync(platformBinary, [], {
    cwd: packageRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      ...(process.platform === "darwin" ? {} : { OPENNOW_NATIVE_VIDEO_BACKEND: "software" }),
      SDL_AUDIODRIVER: "dummy",
      SDL_VIDEODRIVER: "dummy",
    },
    input,
    timeout: 15_000,
  });
  if (verify.status !== 0) {
    const detail = verify.stderr || verify.stdout || verify.error?.message || "no process output";
    throw new Error(
      `Native streamer verification failed (status=${verify.status}, signal=${verify.signal}): ${detail}`,
    );
  }
  const messages = verify.stdout
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const ready = messages.find((message) => message.id === "verify" && message.type === "ready");
  const stopped = messages.find((message) => message.id === "stop" && message.type === "ok");
  const started = messages.find((message) => message.id === "verify-start" && message.type === "ok");
  const capabilitiesComplete = ready?.capabilities?.supportsOfferAnswer === true
    && ready.capabilities.supportsVideoDecode === true
    && ready.capabilities.supportsVideoPresent === true
    && ready.capabilities.supportsAudioDecode === true
    && ready.capabilities.supportsAudioOutput === true;
  if (!capabilitiesComplete || !started || !stopped) {
    throw new Error(`Native streamer verification returned an incomplete handshake: ${verify.stdout}`);
  }
}

console.log(`Built native streamer v2: ${platformBinary}`);
