import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function parseArgs(argv) {
  const parsed = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (!value.startsWith("--")) {
      continue;
    }
    const key = value.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith("--")) {
      parsed.set(key, "true");
      continue;
    }
    parsed.set(key, next);
    index += 1;
  }
  return parsed;
}

function windowsSdkCandidates(explicitSdkRoot) {
  return [
    explicitSdkRoot,
    process.env.GSTREAMER_1_0_ROOT_MSVC_X86_64,
    "C:\\Program Files\\gstreamer\\1.0\\msvc_x86_64",
    "C:\\gstreamer\\1.0\\msvc_x86_64",
  ].filter(Boolean);
}

function resolveWindowsSdkRoot(explicitSdkRoot) {
  const sdkRoot = windowsSdkCandidates(explicitSdkRoot).find((candidate) =>
    existsSync(join(candidate, "bin", "gstreamer-1.0-0.dll"))
    && existsSync(join(candidate, "lib", "gstreamer-1.0")),
  );

  if (!sdkRoot) {
    throw new Error(
      "GStreamer MSVC x86_64 runtime was not found. Install the runtime/development MSI or pass --sdk-root.",
    );
  }

  return sdkRoot;
}

function copyPathIfPresent(source, destination) {
  if (!existsSync(source)) {
    return false;
  }

  const stats = statSync(source);
  if (stats.isDirectory()) {
    cpSync(source, destination, {
      recursive: true,
      force: true,
      filter: (entry) => {
        const lower = entry.toLowerCase();
        return !lower.endsWith(".pdb")
          && !lower.endsWith(".lib")
          && !lower.endsWith(".a")
          && !lower.includes(`${join("share", "doc").toLowerCase()}`);
      },
    });
    return true;
  }

  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  return true;
}

function copyMatchingFiles(sourceDir, destinationDir, pattern) {
  if (!existsSync(sourceDir)) {
    return;
  }

  mkdirSync(destinationDir, { recursive: true });
  for (const name of readdirSync(sourceDir)) {
    if (pattern.test(name)) {
      copyFileSync(join(sourceDir, name), join(destinationDir, name));
    }
  }
}

function bundleWindowsRuntime({ sdkRoot, destination }) {
  const resolvedSdkRoot = resolveWindowsSdkRoot(sdkRoot);
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });

  const copied = [
    copyPathIfPresent(join(resolvedSdkRoot, "bin"), join(destination, "bin")),
    copyPathIfPresent(
      join(resolvedSdkRoot, "lib", "gstreamer-1.0"),
      join(destination, "lib", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedSdkRoot, "lib", "gio", "modules"),
      join(destination, "lib", "gio", "modules"),
    ),
    copyPathIfPresent(
      join(resolvedSdkRoot, "libexec", "gstreamer-1.0"),
      join(destination, "libexec", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedSdkRoot, "share", "gstreamer-1.0"),
      join(destination, "share", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedSdkRoot, "share", "glib-2.0"),
      join(destination, "share", "glib-2.0"),
    ),
    copyPathIfPresent(join(resolvedSdkRoot, "etc"), join(destination, "etc")),
  ].filter(Boolean).length;

  copyMatchingFiles(resolvedSdkRoot, destination, /^(copying|license|readme)/i);
  writeFileSync(
    join(destination, "OPENNOW-GSTREAMER-RUNTIME.txt"),
    [
      "OpenNOW private GStreamer runtime bundle",
      `Source: ${resolvedSdkRoot}`,
      `Generated: ${new Date().toISOString()}`,
      "",
      "This directory is loaded only for the native streamer child process.",
      "Keep the GStreamer directory layout intact so plugins resolve relative to the core DLL.",
      "",
    ].join("\n"),
  );

  console.log(`Bundled GStreamer runtime from ${resolvedSdkRoot} to ${destination} (${copied} paths).`);
}

function brewPrefix() {
  const result = spawnSync("brew", ["--prefix"], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return null;
  }
  return result.stdout.trim() || null;
}

function macosRuntimeCandidates(explicitSdkRoot) {
  return [
    explicitSdkRoot,
    process.env.GSTREAMER_1_0_ROOT_MACOS,
    brewPrefix(),
    "/opt/homebrew",
    "/usr/local",
  ].filter(Boolean);
}

function resolveMacosRuntimeRoot(explicitSdkRoot) {
  const runtimeRoot = macosRuntimeCandidates(explicitSdkRoot).find((candidate) =>
    existsSync(join(candidate, "lib", "libgstreamer-1.0.dylib"))
    && existsSync(join(candidate, "lib", "gstreamer-1.0")),
  );

  if (!runtimeRoot) {
    throw new Error(
      "GStreamer Homebrew runtime was not found. Install gstreamer and plugin packages with brew or pass --sdk-root.",
    );
  }

  return runtimeRoot;
}

function bundleMacosRuntime({ sdkRoot, destination }) {
  const resolvedRuntimeRoot = resolveMacosRuntimeRoot(sdkRoot);
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });

  const copied = [
    copyPathIfPresent(
      join(resolvedRuntimeRoot, "lib", "gstreamer-1.0"),
      join(destination, "lib", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedRuntimeRoot, "lib", "gio", "modules"),
      join(destination, "lib", "gio", "modules"),
    ),
    copyPathIfPresent(
      join(resolvedRuntimeRoot, "libexec", "gstreamer-1.0"),
      join(destination, "libexec", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedRuntimeRoot, "share", "gstreamer-1.0"),
      join(destination, "share", "gstreamer-1.0"),
    ),
    copyPathIfPresent(
      join(resolvedRuntimeRoot, "share", "glib-2.0"),
      join(destination, "share", "glib-2.0"),
    ),
  ].filter(Boolean).length;

  copyMatchingFiles(join(resolvedRuntimeRoot, "lib"), join(destination, "lib"), /\.dylib$/);
  copyMatchingFiles(resolvedRuntimeRoot, destination, /^(copying|license|readme)/i);
  writeFileSync(
    join(destination, "OPENNOW-GSTREAMER-RUNTIME.txt"),
    [
      "OpenNOW private GStreamer runtime bundle",
      `Source: ${resolvedRuntimeRoot}`,
      `Generated: ${new Date().toISOString()}`,
      "",
      "This directory is loaded only for the native streamer child process.",
      "macOS uses DYLD_LIBRARY_PATH for this child process so copied plugins resolve against this bundle.",
      "",
    ].join("\n"),
  );

  console.log(`Bundled GStreamer runtime from ${resolvedRuntimeRoot} to ${destination} (${copied} paths plus dylibs).`);
}

const args = parseArgs(process.argv.slice(2));
const destination = args.get("dest");
if (!destination) {
  console.error("Usage: node scripts/bundle-gstreamer-runtime.mjs --dest <runtime-dir> [--sdk-root <path>]");
  process.exit(1);
}

try {
  if (process.platform === "win32") {
    bundleWindowsRuntime({
      sdkRoot: args.get("sdk-root"),
      destination: resolve(__dirname, "..", destination),
    });
  } else if (process.platform === "darwin") {
    bundleMacosRuntime({
      sdkRoot: args.get("sdk-root"),
      destination: resolve(__dirname, "..", destination),
    });
  } else {
    throw new Error(
      `Bundled GStreamer runtime collection is implemented for Windows and macOS only (got ${process.platform}).`,
    );
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
