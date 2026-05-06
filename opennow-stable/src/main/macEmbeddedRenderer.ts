import { app } from "electron";
import path from "path";
import type { NativeRenderSurface } from "../shared/gfn";

// MACOS COMPILE REQUIRED — runtime check ensures no-op on non-darwin platforms

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

export class MacEmbeddedRendererController {
  private binding: NativeBinding | null = null;
  private active: boolean = false;
  private lastIOSurfaceId: number | null = null;
  private lastDimensions: { width: number; height: number } | null = null;
  private lastFrameReadyTime: number = 0;
  private frameReadyThrottleMs: number = 4; // ~250 fps cap

  constructor() {
    // Only load binding on darwin and when env gate is set
    if (process.platform === "darwin" && process.env.OPENNOW_MACOS_EMBEDDED_RENDERER_PROTOTYPE === "1") {
      this.binding = this.loadBinding();
      this.active = this.binding !== null;
    }
  }

  private loadBinding(): NativeBinding | null {
    try {
      const mainDir = process.env.OPENNOW_MAIN_DIR || path.dirname(__dirname);
      const bindingPath = path.join(mainDir, "..", "native", "macos-embedded-renderer", "build", "Release", "renderer.node");

      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const binding = require(bindingPath) as NativeBinding;
      console.log("[MacEmbeddedRenderer] Native addon loaded");
      return binding;
    } catch (err) {
      console.log(`[MacEmbeddedRenderer] Failed to load native addon: ${err}`);
      return null;
    }
  }

  /**
   * Creates or updates the IOSurface-backed NSView.
   * On first visible call:
   *   - Creates IOSurface + NSView
   *   - Returns { iosurfaceId }
   * On subsequent calls:
   *   - Recreates IOSurface if dimensions changed by ≥2px
   *   - Updates NSView frame/visibility
   *   - Returns { iosurfaceId } or null if binding unavailable
   */
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

      // Threshold for dimension change that triggers IOSurface recreation
      const dimensionThreshold = 2;
      const dimensionsChanged =
        !this.lastDimensions ||
        Math.abs(this.lastDimensions.width - width) >= dimensionThreshold ||
        Math.abs(this.lastDimensions.height - height) >= dimensionThreshold;

      if (dimensionsChanged && this.lastIOSurfaceId !== null) {
        // Recreate IOSurface for new dimensions
        this.binding.destroySurface();
        this.lastIOSurfaceId = null;
      }

      // Create surface on first visible call or after dimension change
      if (this.lastIOSurfaceId === null) {
        const result = this.binding.createSurface(windowHandle, width, height, scale);
        this.lastIOSurfaceId = result.iosurfaceId;
        this.lastDimensions = { width, height };
        console.log(`[MacEmbeddedRenderer] Created IOSurface: id=${result.iosurfaceId}, ${width}x${height}@${scale}x`);
        // Apply initial position and visibility immediately after creation.
        this.binding.updateSurface(windowHandle, rect.x, rect.y, width, height, scale, surface.visible);
        return result;
      }

      // Update NSView frame and visibility on subsequent calls
      this.binding.updateSurface(windowHandle, rect.x, rect.y, width, height, scale, surface.visible);
      this.lastDimensions = { width, height };

      return { iosurfaceId: this.lastIOSurfaceId };
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in createOrUpdateSurface: ${err}`);
      return null;
    }
  }

  /**
   * Called by manager.ts when a frame-ready event is received from the Rust process.
   * Rate-limited to ~250 fps to avoid excessive layer invalidation.
   */
  notifyFrameReady(): void {
    if (!this.binding || !this.active) {
      return;
    }

    try {
      const now = Date.now();
      if (now - this.lastFrameReadyTime < this.frameReadyThrottleMs) {
        return; // Skip frame-ready if called too soon
      }

      this.lastFrameReadyTime = now;
      this.binding.notifyFrameReady();
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in notifyFrameReady: ${err}`);
    }
  }

  /**
   * Stops the addon, releases NSView + IOSurface.
   */
  dispose(reason: string = "unknown"): void {
    if (!this.binding || !this.active) {
      return;
    }

    try {
      this.binding.destroySurface();
      console.log(`[MacEmbeddedRenderer] Disposed: ${reason}`);
      this.lastIOSurfaceId = null;
      this.lastDimensions = null;
      this.active = false;
    } catch (err) {
      console.error(`[MacEmbeddedRenderer] Error in dispose: ${err}`);
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
