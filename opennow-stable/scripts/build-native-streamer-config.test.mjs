import assert from "node:assert/strict";
import test from "node:test";

import { nativeStreamerCargoArgs } from "./build-native-streamer-config.mjs";

const common = [
  "build",
  "--locked",
  "--release",
  "--package",
  "opennow-streamer",
  "--manifest-path",
  "/workspace/Cargo.toml",
];

test("production Linux build bundles FFmpeg for both architectures", () => {
  for (const [platformKey, nativeTarget] of [
    ["linux-x64", "x86_64-unknown-linux-gnu"],
    ["linux-arm64", "aarch64-unknown-linux-gnu"],
  ]) {
    assert.deepEqual(
      nativeStreamerCargoArgs({
        manifestPath: "/workspace/Cargo.toml",
        nativeTarget,
        platformKey,
      }),
      [...common, "--features", "linux-ffmpeg-bundled", "--target", nativeTarget],
    );
  }
});

test("non-Linux production build snapshot does not enable Linux VA-API", () => {
  assert.deepEqual(
    nativeStreamerCargoArgs({
      manifestPath: "/workspace/Cargo.toml",
      nativeTarget: "x86_64-pc-windows-msvc",
      platformKey: "win32-x64",
    }),
    [...common, "--target", "x86_64-pc-windows-msvc"],
  );
});

test("host Linux production build bundles FFmpeg without a target", () => {
  assert.deepEqual(
    nativeStreamerCargoArgs({
      manifestPath: "/workspace/Cargo.toml",
      nativeTarget: "",
      platformKey: "linux-x64",
    }),
    [...common, "--features", "linux-ffmpeg-bundled"],
  );
});

test("Linux production build can explicitly enable VA-API", () => {
  assert.deepEqual(
    nativeStreamerCargoArgs({
      manifestPath: "/workspace/Cargo.toml",
      nativeTarget: "",
      platformKey: "linux-x64",
      enableLinuxVaapi: true,
    }),
    [...common, "--features", "linux-vaapi,linux-ffmpeg-bundled"],
  );
});

test("Linux production build can explicitly disable FFmpeg", () => {
  assert.deepEqual(
    nativeStreamerCargoArgs({
      manifestPath: "/workspace/Cargo.toml",
      nativeTarget: "",
      platformKey: "linux-x64",
      enableLinuxFfmpeg: false,
    }),
    common,
  );
});

test("Linux production build can disable every optional decoder", () => {
  assert.deepEqual(
    nativeStreamerCargoArgs({
      manifestPath: "/workspace/Cargo.toml",
      nativeTarget: "",
      platformKey: "linux-x64",
      enableLinuxVaapi: false,
      enableLinuxFfmpeg: false,
    }),
    common,
  );
});
