import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

const require = createRequire(import.meta.url);

const electronPackageJson = require.resolve("electron/package.json");
const electronDir = path.dirname(electronPackageJson);
const electronPackage = require(electronPackageJson);
const electronChecksums = require(path.join(electronDir, "checksums.json"));
const electronInstallScript = path.join(electronDir, "install.js");
const electronPathFile = path.join(electronDir, "path.txt");
const platform = process.env.ELECTRON_INSTALL_PLATFORM ?? process.env.npm_config_platform ?? process.platform;
const arch = process.env.ELECTRON_INSTALL_ARCH ?? process.env.npm_config_arch ?? process.arch;

function getPlatformPath() {
  switch (platform) {
    case "mas":
    case "darwin":
      return "Electron.app/Contents/MacOS/Electron";
    case "freebsd":
    case "openbsd":
    case "linux":
      return "electron";
    case "win32":
      return "electron.exe";
    default:
      throw new Error(`Electron builds are not available on platform: ${platform}`);
  }
}

function readElectronPath() {
  if (!existsSync(electronPathFile)) {
    return null;
  }

  const electronPath = readFileSync(electronPathFile, "utf8").trim();
  return electronPath || null;
}

function hasElectronBinary() {
  const electronPath = readElectronPath();
  if (!electronPath) {
    return false;
  }

  const distRoot = process.env.ELECTRON_OVERRIDE_DIST_PATH ?? path.join(electronDir, "dist");
  const installedVersionPath = path.join(distRoot, "version");
  if (!existsSync(installedVersionPath)) {
    return false;
  }

  const installedVersion = readFileSync(installedVersionPath, "utf8").trim().replace(/^v/, "");
  if (installedVersion !== electronPackage.version) {
    return false;
  }

  return existsSync(path.join(distRoot, electronPath));
}

if (hasElectronBinary()) {
  process.exit(0);
}

if (!existsSync(electronInstallScript)) {
  console.error(`Electron install script not found: ${electronInstallScript}`);
  process.exit(1);
}

console.log("Installing Electron runtime binary...");

const installResult = spawnSync(process.execPath, [electronInstallScript], {
  stdio: "inherit",
});

if (installResult.status !== 0) {
  process.exit(installResult.status ?? 1);
}

if (!hasElectronBinary()) {
  console.log("Electron package installer did not produce a runtime binary; using archive fallback...");

  const { downloadArtifact } = require("@electron/get");
  const zipPath = await downloadArtifact({
    version: electronPackage.version,
    artifactName: "electron",
    force: process.env.force_no_cache === "true",
    cacheRoot: process.env.electron_config_cache,
    checksums:
      process.env.electron_use_remote_checksums || process.env.npm_config_electron_use_remote_checksums
        ? undefined
        : electronChecksums,
    platform,
    arch,
  });

  const distRoot = process.env.ELECTRON_OVERRIDE_DIST_PATH ?? path.join(electronDir, "dist");
  const resolvedDistRoot = path.resolve(distRoot);
  const resolvedElectronDir = path.resolve(electronDir);

  if (!process.env.ELECTRON_OVERRIDE_DIST_PATH && !resolvedDistRoot.startsWith(resolvedElectronDir)) {
    console.error(`Refusing to rewrite unexpected Electron dist path: ${resolvedDistRoot}`);
    process.exit(1);
  }

  rmSync(resolvedDistRoot, { recursive: true, force: true });
  mkdirSync(resolvedDistRoot, { recursive: true });

  const tarResult = spawnSync("tar", ["-xf", zipPath, "-C", resolvedDistRoot], {
    stdio: "inherit",
    shell: false,
  });

  if (tarResult.status !== 0 && platform === "win32") {
    const expandResult = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
        zipPath,
        resolvedDistRoot,
      ],
      { stdio: "inherit" },
    );

    if (expandResult.status !== 0) {
      process.exit(expandResult.status ?? 1);
    }
  } else if (tarResult.status !== 0) {
    process.exit(tarResult.status ?? 1);
  }

  const extractedTypeDef = path.join(resolvedDistRoot, "electron.d.ts");
  if (existsSync(extractedTypeDef)) {
    renameSync(extractedTypeDef, path.join(electronDir, "electron.d.ts"));
  }

  writeFileSync(electronPathFile, getPlatformPath());
}

if (!hasElectronBinary()) {
  console.error("Electron install completed, but the runtime binary is still missing.");
  process.exit(1);
}

console.log(`Electron ${electronPackage.version} runtime is installed for ${platform}-${arch}.`);
