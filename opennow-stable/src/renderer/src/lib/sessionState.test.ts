/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { toLaunchErrorState } from "./sessionState";

const translations: Record<string, string> = {
  "errors.duplicateSessionTitle": "Duplicate Session Detected",
  "errors.duplicateSessionDescription": "Another session is already running.",
  "errors.insufficientPlayabilityTitle": "Membership Upgrade Required",
  "errors.insufficientPlayabilityDescription": "Your current GeForce NOW membership is not high enough to play this game. Upgrade to a higher tier and try again.",
  "errors.insufficientPlayabilityTierDescription": "This game requires {{tier}} on GeForce NOW. Upgrade your membership to play it.",
  "errors.launchFailedTitle": "Launch Failed",
  "errors.launchUnknown": "The game could not start. Please try again.",
};

function t(key: string, values: Record<string, string | number | boolean | null | undefined> = {}): string {
  const template = translations[key] ?? key;
  return template.replace(/\{\{\s*(\w+)\s*\}\}/g, (_match, token: string) => String(values[token] ?? ""));
}

test("launch error state treats insufficient playability as a membership upgrade problem", () => {
  const state = toLaunchErrorState(t, {
    gfnErrorCode: 3237093718,
    title: "Playability Level Issue",
    description: "Your account's playability level is insufficient.",
  }, "queue");

  assert.equal(state.title, "Membership Upgrade Required");
  assert.match(state.description, /GeForce NOW membership is not high enough/i);
  assert.equal(state.codeLabel, "SessionInsufficientPlayabilityLevel (3237093718)");
});

test("launch error state prefers game catalog unplayable copy when available", () => {
  const state = toLaunchErrorState(t, { gfnErrorCode: 3237093718 }, "setup", {
    title: "Cyberpunk 2077",
    membershipTierLabel: "Ultimate",
    catalogSkuStrings: {
      SKU_BASED_UNPLAYABLE_DIALOG_HEADER: "Ultimate membership required",
      SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE: "Upgrade to {{SKU}} to play this game.",
    },
  });

  assert.equal(state.title, "Ultimate membership required");
  assert.equal(state.description, "Upgrade to Ultimate membership required to play this game.");
});

test("launch error state still treats session limit as duplicate session", () => {
  const state = toLaunchErrorState(t, { gfnErrorCode: 3237093643 }, "queue");

  assert.equal(state.title, "Duplicate Session Detected");
  assert.equal(state.description, "Another session is already running.");
});

