export function nativeStreamerCargoArgs({
  manifestPath,
  nativeTarget,
  platformKey,
  enableLinuxVaapi = false,
  enableLinuxFfmpeg = true,
}) {
  const args = [
    "build",
    "--locked",
    "--release",
    "--package",
    "opennow-streamer",
    "--manifest-path",
    manifestPath,
  ];
  if (platformKey.startsWith("linux-")) {
    const features = [];
    if (enableLinuxVaapi) features.push("linux-vaapi");
    if (enableLinuxFfmpeg) features.push("linux-ffmpeg-bundled");
    if (features.length > 0) args.push("--features", features.join(","));
  }
  if (nativeTarget) {
    args.push("--target", nativeTarget);
  }
  return args;
}
