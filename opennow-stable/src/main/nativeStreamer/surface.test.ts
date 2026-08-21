/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import type { BrowserWindow } from "electron";
import { normalizeNativeRenderSurface } from "./surface";

test("normalizes renderer bounds into absolute screen coordinates", () => {
  const handle = Buffer.alloc(8);
  handle.writeBigUInt64LE(0x1234n);
  const window = {
    getNativeWindowHandle: () => handle,
    getContentBounds: () => ({ x: 100, y: 200, width: 1280, height: 720 }),
  } as unknown as BrowserWindow;

  const surface = normalizeNativeRenderSurface(window, {
    rect: { x: 12.4, y: 24.6, width: 640.2, height: 360.4 },
    visible: true,
    deviceScaleFactor: 2,
  });

  assert.deepEqual(surface, {
    windowHandle: "0x1234",
    deviceScaleFactor: 2,
    visible: true,
    showStats: false,
    rect: { x: 12, y: 25, width: 640, height: 360 },
    screenRect: { x: 112, y: 225, width: 640, height: 360 },
  });
});
