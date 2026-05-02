export type VideoCodec = "H264" | "H265" | "AV1";
export type VideoAccelerationPreference = "auto" | "hardware" | "software";

/** Color quality (bit depth + chroma subsampling), matching Rust ColorQuality enum */
export type ColorQuality = "8bit_420" | "8bit_444" | "10bit_420" | "10bit_444";

/** Game language codes for in-game localization (sent to GFN servers) */
export type GameLanguage =
  | "en_US" | "en_GB" | "de_DE" | "fr_FR" | "es_ES" | "es_MX" | "it_IT"
  | "pt_PT" | "pt_BR" | "ru_RU" | "pl_PL" | "tr_TR" | "ar_SA" | "ja_JP"
  | "ko_KR" | "zh_CN" | "zh_TW" | "th_TH" | "vi_VN" | "id_ID" | "cs_CZ"
  | "el_GR" | "hu_HU" | "ro_RO" | "uk_UA" | "nl_NL" | "sv_SE" | "da_DK"
  | "fi_FI" | "no_NO";

/** Keyboard layout codes for physical key mapping in remote sessions */
export type KeyboardLayout =
  | "en-US" | "en-GB" | "tr-TR" | "de-DE" | "fr-FR" | "es-ES" | "es-MX" | "it-IT"
  | "pt-PT" | "pt-BR" | "pl-PL" | "ru-RU" | "ja-JP" | "ko-KR" | "zh-CN" | "zh-TW";

export interface KeyboardLayoutOption {
  value: KeyboardLayout;
  label: string;
  macValue?: string;
}

export const DEFAULT_KEYBOARD_LAYOUT: KeyboardLayout = "en-US";

export const keyboardLayoutOptions: readonly KeyboardLayoutOption[] = [
  { value: "en-US", label: "English (US)", macValue: "m-us" },
  { value: "en-GB", label: "English (UK)", macValue: "m-brit" },
  { value: "tr-TR", label: "Turkish Q", macValue: "m-tr-qty" },
  { value: "de-DE", label: "German" },
  { value: "fr-FR", label: "French" },
  { value: "es-ES", label: "Spanish" },
  { value: "es-MX", label: "Spanish (Latin America)" },
  { value: "it-IT", label: "Italian" },
  { value: "pt-PT", label: "Portuguese (Portugal)" },
  { value: "pt-BR", label: "Portuguese (Brazil)" },
  { value: "pl-PL", label: "Polish" },
  { value: "ru-RU", label: "Russian" },
  { value: "ja-JP", label: "Japanese" },
  { value: "ko-KR", label: "Korean" },
  { value: "zh-CN", label: "Chinese (Simplified)" },
  { value: "zh-TW", label: "Chinese (Traditional)" },
] as const;

export function resolveGfnKeyboardLayout(layout: KeyboardLayout, platform: string): string {
  const option = keyboardLayoutOptions.find((candidate) => candidate.value === layout);
  if (platform === "darwin" && option?.macValue) {
    return option.macValue;
  }
  return option?.value ?? DEFAULT_KEYBOARD_LAYOUT;
}

/** Helper: get CloudMatch bitDepth value (0 = 8-bit SDR, 10 = 10-bit HDR capable) */
export function colorQualityBitDepth(cq: ColorQuality): number {
  return cq.startsWith("10bit") ? 10 : 0;
}

/** Helper: get CloudMatch chromaFormat value (0 = 4:2:0, 2 = 4:4:4) */
export function colorQualityChromaFormat(cq: ColorQuality): number {
  return cq.endsWith("444") ? 2 : 0;
}

/** Helper: does this color quality mode require HEVC or AV1? */
export function colorQualityRequiresHevc(cq: ColorQuality): boolean {
  return cq !== "8bit_420";
}

export const USER_FACING_VIDEO_CODEC_OPTIONS: readonly VideoCodec[] = ["H264", "H265", "AV1"];
export const USER_FACING_COLOR_QUALITY_OPTIONS: readonly ColorQuality[] = ["8bit_420", "8bit_444", "10bit_420", "10bit_444"];

export function isSupportedUserFacingCodec(codec: VideoCodec): boolean {
  return USER_FACING_VIDEO_CODEC_OPTIONS.includes(codec);
}

export function normalizeStreamPreferences(codec: VideoCodec, colorQuality: ColorQuality): {
  codec: VideoCodec;
  colorQuality: ColorQuality;
  migrated: boolean;
} {
  const normalizedCodec = isSupportedUserFacingCodec(codec)
    ? codec
    : USER_FACING_VIDEO_CODEC_OPTIONS[0];
  const normalizedColorQuality = USER_FACING_COLOR_QUALITY_OPTIONS.includes(colorQuality)
    ? colorQuality
    : USER_FACING_COLOR_QUALITY_OPTIONS[0];

  return {
    codec: normalizedCodec,
    colorQuality: normalizedColorQuality,
    migrated: normalizedCodec !== codec || normalizedColorQuality !== colorQuality,
  };
}

/** Helper: is this a 10-bit (HDR-capable) mode? */
export function colorQualityIs10Bit(cq: ColorQuality): boolean {
  return cq.startsWith("10bit");
}
