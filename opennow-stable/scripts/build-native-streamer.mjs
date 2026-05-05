import { copyFileSync, chmodSync, existsSync, mkdirSync, statSync } from "node:fs";
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
const nativeTarget = process.env.OPENNOW_NATIVE_STREAMER_TARGET?.trim() || "";
const platformKey = process.env.OPENNOW_NATIVE_STREAMER_PLATFORM_KEY?.trim() || `${process.platform}-${process.arch}`;
const targetReleaseDir = nativeTarget
  ? join(crateRoot, "target", nativeTarget, "release")
  : join(crateRoot, "target", "release");
const builtBinary = join(targetReleaseDir, exeName);
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

function appendEnvValue(env, key, value) {
  env[key] = env[key]?.trim() ? `${env[key]} ${value}` : value;
}

function configureDarwinLinkerPadding(env, nativeFeatures) {
  const isDarwinBuild = process.platform === "darwin" || /apple-darwin$/.test(nativeTarget);
  if (!isDarwinBuild || !hasFeature(nativeFeatures, "gstreamer") || process.env.OPENNOW_BUNDLE_GSTREAMER_RUNTIME !== "1") {
    return;
  }
  appendEnvValue(env, "RUSTFLAGS", "-C link-arg=-Wl,-headerpad_max_install_names");
}

function brewPrefix() {
  const result = spawnSync("brew", ["--prefix"], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() || null : null;
}

function brewPrefixForPackage(packageName) {
  const result = spawnSync("brew", ["--prefix", packageName], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() || null : null;
}

function configuredCandidate(root, source) {
  return root ? { root, source } : null;
}

function existingConfiguredCandidates(candidates) {
  return candidates.filter(Boolean);
}

function formatCandidateSources(candidates) {
  return candidates.map((candidate) => candidate.source).join(", ") || "none";
}

function configureGstreamerSdk(env) {
  if (process.platform === "win32") {
    const candidates = existingConfiguredCandidates([
      configuredCandidate(env.GSTREAMER_1_0_ROOT_MSVC_X86_64, "GSTREAMER_1_0_ROOT_MSVC_X86_64"),
      configuredCandidate("C:\\Program Files\\gstreamer\\1.0\\msvc_x86_64", "default Program Files"),
      configuredCandidate("C:\\gstreamer\\1.0\\msvc_x86_64", "default C drive"),
    ]);
    const sdk = candidates
      .map((candidate) => {
        const pkgConfigFile = join(candidate.root, "lib", "pkgconfig", "gstreamer-1.0.pc");
        const pkgConfigBinary = ["pkg-config.exe", "pkgconf.exe"]
          .map((name) => join(candidate.root, "bin", name))
          .find((path) => existsSync(path));
        return { ...candidate, pkgConfigBinary, pkgConfigFile };
      })
      .find((candidate) => candidate.pkgConfigBinary && existsSync(candidate.pkgConfigFile));
    if (!sdk) {
      console.warn(
        [
          "GStreamer SDK was not found automatically; relying on the current PKG_CONFIG environment.",
          `Checked ${candidates.length} candidate source(s): ${formatCandidateSources(candidates)}.`,
          "Expected relative files: bin/pkg-config.exe or bin/pkgconf.exe, and lib/pkgconfig/gstreamer-1.0.pc.",
        ].join(" "),
      );
      return null;
    }
    const pkgConfigDir = join(sdk.root, "lib", "pkgconfig");
    env.PKG_CONFIG = sdk.pkgConfigBinary;
    env.PKG_CONFIG_PATH = env.PKG_CONFIG_PATH ? `${pkgConfigDir}${delimiter}${env.PKG_CONFIG_PATH}` : pkgConfigDir;
    prependEnvPath(env, join(sdk.root, "bin"));
    console.log(`Configured GStreamer SDK from ${sdk.source}.`);
    console.log("Configured pkg-config executable for GStreamer SDK.");
    return sdk.root;
  }

  if (process.platform === "darwin") {
    const homebrewRoot = brewPrefix();
    const homebrewGStreamerRoot = brewPrefixForPackage("gstreamer");
    const candidates = existingConfiguredCandidates([
      configuredCandidate(env.GSTREAMER_1_0_ROOT_MACOS, "GSTREAMER_1_0_ROOT_MACOS"),
      configuredCandidate("/Library/Frameworks/GStreamer.framework/Versions/1.0", "GStreamer framework version 1.0"),
      configuredCandidate("/Library/Frameworks/GStreamer.framework/Versions/Current", "GStreamer framework current"),
      configuredCandidate(homebrewGStreamerRoot, "Homebrew gstreamer prefix"),
      configuredCandidate(homebrewRoot && join(homebrewRoot, "opt", "gstreamer"), "Homebrew opt gstreamer"),
      configuredCandidate(homebrewRoot, "Homebrew prefix"),
      configuredCandidate("/opt/homebrew", "default Homebrew Apple Silicon prefix"),
      configuredCandidate("/usr/local", "default Homebrew Intel prefix"),
    ]);
    const sdk = candidates.find((candidate) =>
      existsSync(join(candidate.root, "lib", "pkgconfig", "gstreamer-1.0.pc"))
      && existsSync(join(candidate.root, "lib", "libgstreamer-1.0.dylib")),
    );
    if (!sdk) {
      console.warn(
        [
          "GStreamer macOS SDK was not found automatically; relying on the current PKG_CONFIG environment.",
          `Checked ${candidates.length} candidate source(s): ${formatCandidateSources(candidates)}.`,
          "Expected relative files: lib/pkgconfig/gstreamer-1.0.pc and lib/libgstreamer-1.0.dylib.",
        ].join(" "),
      );
      return null;
    }
    const sdkRoot = sdk.root;
    const pkgConfigDir = join(sdkRoot, "lib", "pkgconfig");
    env.GSTREAMER_1_0_ROOT_MACOS = sdkRoot;
    env.PKG_CONFIG_PATH = env.PKG_CONFIG_PATH ? `${pkgConfigDir}${delimiter}${env.PKG_CONFIG_PATH}` : pkgConfigDir;
    prependEnvPath(env, join(sdkRoot, "bin"));
    env.DYLD_LIBRARY_PATH = env.DYLD_LIBRARY_PATH ? `${join(sdkRoot, "lib")}${delimiter}${env.DYLD_LIBRARY_PATH}` : join(sdkRoot, "lib");
    env.DYLD_FALLBACK_LIBRARY_PATH = env.DYLD_FALLBACK_LIBRARY_PATH ? `${join(sdkRoot, "lib")}${delimiter}${env.DYLD_FALLBACK_LIBRARY_PATH}` : join(sdkRoot, "lib");
    if (/apple-darwin$/.test(nativeTarget)) {
      env.PKG_CONFIG_ALLOW_CROSS = "1";
      env.PKG_CONFIG_SYSROOT_DIR = env.PKG_CONFIG_SYSROOT_DIR || "/";
    }
    console.log(`Configured GStreamer SDK from ${sdk.source}.`);
    return sdkRoot;
  }

  return null;
}

function bundleGstreamerRuntime(sdkRoot) {
  if (process.env.OPENNOW_BUNDLE_GSTREAMER_RUNTIME !== "1") {
    return false;
  }

  const args = [
    join(__dirname, "bundle-gstreamer-runtime.mjs"),
    "--dest",
    join(packagePlatformBinaryDir, "gstreamer"),
  ];

  if (sdkRoot) {
    args.push("--sdk-root", sdkRoot);
  }
  args.push("--binary", packagePlatformBinary);

  const result = spawnSync(process.execPath, args, {
    cwd: packageRoot,
    stdio: "inherit",
    env: process.env,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
  return true;
}

function isExistingFile(path) {
  try {
    return existsSync(path) && statSync(path).isFile();
  } catch {
    return false;
  }
}

function isExistingDirectory(path) {
  try {
    return existsSync(path) && statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function buildBundledGstreamerEnv(baseEnv, binaryPath) {
  const env = { SystemRoot: baseEnv.SystemRoot, WINDIR: baseEnv.WINDIR };
  const runtimeRoot = join(dirname(binaryPath), "gstreamer");
  const binDir = join(runtimeRoot, "bin");
  const libDir = join(runtimeRoot, "lib");
  const pluginDir = join(libDir, "gstreamer-1.0");
  const scanner = join(runtimeRoot, "libexec", "gstreamer-1.0", process.platform === "win32" ? "gst-plugin-scanner.exe" : "gst-plugin-scanner");
  const gioModulesDir = join(libDir, "gio", "modules");

  if (!isExistingDirectory(runtimeRoot)) {
    throw new Error(`Bundled GStreamer runtime was not found next to ${binaryPath}`);
  }
  if (process.platform === "win32") prependEnvPath(env, dirname(binaryPath));
  if (isExistingDirectory(binDir)) prependEnvPath(env, binDir);
  if (isExistingDirectory(pluginDir)) {
    env.GST_PLUGIN_PATH = pluginDir;
    env.GST_PLUGIN_PATH_1_0 = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH_1_0 = pluginDir;
  }
  if (isExistingFile(scanner)) {
    env.GST_PLUGIN_SCANNER = scanner;
    env.GST_PLUGIN_SCANNER_1_0 = scanner;
  }
  env.GST_REGISTRY_REUSE_PLUGIN_SCANNER = "no";
  if (isExistingDirectory(gioModulesDir)) {
    env.GIO_MODULE_DIR = gioModulesDir;
    env.GIO_EXTRA_MODULES = gioModulesDir;
  }
  if (process.platform === "linux" && isExistingDirectory(libDir)) {
    env.LD_LIBRARY_PATH = env.LD_LIBRARY_PATH ? `${libDir}${delimiter}${env.LD_LIBRARY_PATH}` : libDir;
  }
  if (process.platform === "darwin" && isExistingDirectory(libDir)) {
    env.DYLD_LIBRARY_PATH = env.DYLD_LIBRARY_PATH ? `${libDir}${delimiter}${env.DYLD_LIBRARY_PATH}` : libDir;
    env.DYLD_FALLBACK_LIBRARY_PATH = env.DYLD_FALLBACK_LIBRARY_PATH ? `${libDir}${delimiter}${env.DYLD_FALLBACK_LIBRARY_PATH}` : libDir;
  }
  if (process.platform === "win32" && isExistingDirectory(libDir)) {
    prependEnvPath(env, libDir);
  }
  return env;
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

  if (!Array.isArray(capabilities.videoBackends) || capabilities.videoBackends.length === 0) {
    console.error(
      `Native streamer verification expected video backend capabilities, got: ${JSON.stringify(
        capabilities,
      )}`,
    );
    process.exit(1);
  }

  const availableVideoBackends = capabilities.videoBackends
    .filter((backend) => backend?.available)
    .map((backend) => {
      const codecs = Array.isArray(backend.codecs)
        ? backend.codecs.filter((codec) => codec.available).map((codec) => codec.codec).join("/")
        : "";
      return `${backend.backend}${codecs ? `(${codecs})` : ""}`;
    });

  if (availableVideoBackends.length === 0) {
    console.error(
      `Native streamer verification found no usable video backend: ${JSON.stringify(
        capabilities.videoBackends,
      )}`,
    );
    process.exit(1);
  }

  console.log(`Verified native streamer GStreamer capabilities: ${availableVideoBackends.join(", ")}.`);
}

const cargoArgs = ["build", "--release", "--manifest-path", manifestPath];
if (nativeTarget) {
  cargoArgs.push("--target", nativeTarget);
}
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
  gstreamerSdkRoot = configureGstreamerSdk(buildEnv);
}
configureDarwinLinkerPadding(buildEnv, nativeFeatures);

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
  if (bundleGstreamerRuntime(gstreamerSdkRoot)) {
    verifyGstreamerBinary(packagePlatformBinary, buildBundledGstreamerEnv(buildEnv, packagePlatformBinary));
  }
}

console.log(`Copied native streamer to ${packageBinary}`);
console.log(`Copied native streamer to ${packagePlatformBinary}`);
