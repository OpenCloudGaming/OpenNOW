import test from "node:test";
import assert from "node:assert/strict";

import {
  appendPinDigit,
  deletePinDigit,
  getLockoutSecondsRemaining,
  isPinComplete,
  PIN_LENGTH,
} from "./consolePinInput";

test("appends digits up to the PIN length and then ignores further input", () => {
  let entry = "";
  for (const digit of ["1", "2", "3", "4"]) entry = appendPinDigit(entry, digit);
  assert.equal(entry, "1234");
  assert.equal(appendPinDigit(entry, "5"), "1234");
});

test("ignores non-digit input", () => {
  assert.equal(appendPinDigit("12", "a"), "12");
  assert.equal(appendPinDigit("12", ""), "12");
  assert.equal(appendPinDigit("12", "12"), "12");
  assert.equal(appendPinDigit("12", "１"), "12", "full-width digits are not ASCII input");
});

test("deleting from an empty entry is a no-op", () => {
  assert.equal(deletePinDigit(""), "");
  assert.equal(deletePinDigit("123"), "12");
});

test("reports completion only at the exact length", () => {
  assert.equal(isPinComplete("123"), false);
  assert.equal(isPinComplete("1234"), true);
  assert.equal(PIN_LENGTH, 4);
});

test("rounds the lockout countdown up so it never under-reports", () => {
  assert.equal(getLockoutSecondsRemaining(null, 0), 0);
  assert.equal(getLockoutSecondsRemaining(10_000, 10_000), 0);
  assert.equal(getLockoutSecondsRemaining(10_001, 10_000), 1);
  assert.equal(getLockoutSecondsRemaining(11_500, 10_000), 2);
  assert.equal(getLockoutSecondsRemaining(5_000, 10_000), 0, "an expired lock reads as zero");
});
