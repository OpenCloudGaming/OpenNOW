import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { delimiter, dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const packageRoot = resolve(__dirname, "..");

function parseArgs(argv) {
  const parsed = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (!value.startsWith("--")) continue;
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

function isExistingFile(path) {
  try { return existsSync(path) && statSync(path).isFile(); } catch { return false; }
}

function isExistingDirectory(path) {
  try { return existsSync(path) && statSync(path).isDirectory(); } catch { return false; }
}

function run(command, args, options = {}) {
  return spawnSync(command, args, { encoding: "utf8", ...options });
}

function commandAvailable(command) {
  return run("/usr/bin/env", ["which", command]).status === 0;
}

function brewPrefix() {
  const result = run("brew", ["--prefix"]);
  return result.status === 0 ? result.stdout.trim() || null : null;
}

function copyPathIfPresent(source, destination) {
  if (!existsSync(source)) return false;
  const stats = statSync(source);
  if (stats.isDirectory()) {
    cpSync(source, destination, {
      recursive: true,
      force: true,
      // Follow symlinks to copy actual files instead of broken links
      // (Homebrew GStreamer installs some plugins as symlinks).
      dereference: true,
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
  if (!isExistingDirectory(sourceDir)) return;
  mkdirSync(destinationDir, { recursive: true });
  for (const name of readdirSync(sourceDir)) {
    if (pattern.test(name)) copyFileSync(join(sourceDir, name), join(destinationDir, name));
  }
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
    isExistingFile(join(candidate, "bin", "gstreamer-1.0-0.dll"))
    && isExistingDirectory(join(candidate, "lib", "gstreamer-1.0")),
  );
  if (!sdkRoot) throw new Error("GStreamer MSVC x86_64 runtime was not found. Install the runtime/development MSI or pass --sdk-root.");
  return sdkRoot;
}

function macosCandidates(explicitSdkRoot) {
  return [
    explicitSdkRoot,
    process.env.GSTREAMER_1_0_ROOT_MACOS,
    "/Library/Frameworks/GStreamer.framework/Versions/1.0",
    "/Library/Frameworks/GStreamer.framework/Versions/Current",
    brewPrefix(),
    "/opt/homebrew",
    "/usr/local",
  ].filter(Boolean);
}

function validateMacosRoot(candidate) {
  return isExistingFile(join(candidate, "lib", "pkgconfig", "gstreamer-1.0.pc"))
    && isExistingFile(join(candidate, "lib", "libgstreamer-1.0.dylib"))
    && isExistingDirectory(join(candidate, "lib", "gstreamer-1.0"));
}

function resolveMacosRuntimeRoot(explicitSdkRoot) {
  const runtimeRoot = macosCandidates(explicitSdkRoot).find(validateMacosRoot);
  if (!runtimeRoot) {
    throw new Error("GStreamer macOS runtime was not found. Install the official runtime/devel .pkg packages or Homebrew packages, or pass --sdk-root.");
  }
  return runtimeRoot;
}

function writeMetadata(destination, source, platform) {
  writeFileSync(
    join(destination, "OPENNOW-GSTREAMER-RUNTIME.txt"),
    [
      "OpenNOW private GStreamer runtime bundle",
      `Source: ${source}`,
      `Generated: ${new Date().toISOString()}`,
      `Platform: ${platform}`,
      "Scope: native streamer child process only",
      "",
      "This directory is loaded only for the native streamer child process. Keep the private layout intact.",
      "",
    ].join("\n"),
  );
}

const WINDOWS_LOADER_DLLS = [
  "gstreamer-1.0-0.dll",
  "glib-2.0-0.dll",
  "gobject-2.0-0.dll",
  "gio-2.0-0.dll",
  "gmodule-2.0-0.dll",
  "gthread-2.0-0.dll",
  "intl-8.dll",
  "ffi-8.dll",
  "pcre2-8-0.dll",
  "orc-0.4-0.dll",
  "winpthread-1.dll",
  "zlib1.dll",
];

const WINDOWS_VC_RUNTIME_DLLS = [
  "vcruntime140.dll",
  "vcruntime140_1.dll",
  "msvcp140.dll",
  "msvcp140_1.dll",
  "msvcp140_2.dll",
  "msvcp140_atomic_wait.dll",
  "msvcp140_codecvt_ids.dll",
  "concrt140.dll",
];

function windowsVcRuntimeSearchDirs() {
  return [
    process.env.VCToolsRedistDir ? join(process.env.VCToolsRedistDir, "x64", "Microsoft.VC143.CRT") : null,
    process.env.VCToolsRedistDir ? join(process.env.VCToolsRedistDir, "x64", "Microsoft.VC142.CRT") : null,
    process.env.VCToolsRedistDir ? join(process.env.VCToolsRedistDir, "x64", "Microsoft.VC141.CRT") : null,
    process.env.SystemRoot ? join(process.env.SystemRoot, "System32") : null,
    ...String(process.env.PATH ?? "").split(delimiter),
  ].filter(Boolean);
}

function copyWindowsLoaderDlls({ sdkRoot, destination, binary }) {
  const executableDir = binary ? dirname(binary) : dirname(destination);
  const sdkBin = join(sdkRoot, "bin");
  const copiedLoader = [];
  const copiedVc = [];

  for (const name of WINDOWS_LOADER_DLLS) {
    const source = join(sdkBin, name);
    if (!isExistingFile(source)) continue;
    copyFileSync(source, join(executableDir, name));
    copiedLoader.push(name);
  }

  const vcSearchDirs = windowsVcRuntimeSearchDirs();
  for (const name of WINDOWS_VC_RUNTIME_DLLS) {
    const source = vcSearchDirs.map((dir) => join(dir, name)).find(isExistingFile);
    if (!source) continue;
    copyFileSync(source, join(executableDir, name));
    copyFileSync(source, join(destination, "bin", name));
    copiedVc.push(name);
  }

  console.log(`Copied Windows loader DLLs next to native streamer: ${copiedLoader.length ? copiedLoader.join(", ") : "none"}.`);
  console.log(`Copied Windows VC runtime DLLs: ${copiedVc.length ? copiedVc.join(", ") : "none found"}.`);
}

function bundleWindowsRuntime({ sdkRoot, destination, binary }) {
  const resolvedSdkRoot = resolveWindowsSdkRoot(sdkRoot);
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });
  const copied = [
    copyPathIfPresent(join(resolvedSdkRoot, "bin"), join(destination, "bin")),
    copyPathIfPresent(join(resolvedSdkRoot, "lib", "gstreamer-1.0"), join(destination, "lib", "gstreamer-1.0")),
    copyPathIfPresent(join(resolvedSdkRoot, "lib", "gio", "modules"), join(destination, "lib", "gio", "modules")),
    copyPathIfPresent(join(resolvedSdkRoot, "libexec", "gstreamer-1.0"), join(destination, "libexec", "gstreamer-1.0")),
    copyPathIfPresent(join(resolvedSdkRoot, "share", "gstreamer-1.0"), join(destination, "share", "gstreamer-1.0")),
    copyPathIfPresent(join(resolvedSdkRoot, "share", "glib-2.0"), join(destination, "share", "glib-2.0")),
    copyPathIfPresent(join(resolvedSdkRoot, "etc"), join(destination, "etc")),
  ].filter(Boolean).length;
  copyMatchingFiles(resolvedSdkRoot, destination, /^(copying|license|readme)/i);
  copyWindowsLoaderDlls({ sdkRoot: resolvedSdkRoot, destination, binary });
  writeMetadata(destination, resolvedSdkRoot, "win32");
  console.log(`Bundled GStreamer runtime from ${resolvedSdkRoot} to ${destination} (${copied} paths).`);
}

function walkFiles(root) {
  if (!isExistingDirectory(root)) return [];
  const out = [];
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop();
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      const stats = statSync(path);
      if (stats.isDirectory()) stack.push(path);
      else if (stats.isFile()) out.push(path);
    }
  }
  return out;
}

function isMachO(path) {
  try {
    const buffer = readFileSync(path, { flag: "r" });
    if (buffer.length < 4) return false;
    const magic = buffer.readUInt32BE(0);
    return [0xfeedface, 0xfeedfacf, 0xcafebabe, 0xcafebabf, 0xbebafeca, 0xcffaedfe, 0xcefaedfe].includes(magic);
  } catch {
    return false;
  }
}

function dylibName(ref) {
  return ref.split("/").pop();
}

function isPathInside(path, parent) {
  const relativePath = relative(parent, path);
  return relativePath === "" || (!relativePath.startsWith("..") && !relativePath.startsWith("/"));
}

function posixPath(path) {
  return path.split(/[\\/]+/).filter(Boolean).join("/");
}

function relocationTarget(file, destination, libDir, dep) {
  const name = dylibName(dep);
  if (!name) return null;
  if (!isPathInside(file, destination)) return `@executable_path/gstreamer/lib/${name}`;
  const relativeLibDir = posixPath(relative(dirname(file), libDir));
  return relativeLibDir ? `@loader_path/${relativeLibDir}/${name}` : `@loader_path/${name}`;
}

function shouldRewriteDependency(dep, roots, bundledLibs) {
  const name = dylibName(dep);
  if (!name || !bundledLibs.has(name)) return false;
  if (dep.startsWith("@rpath/") || dep.startsWith("@loader_path/") || dep.startsWith("@executable_path/")) return bundledLibs.has(name);
  return roots.some((root) => dep === join(root, "lib", name) || dep.startsWith(`${root}/`) || dep.includes("GStreamer.framework"));
}

function macosExternalRoots(sourceRoot) {
  return [
    sourceRoot,
    "/Library/Frameworks/GStreamer.framework/Versions/1.0",
    "/Library/Frameworks/GStreamer.framework/Versions/Current",
    "/Library/Frameworks/GStreamer.framework",
    "/opt/homebrew",
    "/usr/local",
  ];
}

function runInstallNameTool(args, file) {
  const result = run("install_name_tool", args, { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`install_name_tool failed for ${file}: install_name_tool ${args.join(" ")}`);
  }
}

function patchMachO(file, destination, sourceRoot, libDir, bundledLibs) {
  if (!isMachO(file) || !commandAvailable("otool") || !commandAvailable("install_name_tool")) return;
  const output = run("otool", ["-L", file]);
  if (output.status !== 0) return;
  const roots = macosExternalRoots(sourceRoot);
  for (const line of output.stdout.split(/\r?\n/).slice(1)) {
    const dep = line.trim().split(/\s+/)[0];
    if (!dep || !shouldRewriteDependency(dep, roots, bundledLibs)) continue;
    const target = relocationTarget(file, destination, libDir, dep);
    if (target && target !== dep) runInstallNameTool(["-change", dep, target, file], file);
  }
  if (isPathInside(file, libDir) && extname(file) === ".dylib") {
    runInstallNameTool(["-id", `@rpath/${dylibName(file)}`, file], file);
  }
}

function isExternalGstreamerDependency(dep, roots, bundledLibs) {
  const name = dylibName(dep);
  if (!name || !bundledLibs.has(name)) return false;
  if (dep.startsWith("@")) return false;
  return roots.some((root) => dep === join(root, "lib", name) || dep.startsWith(`${root}/`) || dep.includes("GStreamer.framework"));
}

function validatePackagedBinaryRelocation(binary, sourceRoot, bundledLibs) {
  if (!binary || !isMachO(binary) || !commandAvailable("otool")) return;
  const output = run("otool", ["-L", binary]);
  if (output.status !== 0) {
    throw new Error(`otool -L failed for packaged native streamer: ${binary}`);
  }
  const roots = macosExternalRoots(sourceRoot);
  const leakedDeps = output.stdout
    .split(/\r?\n/)
    .slice(1)
    .map((line) => line.trim().split(/\s+/)[0])
    .filter((dep) => dep && isExternalGstreamerDependency(dep, roots, bundledLibs));
  if (leakedDeps.length > 0) {
    throw new Error(
      `Packaged native streamer still references external GStreamer dependencies after relocation: ${leakedDeps.join(", ")}`,
    );
  }
}

function bundleMacosRuntime({ sdkRoot, destination, binary }) {
  const resolvedRuntimeRoot = resolveMacosRuntimeRoot(sdkRoot);
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });
  const copied = [
    copyPathIfPresent(join(resolvedRuntimeRoot, "bin"), join(destination, "bin")),
    copyPathIfPresent(join(resolvedRuntimeRoot, "lib", "gstreamer-1.0"), join(destination, "lib", "gstreamer-1.0")),
    copyPathIfPresent(join(resolvedRuntimeRoot, "lib", "gio", "modules"), join(destination, "lib", "gio", "modules")),
    copyPathIfPresent(join(resolvedRuntimeRoot, "libexec", "gstreamer-1.0"), join(destination, "libexec", "gstreamer-1.0")),
    copyPathIfPresent(join(resolvedRuntimeRoot, "share"), join(destination, "share")),
    copyPathIfPresent(join(resolvedRuntimeRoot, "etc"), join(destination, "etc")),
  ].filter(Boolean).length;
  copyMatchingFiles(join(resolvedRuntimeRoot, "lib"), join(destination, "lib"), /\.(dylib|so)$/);
  copyMatchingFiles(resolvedRuntimeRoot, destination, /^(copying|license|readme)/i);
  const libDir = join(destination, "lib");
  const bundledLibs = new Set(readdirSync(libDir).filter((name) => name.endsWith(".dylib") || name.endsWith(".so")));
  for (const file of [...walkFiles(destination), binary].filter(Boolean)) {
    try { chmodSync(file, statSync(file).mode | 0o200); } catch {}
    patchMachO(file, destination, resolvedRuntimeRoot, libDir, bundledLibs);
  }
  validatePackagedBinaryRelocation(binary, resolvedRuntimeRoot, bundledLibs);
  writeMetadata(destination, resolvedRuntimeRoot, "darwin");
  console.log(`Bundled GStreamer runtime from ${resolvedRuntimeRoot} to ${destination} (${copied} paths plus dylibs).`);
}

const args = parseArgs(process.argv.slice(2));
const destination = args.get("dest");
if (!destination) {
  console.error("Usage: node scripts/bundle-gstreamer-runtime.mjs --dest <runtime-dir> [--sdk-root <path>] [--binary <path>]");
  process.exit(1);
}

try {
  const resolvedDestination = resolve(packageRoot, destination);
  if (process.platform === "win32") {
    bundleWindowsRuntime({
      sdkRoot: args.get("sdk-root"),
      destination: resolvedDestination,
      binary: args.get("binary") ? resolve(packageRoot, args.get("binary")) : null,
    });
  } else if (process.platform === "darwin") {
    bundleMacosRuntime({
      sdkRoot: args.get("sdk-root"),
      destination: resolvedDestination,
      binary: args.get("binary") ? resolve(packageRoot, args.get("binary")) : null,
    });
  } else {
    throw new Error(
      `Private GStreamer runtime collection is intentionally unsupported on Linux (${process.platform}). Linux builds use distro GStreamer packages because AppImage/private bundling is unreliable across glibc, libdrm/VAAPI/Vulkan, and GPU driver stacks.`,
    );
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
