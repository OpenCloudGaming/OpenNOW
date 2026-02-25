import { screen } from "electron";
import { execSync } from "node:child_process";

export interface OsHdrInfo {
  osHdrEnabled: boolean;
  platform: string;
}

function detectWindows(): OsHdrInfo {
  try {
    const result = execSync(
      'powershell -NoProfile -Command "Get-ItemPropertyValue -Path \'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\AdvancedColor\' -Name AdvancedColorEnabled 2>$null"',
      { encoding: "utf-8", timeout: 3000 },
    ).trim();

    if (result === "1") {
      return { osHdrEnabled: true, platform: "windows" };
    }

    const displays = screen.getAllDisplays();
    for (const display of displays) {
      const colorSpace = (display as unknown as Record<string, unknown>).colorSpace;
      if (typeof colorSpace === "string" && colorSpace.toLowerCase().includes("hdr")) {
        return { osHdrEnabled: true, platform: "windows" };
      }
      if (display.colorDepth > 24) {
        return { osHdrEnabled: true, platform: "windows" };
      }
    }

    return { osHdrEnabled: false, platform: "windows" };
  } catch {
    const displays = screen.getAllDisplays();
    const hasHighDepth = displays.some((d) => d.colorDepth > 24);
    return { osHdrEnabled: hasHighDepth, platform: "windows" };
  }
}

function detectMacOS(): OsHdrInfo {
  try {
    const result = execSync(
      "system_profiler SPDisplaysDataType 2>/dev/null | grep -i 'HDR\\|EDR\\|XDR'",
      { encoding: "utf-8", timeout: 3000 },
    ).trim();

    if (result.length > 0) {
      return { osHdrEnabled: true, platform: "macos" };
    }
    return { osHdrEnabled: false, platform: "macos" };
  } catch {
    return { osHdrEnabled: false, platform: "macos" };
  }
}

function detectLinux(): OsHdrInfo {
  try {
    const sessionType = process.env.XDG_SESSION_TYPE ?? "";
    const isWayland = sessionType.toLowerCase() === "wayland";

    if (!isWayland) {
      return { osHdrEnabled: false, platform: "linux" };
    }

    const result = execSync(
      "kscreen-doctor --outputs 2>/dev/null | grep -i 'hdr'",
      { encoding: "utf-8", timeout: 3000 },
    ).trim();

    return { osHdrEnabled: result.length > 0, platform: "linux" };
  } catch {
    return { osHdrEnabled: false, platform: "linux" };
  }
}

export function getOsHdrInfo(): OsHdrInfo {
  switch (process.platform) {
    case "win32":
      return detectWindows();
    case "darwin":
      return detectMacOS();
    case "linux":
      return detectLinux();
    default:
      return { osHdrEnabled: false, platform: "unknown" };
  }
}
