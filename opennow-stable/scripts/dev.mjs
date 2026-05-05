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
  ?? "none";
const targetDir = nativeTarget
  ? join(crateRoot, "target", nativeTarget, nativeProfile)
  : join(crateRoot, "target", nativeProfile);
const streamerBinary = join(targetDir, exeName);

function runNativeBuild() {
  if (process.env.OPENNOW_SKIP_NATIVE_STREAMER_BUILD === "1") {
    console.log("Skipping native streamer build because OPENNOW_SKIP_NATIVE_STREAMER_BUILD=1.");
    return;
  }

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
    env: process.env,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function runElectronVite() {
  const child = spawn("electron-vite", ["dev"], {
    cwd: packageRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
    env: {
      ...process.env,
      OPENNOW_NATIVE_STREAMER: process.env.OPENNOW_NATIVE_STREAMER?.trim() || streamerBinary,
    },
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
runElectronVite();
