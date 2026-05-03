import { copyFileSync, chmodSync, existsSync, mkdirSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const packageRoot = resolve(__dirname, "..");
const repoRoot = resolve(packageRoot, "..");
const crateRoot = join(repoRoot, "native", "opennow-streamer");
const manifestPath = join(crateRoot, "Cargo.toml");
const exeName = process.platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
const platformKey = `${process.platform}-${process.arch}`;
const builtBinary = join(crateRoot, "target", "release", exeName);
const packageBinaryDir = join(crateRoot, "bin");
const packageBinary = join(packageBinaryDir, exeName);
const packagePlatformBinaryDir = join(packageBinaryDir, platformKey);
const packagePlatformBinary = join(packagePlatformBinaryDir, exeName);

function hasFeature(features, feature) {
  return features
    .split(/[,\s]+/)
    .map((value) => value.trim().toLowerCase())
    .filter(Boolean)
    .includes(feature);
}

function prependEnvPath(env, directory) {
  const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
  env[pathKey] = env[pathKey] ? `${directory}${delimiter}${env[pathKey]}` : directory;
}

function configureWindowsGstreamerSdk(env) {
  if (process.platform !== "win32") {
    return null;
  }

  const candidates = [
    env.GSTREAMER_1_0_ROOT_MSVC_X86_64,
    "C:\\Program Files\\gstreamer\\1.0\\msvc_x86_64",
    "C:\\gstreamer\\1.0\\msvc_x86_64",
  ].filter(Boolean);

  const sdkRoot = candidates.find(
    (candidate) =>
      existsSync(join(candidate, "bin", "pkg-config.exe")) &&
      existsSync(join(candidate, "lib", "pkgconfig", "gstreamer-1.0.pc")),
  );

  if (!sdkRoot) {
    console.warn(
      "GStreamer SDK was not found automatically; relying on the current PKG_CONFIG environment.",
    );
    return null;
  }

  const pkgConfigDir = join(sdkRoot, "lib", "pkgconfig");
  env.PKG_CONFIG = join(sdkRoot, "bin", "pkg-config.exe");
  env.PKG_CONFIG_PATH = env.PKG_CONFIG_PATH
    ? `${pkgConfigDir}${delimiter}${env.PKG_CONFIG_PATH}`
    : pkgConfigDir;
  prependEnvPath(env, join(sdkRoot, "bin"));
  console.log(`Configured GStreamer SDK: ${sdkRoot}`);
  return sdkRoot;
}

function bundleGstreamerRuntime(sdkRoot) {
  if (process.env.OPENNOW_BUNDLE_GSTREAMER_RUNTIME !== "1") {
    return;
  }

  const args = [
    join(__dirname, "bundle-gstreamer-runtime.mjs"),
    "--dest",
    join(packagePlatformBinaryDir, "gstreamer"),
  ];

  if (sdkRoot) {
    args.push("--sdk-root", sdkRoot);
  }

  const result = spawnSync(process.execPath, args, {
    cwd: packageRoot,
    stdio: "inherit",
    env: process.env,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function verifyGstreamerBinary(binaryPath, env) {
  const result = spawnSync(binaryPath, {
    input: `${JSON.stringify({ id: "verify", type: "hello", protocolVersion: 1 })}\n`,
    encoding: "utf8",
    env: {
      ...env,
      OPENNOW_NATIVE_STREAMER_BACKEND: "gstreamer",
    },
  });

  if (result.status !== 0) {
    console.error(result.stderr || result.stdout);
    console.error(`Native streamer verification failed for ${binaryPath}`);
    process.exit(result.status ?? 1);
  }

  const responseLine = result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);

  let response;
  try {
    response = JSON.parse(responseLine ?? "");
  } catch (error) {
    console.error(`Native streamer verification returned invalid JSON: ${responseLine}`);
    process.exit(1);
  }

  const capabilities = response.capabilities;
  if (
    response.type !== "ready" ||
    capabilities?.backend !== "gstreamer" ||
    capabilities?.supportsOfferAnswer !== true ||
    capabilities?.supportsInput !== true
  ) {
    console.error(
      `Native streamer verification expected a GStreamer backend, got: ${JSON.stringify(
        capabilities,
      )}`,
    );
    process.exit(1);
  }

  console.log("Verified native streamer GStreamer capabilities.");
}

const cargoArgs = ["build", "--release", "--manifest-path", manifestPath];
const nativeFeatures = process.env.OPENNOW_NATIVE_STREAMER_FEATURES?.trim() || "gstreamer";
if (nativeFeatures && nativeFeatures.toLowerCase() !== "none") {
  cargoArgs.push("--features", nativeFeatures);
}
console.log(
  nativeFeatures.toLowerCase() === "none"
    ? "Building native streamer without optional features."
    : `Building native streamer with features: ${nativeFeatures}`,
);

const buildEnv = { ...process.env };
let gstreamerSdkRoot = null;
if (hasFeature(nativeFeatures, "gstreamer")) {
  gstreamerSdkRoot = configureWindowsGstreamerSdk(buildEnv);
}

const cargoCommand = process.platform === "win32" ? "cargo.exe" : "cargo";
const result = spawnSync(cargoCommand, cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit",
  env: buildEnv,
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

if (!existsSync(builtBinary)) {
  console.error(`Native streamer build did not produce ${builtBinary}`);
  process.exit(1);
}

mkdirSync(packageBinaryDir, { recursive: true });
mkdirSync(packagePlatformBinaryDir, { recursive: true });
copyFileSync(builtBinary, packageBinary);
copyFileSync(builtBinary, packagePlatformBinary);

if (process.platform !== "win32") {
  chmodSync(packageBinary, 0o755);
  chmodSync(packagePlatformBinary, 0o755);
}

if (hasFeature(nativeFeatures, "gstreamer")) {
  verifyGstreamerBinary(packageBinary, buildEnv);
  bundleGstreamerRuntime(gstreamerSdkRoot);
}

console.log(`Copied native streamer to ${packageBinary}`);
console.log(`Copied native streamer to ${packagePlatformBinary}`);
