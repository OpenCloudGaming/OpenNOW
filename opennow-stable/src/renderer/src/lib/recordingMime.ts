/**
 * Ordered MediaRecorder candidates: MP4 + H.264/AAC first when supported — Chromium often picks a
 * hardware-backed encoder on consumer GPUs — then WebM fallbacks for Electron variance.
 */
export const RECORDING_MIME_CANDIDATES: readonly string[] = [
  "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
  "video/mp4;codecs=avc1",
  "video/mp4",
  "video/webm;codecs=vp8,opus",
  "video/webm;codecs=h264",
  "video/webm;codecs=vp8",
  "video/webm",
];

export function pickSupportedRecordingMime(fallback: string = "video/webm"): string {
  return RECORDING_MIME_CANDIDATES.find((m) => MediaRecorder.isTypeSupported(m)) ?? fallback;
}
