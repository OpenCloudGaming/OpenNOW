import { describe, expect, it } from "vitest";

import {
  colorQualityBitDepth,
  colorQualityChromaFormat,
  colorQualityIs10Bit,
  colorQualityRequiresHevc,
} from "../../src/shared/gfn";

describe("shared gfn helpers", () => {
  it("derives bit depth, chroma format, codec requirements, and 10-bit state", () => {
    expect(colorQualityBitDepth("8bit_420")).toBe(0);
    expect(colorQualityBitDepth("10bit_444")).toBe(10);
    expect(colorQualityChromaFormat("8bit_420")).toBe(0);
    expect(colorQualityChromaFormat("8bit_444")).toBe(2);
    expect(colorQualityRequiresHevc("8bit_420")).toBe(false);
    expect(colorQualityRequiresHevc("8bit_444")).toBe(true);
    expect(colorQualityIs10Bit("10bit_420")).toBe(true);
    expect(colorQualityIs10Bit("8bit_444")).toBe(false);
  });
});
