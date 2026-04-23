import { copyFileSync, chmodSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const packageRoot = resolve(__dirname, "..");
const repoRoot = resolve(packageRoot, "..");
const crateRoot = join(repoRoot, "native", "opennow-streamer");
const manifestPath = join(crateRoot, "Cargo.toml");
const exeName = process.platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
const builtBinary = join(crateRoot, "target", "release", exeName);
const packageBinaryDir = join(crateRoot, "bin");
const packageBinary = join(packageBinaryDir, exeName);

const cargoArgs = ["build", "--release", "--manifest-path", manifestPath];
const nativeFeatures = process.env.OPENNOW_NATIVE_STREAMER_FEATURES?.trim();
if (nativeFeatures) {
  cargoArgs.push("--features", nativeFeatures);
}

const result = spawnSync("cargo", cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit",
  shell: process.platform === "win32",
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

if (!existsSync(builtBinary)) {
  console.error(`Native streamer build did not produce ${builtBinary}`);
  process.exit(1);
}

mkdirSync(packageBinaryDir, { recursive: true });
copyFileSync(builtBinary, packageBinary);

if (process.platform !== "win32") {
  chmodSync(packageBinary, 0o755);
}

console.log(`Copied native streamer to ${packageBinary}`);
