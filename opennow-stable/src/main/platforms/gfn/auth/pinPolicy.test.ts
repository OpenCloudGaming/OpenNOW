import test from "node:test";
import assert from "node:assert/strict";

import {
  createPinAttemptState,
  evaluatePinGate,
  isPinFormatValid,
  PIN_LOCKOUT_STEPS_MS,
  PIN_MAX_ATTEMPTS,
  registerPinFailure,
  registerPinSuccess,
} from "./pinPolicy";

function failTimes(count: number, nowMs: number) {
  let state = createPinAttemptState();
  for (let index = 0; index < count; index += 1) {
    state = registerPinFailure(state, nowMs);
  }
  return state;
}

test("accepts exactly four ASCII digits", () => {
  assert.equal(isPinFormatValid("0000"), true);
  assert.equal(isPinFormatValid("9182"), true);
});

test("rejects wrong lengths, non-digits, and non-ASCII digit forms", () => {
  assert.equal(isPinFormatValid("12"), false);
  assert.equal(isPinFormatValid("12345"), false);
  assert.equal(isPinFormatValid("12a4"), false);
  assert.equal(isPinFormatValid(""), false);
  assert.equal(isPinFormatValid(" 123"), false);
  assert.equal(isPinFormatValid("１２３４"), false, "full-width digits must not pass");
  assert.equal(isPinFormatValid("١٢٣٤"), false, "Arabic-Indic digits must not pass");
});

test("locks out after the attempt limit", () => {
  const state = failTimes(PIN_MAX_ATTEMPTS, 1_000);
  const gate = evaluatePinGate(state, 1_000);
  assert.equal(gate.allowed, false);
  assert.equal(gate.remainingAttempts, 0);
  assert.equal(gate.lockedUntilMs, 1_000 + PIN_LOCKOUT_STEPS_MS[0]);
});

test("counts down remaining attempts before locking", () => {
  const state = failTimes(2, 0);
  assert.equal(evaluatePinGate(state, 0).remainingAttempts, PIN_MAX_ATTEMPTS - 2);
  assert.equal(evaluatePinGate(state, 0).allowed, true);
});

test("allows again exactly at the lockout expiry", () => {
  const state = failTimes(PIN_MAX_ATTEMPTS, 1_000);
  const expiry = 1_000 + PIN_LOCKOUT_STEPS_MS[0];
  assert.equal(evaluatePinGate(state, expiry - 1).allowed, false);
  assert.equal(evaluatePinGate(state, expiry).allowed, true);
});

test("escalates the lockout on each subsequent round and caps at the last step", () => {
  let state = createPinAttemptState();
  for (let level = 0; level < PIN_LOCKOUT_STEPS_MS.length; level += 1) {
    for (let attempt = 0; attempt < PIN_MAX_ATTEMPTS; attempt += 1) {
      state = registerPinFailure(state, 0);
    }
    assert.equal(state.lockedUntilMs, PIN_LOCKOUT_STEPS_MS[level], `level ${level}`);
  }

  for (let attempt = 0; attempt < PIN_MAX_ATTEMPTS; attempt += 1) {
    state = registerPinFailure(state, 0);
  }
  assert.equal(state.lockedUntilMs, PIN_LOCKOUT_STEPS_MS[PIN_LOCKOUT_STEPS_MS.length - 1], "caps at the final step");
});

test("success clears the counter and the escalation level", () => {
  const locked = failTimes(PIN_MAX_ATTEMPTS, 0);
  assert.ok(locked.lockoutLevel > 0);

  const reset = registerPinSuccess(locked);
  assert.deepEqual(reset, createPinAttemptState());
  assert.equal(evaluatePinGate(reset, 0).allowed, true);
  assert.equal(evaluatePinGate(reset, 0).remainingAttempts, PIN_MAX_ATTEMPTS);
});

test("never mutates the state it is given", () => {
  const state = createPinAttemptState();
  const snapshot = { ...state };
  registerPinFailure(state, 500);
  registerPinSuccess(state);
  assert.deepEqual(state, snapshot);
});
