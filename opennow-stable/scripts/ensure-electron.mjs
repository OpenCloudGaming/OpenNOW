import { downloadArtifact } from "@electron/get";
import { spawnSync } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";
import { fileURLToPath } from "url";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const electronDir = path.join(rootDir, "node_modules", "electron");
const distDir = path.join(electronDir, "dist");
const pathFile = path.join(electronDir, "path.txt");
const platformPath = process.platform === "win32" ? "electron.exe" : "electron";

function isElectronInstalled() {
  try {
    const { version } = JSON.parse(fs.readFileSync(path.join(electronDir, "package.json"), "utf8"));
    const installedVersion = fs.readFileSync(path.join(distDir, "version"), "utf8").replace(/^v/, "").trim();
    const executable = path.join(distDir, platformPath);
    return installedVersion === version && fs.readFileSync(pathFile, "utf8") === platformPath && fs.existsSync(executable);
  } catch {
    return false;
  }
}

function extractZip(zipPath, destination) {
  fs.mkdirSync(destination, { recursive: true });

  if (process.platform === "win32") {
    const result = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        `Expand-Archive -Path '${zipPath.replace(/'/g, "''")}' -DestinationPath '${destination.replace(/'/g, "''")}' -Force`,
      ],
      { stdio: "inherit" },
    );
    if (result.status !== 0) {
      throw new Error(`Failed to extract Electron archive (exit ${result.status ?? "unknown"})`);
    }
    return;
  }

  const tarResult = spawnSync("tar", ["-xf", zipPath, "-C", destination], { stdio: "inherit" });
  if (tarResult.status === 0) {
    return;
  }

  const unzipResult = spawnSync("unzip", ["-oq", zipPath, "-d", destination], { stdio: "inherit" });
  if (unzipResult.status !== 0) {
    throw new Error("Failed to extract Electron archive with tar/unzip");
  }
}

async function ensureElectron() {
  if (!fs.existsSync(electronDir)) {
    throw new Error("Missing node_modules/electron. Run `bun install` first.");
  }

  if (isElectronInstalled()) {
    console.log("[ensure-electron] Electron binary already installed.");
    return;
  }

  const { version } = JSON.parse(fs.readFileSync(path.join(electronDir, "package.json"), "utf8"));
  const checksums = JSON.parse(fs.readFileSync(path.join(electronDir, "checksums.json"), "utf8"));

  console.log(`[ensure-electron] Downloading Electron ${version} for ${process.platform}-${process.arch}...`);
  const zipPath = await downloadArtifact({
    version,
    artifactName: "electron",
    platform: process.platform,
    arch: process.arch,
    checksums,
  });

  console.log(`[ensure-electron] Extracting ${zipPath}...`);
  extractZip(zipPath, distDir);

  const srcTypeDefPath = path.join(distDir, "electron.d.ts");
  const targetTypeDefPath = path.join(electronDir, "electron.d.ts");
  if (fs.existsSync(srcTypeDefPath)) {
    fs.renameSync(srcTypeDefPath, targetTypeDefPath);
  }

  await fs.promises.writeFile(pathFile, platformPath, "utf8");
  console.log("[ensure-electron] Electron ready.");
}

ensureElectron().catch((error) => {
  console.error("[ensure-electron] Failed:", error);
  process.exit(1);
});
