import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { resolveNativeStreamerExecutableCandidates } from "./executableDiscovery";

function options(root: string, configuredPath = "") {
  return {
    platform: "linux" as const,
    arch: "x64",
    resourcesPath: join(root, "resources"),
    appPath: join(root, "app"),
    mainDir: join(root, "app", "out", "main"),
    envExecutablePath: undefined,
    getConfiguredPath: () => configuredPath,
  };
}

test("discovers the packaged self-contained executable", () => {
  const root = mkdtempSync(join(tmpdir(), "opennow-native-discovery-"));
  try {
    const executable = join(root, "resources", "native", "opennow-streamer", "linux-x64", "opennow-streamer");
    mkdirSync(join(executable, ".."), { recursive: true });
    writeFileSync(executable, "native");
    chmodSync(executable, 0o755);

    assert.deepEqual(resolveNativeStreamerExecutableCandidates(options(root)), [executable]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects a configured executable path that does not exist", () => {
  const root = mkdtempSync(join(tmpdir(), "opennow-native-discovery-"));
  try {
    const missing = join(root, "missing-streamer");
    assert.throws(
      () => resolveNativeStreamerExecutableCandidates(options(root, missing)),
      /Configured native streamer executable was not found/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
