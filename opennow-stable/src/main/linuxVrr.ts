import { execFile } from "node:child_process";

export type LinuxVrrWindowSystem = "wayland" | "x11" | "unknown";

export interface LinuxVrrProbe {
  platformSupported: boolean;
  displayCapable: boolean;
  active: boolean;
  gsyncDisplay: boolean;
  detection: "native" | "assumed" | "unsupported";
  windowSystem: LinuxVrrWindowSystem;
  displayName?: string;
  refreshHz?: number;
  reason: string;
}

interface HyprlandMonitor {
  name?: unknown;
  description?: unknown;
  refreshRate?: unknown;
  focused?: unknown;
  disabled?: unknown;
  dpmsStatus?: unknown;
  vrr?: unknown;
}

interface XrandrOutput {
  name: string;
  primary: boolean;
  refreshHz?: number;
  vrrCapable: boolean;
}

function execFileText(file: string, args: string[], timeoutMs = 2_500): Promise<string> {
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

export function resolveLinuxVrrWindowSystem(
  env: NodeJS.ProcessEnv = process.env,
): LinuxVrrWindowSystem {
  const sessionType = env.XDG_SESSION_TYPE?.trim().toLowerCase();
  if (sessionType === "wayland" || env.WAYLAND_DISPLAY) return "wayland";
  if (sessionType === "x11" || env.DISPLAY) return "x11";
  return "unknown";
}

export function parseHyprlandVrr(
  optionJson: string,
  monitorsJson: string,
): LinuxVrrProbe {
  const option = JSON.parse(optionJson || "{}") as { int?: unknown };
  const rawMonitors = JSON.parse(monitorsJson || "[]") as HyprlandMonitor[];
  const monitors = Array.isArray(rawMonitors)
    ? rawMonitors.filter((monitor) => monitor.disabled !== true && monitor.dpmsStatus !== false)
    : [];
  const monitor = monitors.find((candidate) => candidate.focused === true) ?? monitors[0];
  const refreshHz = typeof monitor?.refreshRate === "number" && Number.isFinite(monitor.refreshRate)
    ? monitor.refreshRate
    : undefined;
  const displayName = [monitor?.description, monitor?.name]
    .find((value): value is string => typeof value === "string" && value.trim().length > 0);
  const vrrMode = typeof option.int === "number" ? option.int : 0;
  // Hyprland mode 3 additionally requires the Wayland content-type protocol.
  // SDL2 does not advertise game/video content, so only always-on (1) and
  // fullscreen-only (2) can reliably activate VRR for this output window.
  const compositorAllowsVrr = vrrMode === 1 || vrrMode === 2;
  const highRefreshDisplay = refreshHz !== undefined && refreshHz > 60;
  const displayCapable = compositorAllowsVrr && highRefreshDisplay;
  const active = displayCapable && monitor?.vrr === true;

  let reason: string;
  if (!monitor) {
    reason = "hyprland-no-active-display";
  } else if (!highRefreshDisplay) {
    reason = `hyprland-display-refresh-unsupported display=${displayName ?? "unknown"} refresh=${refreshHz ?? "unknown"}`;
  } else if (vrrMode === 3) {
    reason = `hyprland-vrr-content-type-required display=${displayName ?? "unknown"} refresh=${refreshHz?.toFixed(2) ?? "unknown"}`;
  } else if (!compositorAllowsVrr) {
    reason = `hyprland-vrr-disabled display=${displayName ?? "unknown"} refresh=${refreshHz?.toFixed(2) ?? "unknown"}`;
  } else {
    reason = `hyprland-vrr-allowed display=${displayName ?? "unknown"} refresh=${refreshHz?.toFixed(2) ?? "unknown"} active=${active}`;
  }

  return {
    platformSupported: true,
    displayCapable,
    active,
    gsyncDisplay: false,
    detection: "native",
    windowSystem: "wayland",
    displayName,
    refreshHz,
    reason,
  };
}

export function parseXrandrOutputs(stdout: string): XrandrOutput[] {
  const outputs: XrandrOutput[] = [];
  let current: XrandrOutput | undefined;

  for (const line of stdout.split(/\r?\n/)) {
    const connected = /^(\S+)\s+connected(?:\s+(primary))?\b/.exec(line);
    if (connected) {
      current = {
        name: connected[1],
        primary: connected[2] === "primary",
        vrrCapable: false,
      };
      outputs.push(current);
      continue;
    }
    if (!current) continue;

    const vrr = /^\s+(?:vrr_capable|VARIABLE_REFRESH):\s*(\d+)/i.exec(line);
    if (vrr) {
      current.vrrCapable = Number(vrr[1]) > 0;
      continue;
    }
    const activeMode = /^\s+\d+x\d+\s+(.+)$/.exec(line);
    if (activeMode) {
      for (const token of activeMode[1].trim().split(/\s+/)) {
        if (!token.includes("*")) continue;
        const refreshHz = Number.parseFloat(token.replace(/[^\d.]/g, ""));
        if (Number.isFinite(refreshHz)) current.refreshHz = refreshHz;
      }
    }
  }
  return outputs;
}

export function resolveX11Vrr(
  stdout: string,
  hasNvidiaGpu: boolean,
): LinuxVrrProbe {
  const outputs = parseXrandrOutputs(stdout);
  const display = outputs.find((output) => output.primary) ?? outputs[0];
  const explicitlyCapable = display?.vrrCapable === true;
  // NVIDIA's Xorg driver exposes G-SYNC through NV-CONTROL rather than the
  // standard RandR vrr_capable property. Match the official client's
  // vrrDisplayWar only for its supported one-display, high-refresh topology.
  const nvidiaAssumedCapable = hasNvidiaGpu
    && outputs.length === 1
    && (display?.refreshHz ?? 0) > 60;
  const displayCapable = explicitlyCapable || nvidiaAssumedCapable;
  const detection = explicitlyCapable ? "native" : nvidiaAssumedCapable ? "assumed" : "unsupported";
  const reason = displayCapable
    ? `x11-vrr-${explicitlyCapable ? "detected" : "nvidia-assumed"} display=${display?.name ?? "unknown"} refresh=${display?.refreshHz ?? "unknown"}`
    : `x11-vrr-unavailable displays=${outputs.length} nvidia=${hasNvidiaGpu} refresh=${display?.refreshHz ?? "unknown"}`;

  return {
    platformSupported: true,
    displayCapable,
    active: displayCapable,
    gsyncDisplay: nvidiaAssumedCapable,
    detection,
    windowSystem: "x11",
    displayName: display?.name,
    refreshHz: display?.refreshHz,
    reason,
  };
}

export async function probeLinuxVrr(
  env: NodeJS.ProcessEnv = process.env,
): Promise<LinuxVrrProbe> {
  const windowSystem = resolveLinuxVrrWindowSystem(env);
  const desktop = `${env.XDG_CURRENT_DESKTOP ?? ""} ${env.DESKTOP_SESSION ?? ""}`;

  if (windowSystem === "wayland" && /hyprland/i.test(desktop)) {
    try {
      const [option, monitors] = await Promise.all([
        execFileText("hyprctl", ["getoption", "misc:vrr", "-j"]),
        execFileText("hyprctl", ["monitors", "-j"]),
      ]);
      return parseHyprlandVrr(option, monitors);
    } catch (error) {
      return {
        platformSupported: true,
        displayCapable: false,
        active: false,
        gsyncDisplay: false,
        detection: "unsupported",
        windowSystem,
        reason: `hyprland-vrr-probe-failed ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  if (windowSystem === "x11") {
    try {
      const [xrandr, nvidia] = await Promise.all([
        execFileText("xrandr", ["--props", "--current"]),
        execFileText("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"])
          .then((value) => /nvidia/i.test(value))
          .catch(() => false),
      ]);
      return resolveX11Vrr(xrandr, nvidia);
    } catch (error) {
      return {
        platformSupported: true,
        displayCapable: false,
        active: false,
        gsyncDisplay: false,
        detection: "unsupported",
        windowSystem,
        reason: `x11-vrr-probe-failed ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  return {
    platformSupported: false,
    displayCapable: false,
    active: false,
    gsyncDisplay: false,
    detection: "unsupported",
    windowSystem,
    reason: windowSystem === "wayland"
      ? "wayland-compositor-vrr-probe-unsupported"
      : "linux-window-system-unknown",
  };
}
