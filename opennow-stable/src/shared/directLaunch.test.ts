import assert from "node:assert/strict";
import test from "node:test";

import { parseDirectLaunchArgs } from "./directLaunch";

test("parses launch app id from separate CLI argument", () => {
  assert.deepEqual(
    parseDirectLaunchArgs(["OpenNOW", "--launch-app-id", "12345"]),
    { appId: "12345", title: undefined },
  );
});

test("parses app id and title from inline CLI arguments", () => {
  assert.deepEqual(
    parseDirectLaunchArgs(["--launch-title=Fortnite", "--launch-app-id=98765"]),
    { appId: "98765", title: "Fortnite" },
  );
});

test("allows title fallback when no app id is present", () => {
  assert.deepEqual(
    parseDirectLaunchArgs(["--launch-game", "Cyberpunk 2077"]),
    { appId: undefined, title: "Cyberpunk 2077" },
  );
});

test("ignores invalid app ids unless a title fallback is available", () => {
  assert.deepEqual(
    parseDirectLaunchArgs(["--launch-app-id", "not-a-number", "--game", "Portal"]),
    { appId: undefined, title: "Portal" },
  );
  assert.equal(parseDirectLaunchArgs(["--launch-app-id", "not-a-number"]), null);
});
