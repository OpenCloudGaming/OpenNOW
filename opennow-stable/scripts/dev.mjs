import { spawn, spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const packageRoot = resolve(__dirname, "..");
const repoRoot = resolve(packageRoot, "..");
const crateRoot = join(repoRoot, "native", "opennow-streamer");
const exeName = process.platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
const nativeTarget = process.env.OPENNOW_NATIVE_STREAMER_TARGET?.trim() || "";
const nativeProfile = process.env.OPENNOW_NATIVE_STREAMER_DEV_PROFILE?.trim() || "debug";
const nativeFeatures =
  process.env.OPENNOW_NATIVE_STREAMER_DEV_FEATURES?.trim()
  ?? process.env.OPENNOW_NATIVE_STREAMER_FEATURES?.trim()
  ?? "gstreamer";
const targetDir = nativeTarget
  ? join(crateRoot, "target", nativeTarget, nativeProfile)
  : join(crateRoot, "target", nativeProfile);
const streamerBinary = join(targetDir, exeName);

function sanitizedChildEnv(extra = {}) {
  const env = { ...process.env, ...extra };
  for (const key of [
    "NODE_OPTIONS",
    "VSCODE_INSPECTOR_OPTIONS",
    "VSCODE_PID",
    "VSCODE_CWD",
    "VSCODE_NLS_CONFIG",
    "VSCODE_CODE_CACHE_PATH",
    "ELECTRON_RUN_AS_NODE",
  ]) {
    delete env[key];
  }
  return env;
}

function envFlagDisabled(value) {
  if (value === undefined || value === null) return false;
  const normalized = String(value).trim().toLowerCase();
  return normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no";
}

function runMacEmbeddedRendererBuild() {
  if (process.env.OPENNOW_SKIP_NATIVE_STREAMER_BUILD === "1" || process.env.OPENNOW_SKIP_MACOS_EMBEDDED_RENDERER_BUILD === "1") {
    console.log("Skipping macOS embedded renderer build (skip flag set).");
    return;
  }

  if (process.platform !== "darwin") {
    return;
  }

  if (envFlagDisabled(process.env.OPENNOW_MACOS_EMBEDDED_RENDERER)) {
    console.log("Skipping macOS embedded renderer build (OPENNOW_MACOS_EMBEDDED_RENDERER disabled).");
    return;
  }

  console.log("Building macOS embedded renderer addon...");
  const args = [join(__dirname, "build-macos-embedded-renderer.mjs")];
  const result = spawnSync(process.execPath, args, {
    cwd: packageRoot,
    stdio: "inherit",
    env: sanitizedChildEnv(),
  });

  if (result.status !== 0) {
    console.error("macOS embedded renderer build failed; continuing without it.");
  } else {
    console.log("macOS embedded renderer build completed.");
  }
}

function runNativeBuild() {
  if (process.env.OPENNOW_SKIP_NATIVE_STREAMER_BUILD === "1") {
    console.log("Skipping native streamer build because OPENNOW_SKIP_NATIVE_STREAMER_BUILD=1.");
    return;
  }

  console.log(`Building native streamer for dev: profile=${nativeProfile}, features=${nativeFeatures}, binary=${streamerBinary}`);
  const args = [
    join(__dirname, "build-native-streamer.mjs"),
    "--profile",
    nativeProfile,
    "--features",
    nativeFeatures,
    "--no-copy",
    "--skip-verify",
  ];
  const result = spawnSync(process.execPath, args, {
    cwd: packageRoot,
    stdio: "inherit",
    env: sanitizedChildEnv(),
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
  console.log("Native streamer dev build completed.");
}

function runElectronVite() {
  const explicitStreamerBinary = process.env.OPENNOW_NATIVE_STREAMER?.trim() || streamerBinary;
  console.log(`Launching Electron dev server with native streamer: ${explicitStreamerBinary}`);

  const extraEnv = {
    OPENNOW_NATIVE_STREAMER: explicitStreamerBinary,
  };

  // Default embedded renderer on macOS dev unless explicitly disabled
  if (process.platform === "darwin" && !envFlagDisabled(process.env.OPENNOW_MACOS_EMBEDDED_RENDERER)) {
    extraEnv.OPENNOW_MACOS_EMBEDDED_RENDERER = "1";
  }

  const child = spawn("electron-vite", ["dev"], {
    cwd: packageRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
    env: sanitizedChildEnv(extraEnv),
  });

  const forwardSignal = (signal) => {
    if (!child.killed) {
      child.kill(signal);
    }
  };
  process.once("SIGINT", forwardSignal);
  process.once("SIGTERM", forwardSignal);

  child.once("exit", (code, signal) => {
    process.off("SIGINT", forwardSignal);
    process.off("SIGTERM", forwardSignal);
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });

  child.once("error", (error) => {
    console.error(`Failed to start electron-vite dev: ${error.message}`);
    process.exit(1);
  });
}

runNativeBuild();
runMacEmbeddedRendererBuild();
runElectronVite();
