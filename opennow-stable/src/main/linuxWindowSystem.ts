export type LinuxWindowSystem = "x11" | "wayland";

export function resolveLinuxWindowSystem(
  ozonePlatform: string | undefined,
  env: NodeJS.ProcessEnv,
): LinuxWindowSystem {
  const requested = ozonePlatform?.trim().toLowerCase();
  if (requested === "wayland") {
    return "wayland";
  }
  if (requested === "x11") {
    return "x11";
  }

  const sessionType = env.XDG_SESSION_TYPE?.trim().toLowerCase();
  if (sessionType === "wayland" && env.WAYLAND_DISPLAY?.trim()) {
    return "wayland";
  }
  if (!env.DISPLAY?.trim() && env.WAYLAND_DISPLAY?.trim()) {
    return "wayland";
  }
  return "x11";
}
