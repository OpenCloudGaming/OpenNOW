/**
 * Unit tests for HDR capability detection and settings logic.
 *
 * Run: npx tsx tests/hdr-logic.test.ts
 *
 * Tests the pure functions shouldEnableHdr and buildInitialHdrState
 * without requiring a browser environment.
 */

// ── inline types matching @shared/gfn ──────────────────────────────

type HdrStreamingMode = "off" | "auto" | "on";
type HdrPlatformSupport = "supported" | "best_effort" | "unsupported" | "unknown";
type HdrActiveStatus = "active" | "inactive" | "unsupported" | "fallback_sdr";
type ColorQuality = "8bit_420" | "8bit_444" | "10bit_420" | "10bit_444";

interface HdrCapability {
  platform: "windows" | "macos" | "linux" | "unknown";
  platformSupport: HdrPlatformSupport;
  osHdrEnabled: boolean;
  displayHdrCapable: boolean;
  decoder10BitCapable: boolean;
  hdrColorSpaceSupported: boolean;
  notes: string[];
}

interface HdrStreamState {
  status: HdrActiveStatus;
  bitDepth: 8 | 10;
  colorPrimaries: "BT.709" | "BT.2020" | "unknown";
  transferFunction: "SDR" | "PQ" | "HLG" | "unknown";
  matrixCoefficients: "BT.709" | "BT.2020" | "unknown";
  codecProfile: string;
  overlayForcesSdr: boolean;
  fallbackReason: string | null;
}

// ── reimplemented logic (mirrors hdrCapability.ts pure functions) ───

function shouldEnableHdr(
  mode: HdrStreamingMode,
  capability: HdrCapability,
  colorQuality: ColorQuality,
): { enable: boolean; reason: string } {
  if (mode === "off") {
    return { enable: false, reason: "HDR disabled in settings" };
  }

  const is10Bit = colorQuality.startsWith("10bit");
  if (!is10Bit) {
    return { enable: false, reason: "Color quality is 8-bit; 10-bit required for HDR" };
  }

  if (capability.platformSupport === "unsupported") {
    if (mode === "on") {
      return { enable: false, reason: `HDR unsupported on ${capability.platform}: ${capability.notes.slice(-1)[0] ?? "no HDR path"}` };
    }
    return { enable: false, reason: "Platform does not support HDR" };
  }

  if (capability.platformSupport === "unknown") {
    if (mode === "on") {
      return { enable: false, reason: "HDR support unknown on this platform" };
    }
    return { enable: false, reason: "HDR support could not be determined" };
  }

  if (!capability.decoder10BitCapable) {
    return { enable: false, reason: "No 10-bit decoder available" };
  }

  if (mode === "auto") {
    if (capability.platformSupport !== "supported") {
      return { enable: false, reason: `Platform HDR is best-effort on ${capability.platform}; set HDR to "On" to attempt` };
    }
    if (!capability.osHdrEnabled) {
      return { enable: false, reason: "OS HDR is not enabled" };
    }
    if (!capability.displayHdrCapable) {
      return { enable: false, reason: "Display does not report HDR capability" };
    }
    return { enable: true, reason: "All HDR conditions met (auto)" };
  }

  if (mode === "on") {
    if (!capability.osHdrEnabled && capability.platform === "windows") {
      return { enable: false, reason: "Windows OS HDR is disabled; enable HDR in Windows Display Settings" };
    }
    if (!capability.displayHdrCapable) {
      return { enable: false, reason: "Display does not report HDR capability" };
    }
    return { enable: true, reason: "HDR forced on by user" };
  }

  return { enable: false, reason: "Unknown HDR mode" };
}

function buildInitialHdrState(): HdrStreamState {
  return {
    status: "inactive",
    bitDepth: 8,
    colorPrimaries: "BT.709",
    transferFunction: "SDR",
    matrixCoefficients: "BT.709",
    codecProfile: "",
    overlayForcesSdr: false,
    fallbackReason: null,
  };
}

// ── Test runner ─────────────────────────────────────────────────────

let passed = 0;
let failed = 0;

function assert(condition: boolean, name: string): void {
  if (condition) {
    passed++;
    console.log(`  ✓ ${name}`);
  } else {
    failed++;
    console.error(`  ✗ ${name}`);
  }
}

function makeCapability(overrides: Partial<HdrCapability> = {}): HdrCapability {
  return {
    platform: "windows",
    platformSupport: "supported",
    osHdrEnabled: true,
    displayHdrCapable: true,
    decoder10BitCapable: true,
    hdrColorSpaceSupported: true,
    notes: [],
    ...overrides,
  };
}

// ── Tests ───────────────────────────────────────────────────────────

console.log("\n=== shouldEnableHdr ===\n");

console.log("mode=off:");
{
  const r = shouldEnableHdr("off", makeCapability(), "10bit_420");
  assert(!r.enable, "off mode always disables HDR");
  assert(r.reason.includes("disabled"), "reason mentions disabled");
}

console.log("\nmode=auto, full support:");
{
  const r = shouldEnableHdr("auto", makeCapability(), "10bit_420");
  assert(r.enable, "auto enables when all conditions met");
  assert(r.reason.includes("auto"), "reason mentions auto");
}

console.log("\nmode=auto, 8-bit color:");
{
  const r = shouldEnableHdr("auto", makeCapability(), "8bit_420");
  assert(!r.enable, "auto rejects 8-bit color");
  assert(r.reason.includes("8-bit"), "reason mentions 8-bit");
}

console.log("\nmode=auto, OS HDR disabled:");
{
  const r = shouldEnableHdr("auto", makeCapability({ osHdrEnabled: false }), "10bit_420");
  assert(!r.enable, "auto rejects when OS HDR disabled");
  assert(r.reason.includes("OS HDR"), "reason mentions OS HDR");
}

console.log("\nmode=auto, display not HDR capable:");
{
  const r = shouldEnableHdr("auto", makeCapability({ displayHdrCapable: false }), "10bit_420");
  assert(!r.enable, "auto rejects when display not HDR");
  assert(r.reason.includes("Display"), "reason mentions display");
}

console.log("\nmode=auto, best_effort platform (macOS):");
{
  const r = shouldEnableHdr("auto", makeCapability({ platform: "macos", platformSupport: "best_effort" }), "10bit_420");
  assert(!r.enable, "auto rejects best_effort platform");
  assert(r.reason.includes("best-effort"), "reason mentions best-effort");
}

console.log("\nmode=on, full support:");
{
  const r = shouldEnableHdr("on", makeCapability(), "10bit_420");
  assert(r.enable, "on enables when conditions met");
  assert(r.reason.includes("forced"), "reason mentions forced");
}

console.log("\nmode=on, best_effort platform with display HDR:");
{
  const r = shouldEnableHdr("on", makeCapability({ platform: "macos", platformSupport: "best_effort" }), "10bit_420");
  assert(r.enable, "on enables on best_effort with display HDR");
}

console.log("\nmode=on, Windows OS HDR disabled:");
{
  const r = shouldEnableHdr("on", makeCapability({ osHdrEnabled: false, platform: "windows" }), "10bit_420");
  assert(!r.enable, "on rejects when Windows OS HDR disabled");
  assert(r.reason.includes("Windows"), "reason mentions Windows");
}

console.log("\nmode=on, display not HDR capable:");
{
  const r = shouldEnableHdr("on", makeCapability({ displayHdrCapable: false }), "10bit_420");
  assert(!r.enable, "on rejects when display not HDR");
}

console.log("\nmode=auto, unsupported platform (linux):");
{
  const r = shouldEnableHdr("auto", makeCapability({ platform: "linux", platformSupport: "unsupported" }), "10bit_420");
  assert(!r.enable, "auto rejects unsupported platform");
  assert(r.reason.includes("does not support"), "reason mentions unsupported");
}

console.log("\nmode=on, unsupported platform (linux):");
{
  const r = shouldEnableHdr("on", makeCapability({ platform: "linux", platformSupport: "unsupported" }), "10bit_420");
  assert(!r.enable, "on rejects unsupported platform");
  assert(r.reason.includes("unsupported"), "reason mentions unsupported");
}

console.log("\nmode=on, no 10-bit decoder:");
{
  const r = shouldEnableHdr("on", makeCapability({ decoder10BitCapable: false }), "10bit_420");
  assert(!r.enable, "on rejects when no 10-bit decoder");
  assert(r.reason.includes("decoder"), "reason mentions decoder");
}

console.log("\nmode=auto, unknown platform:");
{
  const r = shouldEnableHdr("auto", makeCapability({ platform: "unknown", platformSupport: "unknown" }), "10bit_420");
  assert(!r.enable, "auto rejects unknown platform");
}

console.log("\n=== buildInitialHdrState ===\n");

{
  const state = buildInitialHdrState();
  assert(state.status === "inactive", "initial status is inactive");
  assert(state.bitDepth === 8, "initial bit depth is 8");
  assert(state.colorPrimaries === "BT.709", "initial primaries are BT.709");
  assert(state.transferFunction === "SDR", "initial transfer is SDR");
  assert(state.matrixCoefficients === "BT.709", "initial matrix is BT.709");
  assert(state.codecProfile === "", "initial codec profile is empty");
  assert(!state.overlayForcesSdr, "initial overlay doesn't force SDR");
  assert(state.fallbackReason === null, "initial fallback reason is null");
}

// ── Summary ─────────────────────────────────────────────────────────

console.log(`\n=== Results: ${passed} passed, ${failed} failed ===\n`);
process.exit(failed > 0 ? 1 : 0);
