import type { VideoCodec } from "@shared/gfn";
import { USER_FACING_VIDEO_CODEC_OPTIONS } from "@shared/gfn";

export const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];
export const allResolutionOptions = ["1280x720", "1280x800", "1440x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "2560x1600", "3440x1440", "3840x2160", "3840x2400"];
export const fpsOptions = [30, 60, 120, 144, 165, 240];
export const aspectRatioOptions = ["16:9", "16:10", "21:9", "32:9"] as const;

const RESOLUTION_TO_ASPECT_RATIO: Record<string, string> = {
  "1280x720": "16:9",
  "1280x800": "16:10",
  "1440x900": "16:10",
  "1680x1050": "16:10",
  "1920x1080": "16:9",
  "1920x1200": "16:10",
  "2560x1080": "21:9",
  "2560x1440": "16:9",
  "2560x1600": "16:10",
  "3440x1440": "21:9",
  "3840x2160": "16:9",
  "3840x2400": "16:10",
  "5120x1440": "32:9",
};

export const getResolutionsByAspectRatio = (aspectRatio: string): string[] => {
  return allResolutionOptions.filter(res => RESOLUTION_TO_ASPECT_RATIO[res] === aspectRatio);
};
export const resolutionOptions = getResolutionsByAspectRatio("16:9");
