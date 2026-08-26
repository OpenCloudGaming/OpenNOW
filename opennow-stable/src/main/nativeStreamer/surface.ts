import type { BrowserWindow } from "electron";
import type {
  NativeRenderSurface,
  NativeRenderSurfaceUpdate,
} from "@shared/gfn";

export function nativeWindowHandleToHex(window: BrowserWindow): string | null {
  try {
    const handle = window.getNativeWindowHandle();
    if (handle.byteLength >= 8) {
      return `0x${handle.readBigUInt64LE(0).toString(16)}`;
    }
    if (handle.byteLength >= 4) {
      return `0x${handle.readUInt32LE(0).toString(16)}`;
    }
  } catch {
    // Native Wayland output is a separate top-level surface and does not need
    // an Electron platform handle.
  }
  return null;
}

export function normalizeNativeRenderSurface(
  window: BrowserWindow,
  input: NativeRenderSurfaceUpdate,
  dipToScreenPoint?: (point: { x: number; y: number }) => { x: number; y: number },
): NativeRenderSurface | null {
  if (!input || typeof input !== "object") {
    return null;
  }

  const windowHandle = nativeWindowHandleToHex(window);

  const deviceScaleFactor = Number.isFinite(input.deviceScaleFactor)
    ? Math.min(8, Math.max(0.25, input.deviceScaleFactor))
    : 1;
  const rect = input.rect;
  const contentBounds = window.getContentBounds();
  const visible =
    input.visible === true &&
    rect !== null &&
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width >= 2 &&
    rect.height >= 2;

  const screenOriginDip = {
    x: contentBounds.x + Math.round((rect?.x ?? 0) / deviceScaleFactor),
    y: contentBounds.y + Math.round((rect?.y ?? 0) / deviceScaleFactor),
  };
  const screenOrigin = dipToScreenPoint?.(screenOriginDip) ?? screenOriginDip;

  return {
    ...(windowHandle ? { windowHandle } : {}),
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
    screenRect: visible
      ? {
          x: screenOrigin.x,
          y: screenOrigin.y,
          width: dipToScreenPoint
            ? Math.max(2, Math.round(rect.width))
            : Math.max(2, Math.round(rect.width / deviceScaleFactor)),
          height: dipToScreenPoint
            ? Math.max(2, Math.round(rect.height))
            : Math.max(2, Math.round(rect.height / deviceScaleFactor)),
        }
      : undefined,
  };
}
