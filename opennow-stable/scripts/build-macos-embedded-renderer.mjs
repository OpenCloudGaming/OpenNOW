import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const packageRoot = resolve(__dirname, "..");
const addonDir = join(packageRoot, "native", "macos-embedded-renderer");
const addonBinDir = join(addonDir, "bin");
const builtBinary = join(addonDir, "build", "Release", "renderer.node");
const stagingBinary = join(addonBinDir, "renderer.node");

function envFlagDisabled(value) {
  if (value === undefined || value === null) return false;
  const normalized = String(value).trim().toLowerCase();
  return normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no";
}

function isBuildDisabled() {
  if (process.platform !== "darwin") return true;
  if (envFlagDisabled(process.env.OPENNOW_MACOS_EMBEDDED_RENDERER)) return true;
  if (process.env.OPENNOW_SKIP_MACOS_EMBEDDED_RENDERER_BUILD === "1") return true;
  return false;
}

function main() {
  if (process.platform !== "darwin") {
    console.log(`[build-macos-embedded-renderer] Skipping — not running on darwin (current: ${process.platform}).`);
    return;
  }

  if (isBuildDisabled()) {
    console.log("[build-macos-embedded-renderer] Skipping — build disabled via env.");
    return;
  }

  console.log("[build-macos-embedded-renderer] Building macOS embedded renderer native addon...");

  if (!existsSync(join(addonDir, "node_modules", "node-addon-api")) ||
      !existsSync(join(addonDir, "node_modules", "node-gyp"))) {
    console.log("[build-macos-embedded-renderer] Installing addon dependencies...");
    const installResult = spawnSync("npm", ["install"], {
      cwd: addonDir,
      stdio: "inherit",
      env: process.env,
    });
    if (installResult.status !== 0) {
      console.error("[build-macos-embedded-renderer] npm install failed.");
      process.exit(installResult.status ?? 1);
    }
  }

  const buildResult = spawnSync("npm", ["run", "build"], {
    cwd: addonDir,
    stdio: "inherit",
    env: process.env,
  });

  if (buildResult.status !== 0) {
    console.error("[build-macos-embedded-renderer] node-gyp build failed.");
    process.exit(buildResult.status ?? 1);
  }

  if (!existsSync(builtBinary)) {
    console.error(`[build-macos-embedded-renderer] Build completed but ${builtBinary} not found.`);
    process.exit(1);
  }

  mkdirSync(addonBinDir, { recursive: true });
  copyFileSync(builtBinary, stagingBinary);
  console.log(`[build-macos-embedded-renderer] Staged addon for packaging: ${stagingBinary}`);
  console.log(`[build-macos-embedded-renderer] Build completed: ${builtBinary}`);
}

main();
