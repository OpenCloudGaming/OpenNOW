import { Install, Status, StreamValues } from "./backend";

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function resolutionLabel(value: string): string {
  return value.replace("x", "×") + (value === "1280x800" ? " (native)" : "");
}

export function streamSummary(values: StreamValues): string {
  return [values.resolution.replace("x", "×"), values.fps + " fps", values.maxBitrateMbps + " Mbps"].join(" · ");
}

export function installKind(install: Install): string {
  if (install.kind === "flatpak") return install.args.includes("--system") ? "Flatpak (system)" : "Flatpak (user)";
  if (install.kind === "appimage") return "AppImage";
  if (install.kind === "native") return "System package";
  return "Custom";
}

export function installSummary(install: Install): string {
  const kind = install.kind === "flatpak" ? "Flatpak" : installKind(install);
  return install.version ? kind + " · " + install.version : kind;
}

export function homePath(path: string): string {
  return path.replace(/^\/home\/[^/]+/, "~");
}

export function settingsFile(install: Install): string {
  return homePath(install.data_dir + "/settings.json");
}

export type Phase = "checking" | "installing" | "missing" | "running" | "signedOut" | "update" | "ready";

export function phase(status: Status | null): Phase {
  if (!status) return "checking";
  if (status.installProgress) return "installing";
  if (!status.install) return "missing";
  if (status.running) return "running";
  if (!status.signedIn) return "signedOut";
  if (status.release?.updateAvailable) return "update";
  return "ready";
}
