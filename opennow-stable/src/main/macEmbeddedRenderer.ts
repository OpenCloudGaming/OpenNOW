import { createRequire } from "node:module";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { app } from "electron";
import type { NativeRenderSurface } from "../shared/gfn";

export interface MacEmbeddedRendererResult {
  iosurfaceId: number;
}

interface NativeBinding {
  createSurface(
    windowHandle: string,
    width: number,
    height: number,
    scale: number
  ): { iosurfaceId: number };
  updateSurface(
    windowHandle: string,
    x: number,
    y: number,
    width: number,
    height: number,
    scale: number,
    visible: boolean
  ): void;
  destroySurface(): void;
  notifyFrameReady(): void;
}

const ENABLED_ENV = "OPENNOW_MACOS_EMBEDDED_RENDERER";
const PROTOTYPE_ENV = "OPENNOW_MACOS_EMBEDDED_RENDERER_PROTOTYPE";

function envFlagDisabled(value: string | undefined): boolean {
  if (value === undefined || value === null) return false;
  const normalized = String(value).trim().toLowerCase();
  return normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no";
}

function isEmbeddedRendererEnabled(): boolean {
  if (process.platform !== "darwin") return false;
  if (envFlagDisabled(process.env[ENABLED_ENV])) return false;
  if (envFlagDisabled(process.env[PROTOTYPE_ENV])) return false;
  return true;
}

export class MacEmbeddedRendererController {
  private binding: NativeBinding | null = null;
  private active: boolean = false;
  private lastIOSurfaceId: number | null = null;
  private lastDimensions: { width: number; height: number } | null = null;
  private lastFrameReadyTime: number = 0;
  private frameReadyThrottleMs: number = 4; // ~250 fps cap

  constructor() {
    if (isEmbeddedRendererEnabled()) {
      this.binding = this.loadBinding();
      this.active = this.binding !== null;
    }
  }

  private loadBinding(): NativeBinding | null {
    const candidates: string[] = [];

    // 1. Dev build (from dist-electron/main)
    candidates.push(resolve(__dirname, "../../native/macos-embedded-renderer/build/Release/renderer.node"));

    // 2. Source/dev via app path
    if (app.isReady()) {
      candidates.push(join(app.getAppPath(), "native/macos-embedded-renderer/build/Release/renderer.node"));
    }

    // 3. Packaged resource (flat copy)
    candidates.push(join(process.resourcesPath, "native", "macos-embedded-renderer", "renderer.node"));

    // 4. Packaged resource (build tree copy)
    candidates.push(join(process.resourcesPath, "native", "macos-embedded-renderer", "build", "Release", "renderer.node"));

    const failures: string[] = [];

    for (const candidatePath of candidates) {
      if (!existsSync(candidatePath)) {
        failures.push(`${candidatePath} (not found)`);
        continue;
      }
      try {
        const requireFn = createRequire(import.meta.url);
        const binding = requireFn(candidatePath) as NativeBinding;
        console.log(`[MacEmbeddedRenderer] Native addon loaded from: ${candidatePath}`);
        return binding;
      } catch (err) {
        failures.push(`${candidatePath} (load error: ${err})`);
      }
    }

    console.log(`[MacEmbeddedRenderer] Failed to load native addon from all candidates:
  ${failures.join("\n  ")}`);
    return null;
  }

  createOrUpdateSurface(surface: NativeRenderSurface): MacEmbeddedRendererResult | null {
    if (!this.binding || !this.active) {
      return null;
    }

    try {
      const windowHandle = surface.windowHandle;
      const rect = surface.rect;

      if (!windowHandle || !rect) {
        return null;
      }

      const width = rect.width;
      const height = rect.height;
      const scale = surface.deviceScaleFactor || 1;

      const dimensionThreshold = 8;
      const dimensionsChanged =
        !this.lastDimensions ||
        Math.abs(this.lastDimensions.width - width) >= dimensionThreshold ||
        Math.abs(this.lastDimensions.height - height) >= dimensionThreshold;

      if (dimensionsChanged && this.lastIOSurfaceId !== null) {
        this.binding.destroySurface();
        this.lastIOSurfaceId = null;
      }

      if (this.lastIOSurfaceId === null) {
        const result = this.binding.createSurface(windowHandle, width, height, scale);
        this.lastIOSurfaceId = result.iosurfaceId;
        this.lastDimensions = { width, height };
        console.log(`[MacEmbeddedRenderer] Created IOSurface: id=${result.iosurfaceId}, ${width}x${height}@${scale}x`);
        this.binding.updateSurface(windowHandle, rect.x, rect.y, width, height, scale, surface.visible);
        return result;
      }

      this.binding.updateSurface(windowHandle, rect.x, rect.y, width, height, scale, surface.visible);
      this.lastDimensions = { width, height };

      return { iosurfaceId: this.lastIOSurfaceId };
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in createOrUpdateSurface: ${err}`);
      return null;
    }
  }

  notifyFrameReady(): void {
    if (!this.binding || !this.active) {
      return;
    }

    try {
      const now = Date.now();
      if (now - this.lastFrameReadyTime < this.frameReadyThrottleMs) {
        return;
      }

      this.lastFrameReadyTime = now;
      this.binding.notifyFrameReady();
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in notifyFrameReady: ${err}`);
    }
  }

  dispose(reason: string = "unknown"): void {
    if (!this.binding || !this.active) {
      return;
    }

    try {
      this.binding.destroySurface();
      console.log(`[MacEmbeddedRenderer] Disposed: ${reason}`);
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in dispose: ${err}`);
    } finally {
      this.lastIOSurfaceId = null;
      this.lastDimensions = null;
      this.active = false;
      this.binding = null;
    }
  }

  isActive(): boolean {
    return this.active;
  }
}

let controller: MacEmbeddedRendererController | null = null;

export function getMacEmbeddedRenderer(): MacEmbeddedRendererController {
  if (!controller) {
    controller = new MacEmbeddedRendererController();
  }
  return controller;
}

export function disposeMacEmbeddedRenderer(reason: string = "shutdown"): void {
  if (controller) {
    controller.dispose(reason);
    controller = null;
  }
}
