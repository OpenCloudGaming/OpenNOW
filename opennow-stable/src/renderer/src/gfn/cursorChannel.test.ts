/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  cursorDevicePixelRatioScale,
  nativeCursorStyle,
  parseGfnCursorChannelMessage,
  shouldApplyCursorChannelPosition,
} from "./cursorChannel";

function utf8(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function uint16Le(value: number): number[] {
  return [value & 0xff, (value >> 8) & 0xff];
}

test("parseGfnCursorChannelMessage parses predefined cursor updates with optional position", () => {
  const bytes = new Uint8Array([
    0,
    12,
    0,
    0,
    0,
    ...uint16Le(0),
    ...uint16Le(32768),
    ...uint16Le(65535),
  ]);

  const parsed = parseGfnCursorChannelMessage(bytes);
  assert.deepEqual(parsed, {
    type: "predefined",
    cursorId: 12,
    position: { x: 32768, y: 65535 },
  });
});

test("parseGfnCursorChannelMessage parses custom cursor image metadata", () => {
  const mimeType = utf8("image/png");
  const image = utf8("AAAA");
  const bytes = new Uint8Array([
    1,
    7,
    3,
    4,
    mimeType.length,
    ...mimeType,
    ...uint16Le(image.length),
    ...image,
    ...uint16Le(10),
    ...uint16Le(20),
    ...uint16Le(150),
  ]);

  const parsed = parseGfnCursorChannelMessage(bytes);
  assert.deepEqual(parsed, {
    type: "custom",
    cursorId: 7,
    hotspotX: 3,
    hotspotY: 4,
    mimeType: "image/png",
    imageBase64: "AAAA",
    position: { x: 10, y: 20 },
    scale: 1.5,
  });
});

test("parseGfnCursorChannelMessage rejects truncated custom cursor images", () => {
  const bytes = new Uint8Array([1, 1, 0, 0, 0, ...uint16Le(4), 65, 65]);
  assert.equal(parseGfnCursorChannelMessage(bytes), null);
});

test("cursorDevicePixelRatioScale matches official 1.5x rounding quirk", () => {
  assert.equal(cursorDevicePixelRatioScale(1.4), 1);
  assert.equal(cursorDevicePixelRatioScale(1.5), 2);
  assert.equal(cursorDevicePixelRatioScale(2), 2);
});

test("nativeCursorStyle uses the negotiated image-set function", () => {
  assert.equal(
    nativeCursorStyle("data:image/png;base64,AAAA", 2.5, 3.25, 2, "image-set"),
    "image-set(url(data:image/png;base64,AAAA) 2x) 2.5 3.25, auto",
  );
  assert.equal(
    nativeCursorStyle("data:image/png;base64,AAAA", 2, 3, 1, null),
    "url(data:image/png;base64,AAAA) 2 3, auto",
  );
});

test("shouldApplyCursorChannelPosition only trusts position when cursor becomes visible", () => {
  const position = { x: 0, y: 0 };
  assert.equal(shouldApplyCursorChannelPosition(false, true, position), true);
  assert.equal(shouldApplyCursorChannelPosition(true, true, position), false);
  assert.equal(shouldApplyCursorChannelPosition(false, false, position), false);
  assert.equal(shouldApplyCursorChannelPosition(false, true), false);
});
