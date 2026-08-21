import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

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
const nativeTarget = process.env.OPENNOW_NATIVE_STREAMER_TARGET?.trim() || "";
const platformKey = process.env.OPENNOW_NATIVE_STREAMER_PLATFORM_KEY?.trim()
  || `${process.platform}-${process.arch}`;
const exeName = platformKey.startsWith("win32-") ? "opennow-streamer.exe" : "opennow-streamer";
const releaseDir = nativeTarget
  ? join(workspaceRoot, "target", nativeTarget, "release")
  : join(workspaceRoot, "target", "release");
const builtBinary = join(releaseDir, exeName);
const binRoot = join(workspaceRoot, "bin");
const platformBinary = join(binRoot, platformKey, exeName);
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

const cargoArgs = [
  "build",
  "--locked",
  "--release",
  "--package",
  "opennow-streamer",
  "--manifest-path",
  manifestPath,
];
if (nativeTarget) cargoArgs.push("--target", nativeTarget);

const build = spawnSync("cargo", cargoArgs, {
  cwd: workspaceRoot,
  stdio: "inherit",
  env: process.env,
});
if (build.status !== 0) process.exit(build.status ?? 1);
if (!existsSync(builtBinary)) throw new Error(`Native streamer build missing: ${builtBinary}`);

mkdirSync(dirname(platformBinary), { recursive: true });
copyFileSync(builtBinary, platformBinary);
if (!platformKey.startsWith("win32-")) {
  chmodSync(platformBinary, 0o755);
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
      SDL_AUDIODRIVER: "dummy",
      SDL_VIDEODRIVER: "dummy",
    },
    input,
    timeout: 15_000,
  });
  if (verify.status !== 0) {
    throw new Error(`Native streamer verification failed: ${verify.stderr || verify.stdout}`);
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
