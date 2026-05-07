import type { BrowserWindow } from "electron";
import type {
  NativeRenderSurface,
  NativeRenderSurfaceUpdate,
} from "@shared/gfn";

export function nativeWindowHandleToHex(window: BrowserWindow): string | null {
  const handle = window.getNativeWindowHandle();
  if (handle.byteLength >= 8) {
    return `0x${handle.readBigUInt64LE(0).toString(16)}`;
  }
  if (handle.byteLength >= 4) {
    return `0x${handle.readUInt32LE(0).toString(16)}`;
  }
  return null;
}

export function normalizeNativeRenderSurface(
  window: BrowserWindow,
  input: NativeRenderSurfaceUpdate,
): NativeRenderSurface | null {
  if (!input || typeof input !== "object") {
    return null;
  }

  const windowHandle = nativeWindowHandleToHex(window);
  if (!windowHandle) {
    return null;
  }

  const deviceScaleFactor = Number.isFinite(input.deviceScaleFactor)
    ? Math.min(8, Math.max(0.25, input.deviceScaleFactor))
    : 1;
  const rect = input.rect;
  const visible =
    input.visible === true &&
    rect !== null &&
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width >= 2 &&
    rect.height >= 2;

  return {
    windowHandle,
    deviceScaleFactor,
    visible,
    showStats: input.showStats === true,
    rect: visible
      ? {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.max(2, Math.round(rect.width)),
          height: Math.max(2, Math.round(rect.height)),
        }
      : null,
  };
}
