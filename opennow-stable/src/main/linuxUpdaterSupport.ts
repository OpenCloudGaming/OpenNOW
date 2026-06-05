import { accessSync, constants, readFileSync } from "node:fs";
import { delimiter, isAbsolute, join } from "node:path";

export type LinuxUpdaterPackageKind = "appimage" | "deb" | "native" | "unsupported";

export interface LinuxUpdaterSupport {
  packageKind: LinuxUpdaterPackageKind;
  supported: boolean;
  message?: string;
}

interface LinuxUpdaterSupportOptions {
  platform?: NodeJS.Platform;
  env?: NodeJS.ProcessEnv;
  commandExists?: (command: string, env: NodeJS.ProcessEnv) => boolean;
  readOsRelease?: () => string | null;
}

const UNSUPPORTED_LINUX_UPDATER_MESSAGE =
  "Automatic Linux updates are not available for this install on this system. Download the AppImage from GitHub Releases, or use a Debian/Ubuntu package on a Debian-compatible system with dpkg or apt.";

function hasValue(value: string | undefined): boolean {
  return Boolean(value?.trim());
}

function defaultReadOsRelease(): string | null {
  try {
    return readFileSync("/etc/os-release", "utf8");
  } catch {
    return null;
  }
}

function unquoteOsReleaseValue(value: string): string {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith("\"") && trimmed.endsWith("\"")) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function parseOsRelease(content: string | null): Record<string, string> | null {
  if (!content) {
    return null;
  }

  const entries: Record<string, string> = {};
  for (const line of content.split(/\r?\n/)) {
    const match = line.match(/^([A-Z][A-Z0-9_]*?)=(.*)$/);
    if (match) {
      entries[match[1]] = unquoteOsReleaseValue(match[2]);
    }
  }

  return entries;
}

function isDebianCompatible(osRelease: Record<string, string> | null): boolean {
  if (!osRelease) {
    return true;
  }

  const ids = [osRelease.ID, osRelease.ID_LIKE]
    .filter((value): value is string => Boolean(value))
    .flatMap((value) => value.split(/\s+/))
    .map((value) => value.toLowerCase());

  return ids.some((id) => id === "debian" || id === "ubuntu");
}

export function linuxCommandExists(command: string, env: NodeJS.ProcessEnv = process.env): boolean {
  if (isAbsolute(command)) {
    try {
      accessSync(command, constants.X_OK);
      return true;
    } catch {
      return false;
    }
  }

  for (const directory of (env.PATH ?? "").split(delimiter)) {
    if (!directory) {
      continue;
    }

    try {
      accessSync(join(directory, command), constants.X_OK);
      return true;
    } catch {
      continue;
    }
  }

  return false;
}

export function getLinuxUpdaterSupport(options: LinuxUpdaterSupportOptions = {}): LinuxUpdaterSupport {
  const platform = options.platform ?? process.platform;
  if (platform !== "linux") {
    return {
      packageKind: "native",
      supported: true,
    };
  }

  const env = options.env ?? process.env;
  if (hasValue(env.APPIMAGE)) {
    return {
      packageKind: "appimage",
      supported: true,
    };
  }

  const commandExists = options.commandExists ?? linuxCommandExists;
  if (
    (commandExists("dpkg", env) || commandExists("apt", env)) &&
    isDebianCompatible(parseOsRelease((options.readOsRelease ?? defaultReadOsRelease)()))
  ) {
    return {
      packageKind: "deb",
      supported: true,
    };
  }

  return {
    packageKind: "unsupported",
    supported: false,
    message: UNSUPPORTED_LINUX_UPDATER_MESSAGE,
  };
}
