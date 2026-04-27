import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

export type LinuxPackageType = "deb" | "rpm" | "pacman";
export type UpdaterKind = "default" | "pacman";

interface UpdaterKindInput {
  platform: NodeJS.Platform;
  packageType: LinuxPackageType | null;
  hasCommand: (command: string) => boolean;
}

export function normalizeLinuxPackageType(value: string | null | undefined): LinuxPackageType | null {
  const normalized = value?.trim().toLowerCase();
  if (normalized === "deb" || normalized === "rpm" || normalized === "pacman") {
    return normalized;
  }

  return null;
}

export function readLinuxPackageType(resourcesPath = process.resourcesPath): LinuxPackageType | null {
  if (process.platform !== "linux") {
    return null;
  }

  try {
    const packageTypePath = path.join(resourcesPath, "package-type");
    if (!existsSync(packageTypePath)) {
      return null;
    }

    return normalizeLinuxPackageType(readFileSync(packageTypePath, "utf8"));
  } catch {
    return null;
  }
}

export function hasSystemCommand(command: string): boolean {
  const result = spawnSync("command", ["-v", command], {
    shell: true,
    stdio: "ignore",
  });

  return !result.error && result.status === 0;
}

export function chooseUpdaterKind(input: UpdaterKindInput): UpdaterKind {
  if (input.platform !== "linux" || input.packageType !== "deb") {
    return "default";
  }

  const canInstallDeb = input.hasCommand("dpkg") || input.hasCommand("apt");
  if (canInstallDeb || !input.hasCommand("pacman")) {
    return "default";
  }

  return "pacman";
}
