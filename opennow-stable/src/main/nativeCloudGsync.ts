import { execFile } from "node:child_process";

import {
  DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
  DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
  normalizeCloudGsyncOverride,
  unsupportedNativeCloudGsyncCapabilities,
  type NativeCloudGsyncCapabilities,
} from "@shared/cloudGsync";

interface WindowsDisplayProbe {
  adapters?: string[];
  monitors?: string[];
}

function assumedWindowsCapabilities(reason: string): NativeCloudGsyncCapabilities {
  return {
    platformSupportsCloudGsync: true,
    isVrrCapableDisplay: true,
    isGsyncDisplay: true,
    minimumFpsForCloudGsync: DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr: DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: "assumed",
    reason,
  };
}

function execFileText(file: string, args: string[], timeoutMs: number): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(file, args, { timeout: timeoutMs, windowsHide: true }, (error, stdout) => {
      if (error) {
        reject(error);
        return;
      }
      resolve(stdout);
    });
  });
}

async function probeWindowsDisplayMetadata(): Promise<WindowsDisplayProbe> {
  const script = `
$ErrorActionPreference = "SilentlyContinue"
$adapters = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name } | Where-Object { $_ })
$monitors = @(Get-CimInstance -Namespace root\\wmi -ClassName WmiMonitorID | ForEach-Object {
  $chars = @($_.UserFriendlyName | Where-Object { $_ -gt 0 })
  if ($chars.Count -gt 0) { -join ($chars | ForEach-Object { [char]$_ }) }
} | Where-Object { $_ })
[Console]::Out.Write((ConvertTo-Json -Compress @{ adapters = $adapters; monitors = $monitors }))
`;
  const stdout = await execFileText("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-Command",
    script,
  ], 2500);
  return JSON.parse(stdout || "{}") as WindowsDisplayProbe;
}

export async function getNativeCloudGsyncCapabilities(
  overrideValue: string | null | undefined = process.env.OPENNOW_NATIVE_CLOUD_GSYNC,
): Promise<NativeCloudGsyncCapabilities> {
  const override = normalizeCloudGsyncOverride(overrideValue);

  if (override === "1") {
    return assumedWindowsCapabilities("OPENNOW_NATIVE_CLOUD_GSYNC=1");
  }

  if (process.platform !== "win32") {
    return unsupportedNativeCloudGsyncCapabilities("unsupported-platform");
  }

  try {
    const probe = await probeWindowsDisplayMetadata();
    const adapters = Array.isArray(probe.adapters) ? probe.adapters.filter(Boolean) : [];
    const monitors = Array.isArray(probe.monitors) ? probe.monitors.filter(Boolean) : [];
    const hasNvidiaAdapter = adapters.some((name) => /nvidia/i.test(name));

    if (!hasNvidiaAdapter) {
      return {
        ...unsupportedNativeCloudGsyncCapabilities("no-nvidia-adapter"),
        reason: `no-nvidia-adapter adapters=${adapters.join(",") || "none"} monitors=${monitors.join(",") || "none"}`,
      };
    }

    // Node/Electron has no NVAPI/DXGI VRR binding here. Match the official client's
    // vrrDisplayWar behavior: on Windows with NVIDIA present, treat likely G-Sync
    // setups as VRR-capable when exact detection is unavailable.
    return assumedWindowsCapabilities(
      `nvidia-adapter-assumed-vrr adapters=${adapters.join(",")} monitors=${monitors.join(",") || "unknown"}`,
    );
  } catch (error) {
    return {
      ...unsupportedNativeCloudGsyncCapabilities("detection-failed"),
      reason: `detection-failed ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}
