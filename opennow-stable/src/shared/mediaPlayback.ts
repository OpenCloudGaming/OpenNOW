/** Extensions the app treats as local video files for in-app playback and thumbnails. Keep in sync with main media handling. */
export const PLAYABLE_VIDEO_EXTENSIONS = [".mp4", ".webm", ".mkv", ".mov"] as const;

export function isPlayableVideoFilePath(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return PLAYABLE_VIDEO_EXTENSIONS.some((ext) => lower.endsWith(ext));
}
