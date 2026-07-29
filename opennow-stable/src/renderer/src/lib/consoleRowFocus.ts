export interface RowFocus {
  rowIndex: number;
  columnIndex: number;
}

export type RowFocusDirection = "up" | "down" | "left" | "right";

function clamp(value: number, max: number): number {
  return Math.max(0, Math.min(value, max));
}

/**
 * Grid focus movement for console row layouts.
 *
 * Clamps rather than wraps on both axes — wrapping on a TV grid loses the
 * user's place. Vertical movement preserves the column, clamped into the
 * target row's length.
 */
export function moveRowFocus(
  rowLengths: readonly number[],
  focus: RowFocus,
  direction: RowFocusDirection,
): RowFocus {
  if (rowLengths.length === 0) return focus;

  const rowIndex = clamp(focus.rowIndex, rowLengths.length - 1);
  const currentLength = rowLengths[rowIndex] ?? 0;
  if (currentLength === 0) return focus;

  const columnIndex = clamp(focus.columnIndex, currentLength - 1);

  if (direction === "left") return { rowIndex, columnIndex: clamp(columnIndex - 1, currentLength - 1) };
  if (direction === "right") return { rowIndex, columnIndex: clamp(columnIndex + 1, currentLength - 1) };

  const nextRowIndex = clamp(direction === "up" ? rowIndex - 1 : rowIndex + 1, rowLengths.length - 1);
  const nextLength = rowLengths[nextRowIndex] ?? 0;
  if (nextLength === 0) return { rowIndex, columnIndex };

  return { rowIndex: nextRowIndex, columnIndex: clamp(columnIndex, nextLength - 1) };
}

/** Clamps an arbitrary focus into a valid position for the given row lengths. */
export function clampRowFocus(rowLengths: readonly number[], focus: RowFocus): RowFocus {
  if (rowLengths.length === 0) return { rowIndex: 0, columnIndex: 0 };
  const rowIndex = clamp(focus.rowIndex, rowLengths.length - 1);
  const length = rowLengths[rowIndex] ?? 0;
  return { rowIndex, columnIndex: length === 0 ? 0 : clamp(focus.columnIndex, length - 1) };
}
