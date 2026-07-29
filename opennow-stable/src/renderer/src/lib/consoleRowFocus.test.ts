import test from "node:test";
import assert from "node:assert/strict";

import { clampRowFocus, moveRowFocus } from "./consoleRowFocus";

const GRID = [5, 3, 8];

test("moves within a row and clamps at both horizontal edges", () => {
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 0, columnIndex: 2 }, "right"), { rowIndex: 0, columnIndex: 3 });
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 0, columnIndex: 4 }, "right"), { rowIndex: 0, columnIndex: 4 });
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 0, columnIndex: 0 }, "left"), { rowIndex: 0, columnIndex: 0 });
});

test("clamps at both vertical edges instead of wrapping", () => {
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 0, columnIndex: 1 }, "up"), { rowIndex: 0, columnIndex: 1 });
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 2, columnIndex: 1 }, "down"), { rowIndex: 2, columnIndex: 1 });
});

test("preserves the column when moving vertically", () => {
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 2, columnIndex: 6 }, "up"), { rowIndex: 1, columnIndex: 2 });
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 1, columnIndex: 2 }, "down"), { rowIndex: 2, columnIndex: 2 });
});

test("clamps the column into a shorter target row", () => {
  assert.deepEqual(moveRowFocus(GRID, { rowIndex: 0, columnIndex: 4 }, "down"), { rowIndex: 1, columnIndex: 2 });
});

test("leaves focus unchanged when the grid is empty", () => {
  const focus = { rowIndex: 0, columnIndex: 0 };
  assert.deepEqual(moveRowFocus([], focus, "right"), focus);
});

test("leaves focus unchanged when the current row is empty", () => {
  const focus = { rowIndex: 0, columnIndex: 0 };
  assert.deepEqual(moveRowFocus([0, 3], focus, "right"), focus);
});

test("stays put when the target row is empty", () => {
  assert.deepEqual(moveRowFocus([4, 0], { rowIndex: 0, columnIndex: 2 }, "down"), { rowIndex: 0, columnIndex: 2 });
});

test("clampRowFocus pulls out-of-range focus back into the grid", () => {
  assert.deepEqual(clampRowFocus(GRID, { rowIndex: 9, columnIndex: 9 }), { rowIndex: 2, columnIndex: 7 });
  assert.deepEqual(clampRowFocus(GRID, { rowIndex: -3, columnIndex: -1 }), { rowIndex: 0, columnIndex: 0 });
  assert.deepEqual(clampRowFocus([], { rowIndex: 4, columnIndex: 4 }), { rowIndex: 0, columnIndex: 0 });
});
