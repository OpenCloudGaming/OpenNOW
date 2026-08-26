import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { resolveNativeStreamerExecutableCandidates } from "./executableDiscovery";

function options(
  root: string,
  configuredPath = "",
  platform: NodeJS.Platform = "linux",
  arch = "x64",
) {
  return {
    platform,
    arch,
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

test("discovers only the bundled helper automatically on macOS", () => {
  const root = mkdtempSync(join(tmpdir(), "opennow-native-discovery-"));
  try {
    const plainExecutable = join(
      root,
      "resources",
      "native",
      "opennow-streamer",
      "darwin-arm64",
      "opennow-streamer",
    );
    const bundledExecutable = join(
      root,
      "resources",
      "native",
      "opennow-streamer",
      "darwin-arm64",
      "OpenNOWStreamer.app",
      "Contents",
      "MacOS",
      "opennow-streamer",
    );
    mkdirSync(join(plainExecutable, ".."), { recursive: true });
    mkdirSync(join(bundledExecutable, ".."), { recursive: true });
    writeFileSync(plainExecutable, "legacy-unbundled-native");
    writeFileSync(bundledExecutable, "bundled-native");
    chmodSync(plainExecutable, 0o755);
    chmodSync(bundledExecutable, 0o755);

    assert.deepEqual(
      resolveNativeStreamerExecutableCandidates(options(root, "", "darwin", "arm64")),
      [bundledExecutable],
    );
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
