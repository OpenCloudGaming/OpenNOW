import test from "node:test";
import assert from "node:assert/strict";

import {
  getConsoleImageWidths,
  IMAGE_WIDTH_LADDER,
  snapImageWidth,
  withImageWidth,
} from "./consoleImageSizing";

const NVIDIA = "https://img.nvidiagrid.net/apps/1/ZZ/GAME_BOX_ART_01_abc.jpg;f=webp;w=1200";

test("snaps up to the next ladder step", () => {
  assert.equal(snapImageWidth(1), 160);
  assert.equal(snapImageWidth(160), 160);
  assert.equal(snapImageWidth(161), 240);
  assert.equal(snapImageWidth(500), 640);
});

test("clamps beyond the ladder instead of inventing a width", () => {
  assert.equal(snapImageWidth(99_999), IMAGE_WIDTH_LADDER[IMAGE_WIDTH_LADDER.length - 1]);
  assert.equal(snapImageWidth(0), 160);
  assert.equal(snapImageWidth(-10), 160);
  assert.equal(snapImageWidth(Number.NaN), 160);
});

test("rewrites an existing width parameter", () => {
  assert.equal(withImageWidth(NVIDIA, 180), "https://img.nvidiagrid.net/apps/1/ZZ/GAME_BOX_ART_01_abc.jpg;f=webp;w=240");
});

test("appends a width when the URL only has a format parameter", () => {
  const url = "https://img.nvidiagrid.net/apps/1/ZZ/a.jpg;f=webp";
  assert.equal(withImageWidth(url, 300), `${url};w=320`);
});

test("adds both parameters for a bare NVIDIA URL", () => {
  const url = "https://img.nvidiagrid.net/apps/1/ZZ/a.jpg";
  assert.equal(withImageWidth(url, 300), `${url};f=webp;w=320`);
});

test("never touches non-resizable hosts", () => {
  const steam = "https://cdn.cloudflare.steamstatic.com/steam/apps/440/library_600x900.jpg";
  assert.equal(withImageWidth(steam, 240), steam);
});

test("passes undefined through", () => {
  assert.equal(withImageWidth(undefined, 240), undefined);
});

test("snapping keeps the URL stable across nearby viewport sizes", () => {
  // Two window widths that render slightly different cards must still resolve
  // to the same URL, or every resize would miss the cache.
  const a = withImageWidth(NVIDIA, getConsoleImageWidths(1900).card);
  const b = withImageWidth(NVIDIA, getConsoleImageWidths(1935).card);
  assert.equal(a, b);
});

test("derives widths well below the catalog default", () => {
  const widths = getConsoleImageWidths(1920, 1);
  assert.equal(widths.card, 240, "a ~180px card must not request 1200px");
  assert.ok(widths.thumb <= 160);
  assert.ok(widths.billboard >= 1280);
});

test("accounts for high-DPI displays but stops at 2x", () => {
  assert.ok(getConsoleImageWidths(1920, 2).card > getConsoleImageWidths(1920, 1).card);
  assert.equal(getConsoleImageWidths(1920, 4).card, getConsoleImageWidths(1920, 2).card);
});
