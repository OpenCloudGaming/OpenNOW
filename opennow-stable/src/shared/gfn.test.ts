/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo, GameVariant } from "./gfn";
import {
  OWNED_LIBRARY_STATUSES,
  buildNativeStreamerSessionContext,
  createUnsupportedNativeStreamerStatus,
  isEpicStore,
  isGameInLibrary,
  isNativeDirectXBackendSupported,
  isNativeExternalRendererSupported,
  isNativeExternalRendererRequired,
  isNativeStreamerSupportedPlatform,
  isNvstTransportSupported,
  isOwnedLibraryStatus,
  isOwnedVariant,
  NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE,
  getDefaultStreamPreferences,
  normalizeCodecPreference,
  normalizeFallbackCodecPreference,
  normalizeGameStore,
  normalizeNativeExternalRendererForPlatform,
  normalizeTransportModeForPlatform,
  normalizeStreamPreferences,
  normalizeStreamClientModeForPlatform,
} from "./gfn";

function makeVariant(overrides: Partial<GameVariant> = {}): GameVariant {
  return {
    id: overrides.id ?? "variant-1",
    store: overrides.store ?? "Steam",
    supportedControls: overrides.supportedControls ?? [],
    librarySelected: overrides.librarySelected,
    libraryStatus: overrides.libraryStatus,
    lastPlayedDate: overrides.lastPlayedDate,
    gfnStatus: overrides.gfnStatus,
  };
}

function makeGame(variants: GameVariant[]): GameInfo {
  return {
    id: "game-1",
    title: "Test Game",
    selectedVariantIndex: 0,
    variants,
  };
}

test("counts only the GeForce NOW owned library statuses as owned", () => {
  assert.deepEqual(OWNED_LIBRARY_STATUSES, ["MANUAL", "PLATFORM_SYNC", "IN_LIBRARY"]);

  assert.equal(isOwnedLibraryStatus("MANUAL"), true);
  assert.equal(isOwnedLibraryStatus("PLATFORM_SYNC"), true);
  assert.equal(isOwnedLibraryStatus("IN_LIBRARY"), true);

  assert.equal(isOwnedLibraryStatus("NOT_OWNED"), false);
  assert.equal(isOwnedLibraryStatus(""), false);
  assert.equal(isOwnedLibraryStatus(undefined), false);
});

test("does not treat librarySelected by itself as ownership", () => {
  assert.equal(isOwnedVariant(makeVariant({ librarySelected: true })), false);
  assert.equal(
    isOwnedVariant(makeVariant({ librarySelected: true, libraryStatus: "NOT_OWNED" })),
    false,
  );
  assert.equal(
    isOwnedVariant(makeVariant({ librarySelected: true, libraryStatus: "PLATFORM_SYNC" })),
    true,
  );
});

test("derives game in-library state from owned variants only", () => {
  assert.equal(
    isGameInLibrary(
      makeGame([
        makeVariant({ id: "steam", store: "Steam", libraryStatus: "NOT_OWNED" }),
        makeVariant({ id: "epic", store: "Epic", libraryStatus: "PLATFORM_SYNC" }),
      ]),
    ),
    true,
  );

  assert.equal(
    isGameInLibrary(
      makeGame([
        makeVariant({ id: "steam", store: "Steam" }),
        makeVariant({ id: "epic", store: "Epic", librarySelected: true }),
      ]),
    ),
    false,
  );
});

test("matches Epic store aliases only", () => {
  assert.equal(isEpicStore("EPIC_GAMES_STORE"), true);
  assert.equal(isEpicStore("Epic Games Store"), true);
  assert.equal(isEpicStore("EPIC"), true);
  assert.equal(isEpicStore("EGS"), true);
  assert.equal(isEpicStore("Steam"), false);
});

test("normalizes equivalent store spellings to one canonical key", () => {
  assert.equal(normalizeGameStore("Epic"), "EPIC_GAMES_STORE");
  assert.equal(normalizeGameStore("EGS"), "EPIC_GAMES_STORE");
  assert.equal(normalizeGameStore("GOG.com"), "GOG");
  assert.equal(normalizeGameStore("Battle.net"), "BATTLE_NET");
  assert.equal(normalizeGameStore("Gaijin.net"), "GAIJIN");
  assert.equal(normalizeGameStore("Microsoft Store"), "XBOX");
});

test("buildNativeStreamerSessionContext forwards requested/finalized streaming features", () => {
  const context = buildNativeStreamerSessionContext(
    {
      sessionId: "session-1",
      status: 2,
      zone: "NP-AMS-01",
      serverIp: "1.2.3.4",
      signalingServer: "1.2.3.4:443",
      signalingUrl: "wss://1.2.3.4/nvst/",
      iceServers: [],
      requestedStreamingFeatures: {
        reflex: true,
        bitDepth: 10,
        cloudGsync: true,
        chromaFormat: 2,
        enabledL4S: true,
      },
      finalizedStreamingFeatures: {
        reflex: false,
        bitDepth: 8,
        cloudGsync: false,
        chromaFormat: 0,
        enabledL4S: false,
      },
      negotiatedStreamProfile: {
        resolution: "2560x1440",
        fps: 240,
        enableCloudGsync: false,
      },
    },
    {
      resolution: "2560x1440",
      fps: 240,
      maxBitrateMbps: 75,
      codec: "H265",
      colorQuality: "10bit_444",
      keyboardLayout: "en-US",
      gameLanguage: "en_US",
      enableL4S: true,
      enableCloudGsync: true,
      clientMode: "native",
      nativeCloudGsyncMode: "auto",
      nativeTransitionDiagnostics: {
        forceQueueMode: "adaptive",
      },
    },
    {
      toggleStats: "F3",
      togglePointerLock: "F8",
      toggleFullscreen: "F11",
      stopStream: "Ctrl+Shift+Q",
      toggleAntiAfk: "Ctrl+Shift+K",
      toggleMicrophone: "Ctrl+Shift+M",
      screenshot: "Ctrl+F11",
      toggleRecording: "F12",
    },
  );

  assert.deepEqual(context.session.requestedStreamingFeatures, {
    reflex: true,
    bitDepth: 10,
    cloudGsync: true,
    chromaFormat: 2,
    enabledL4S: true,
  });
  assert.deepEqual(context.session.finalizedStreamingFeatures, {
    reflex: false,
    bitDepth: 8,
    cloudGsync: false,
    chromaFormat: 0,
    enabledL4S: false,
  });
  assert.equal(context.settings.codec, "H265");
  assert.equal(context.session.negotiatedStreamProfile?.codec, "H265");
  assert.equal(context.settings.enableCloudGsync, false);
  assert.equal(context.settings.nativeTransitionDiagnostics?.forceQueueMode, "adaptive");
  assert.equal(context.shortcuts.toggleRecording, "F12");

  const nvstContext = buildNativeStreamerSessionContext(
    {
      ...context.session,
      negotiatedStreamProfile: {
        resolution: "2560x1440",
        fps: 240,
        enableCloudGsync: false,
      },
    },
    { ...context.settings, transportMode: "nvst", codec: "AV1" },
    context.shortcuts,
  );
  assert.equal(nvstContext.settings.codec, "AV1");
  assert.equal(nvstContext.session.negotiatedStreamProfile?.codec, "AV1");
});

test("keeps native stream client mode on supported desktop platforms", () => {
  assert.equal(normalizeStreamClientModeForPlatform("native", "linux"), "native");
  assert.equal(normalizeStreamClientModeForPlatform("native", "darwin"), "native");
  assert.equal(normalizeStreamClientModeForPlatform("web", "linux"), "web");
  assert.equal(normalizeStreamClientModeForPlatform("native", "win32"), "native");
  assert.equal(normalizeStreamClientModeForPlatform("native", "android"), "web");
});

test("defaults codec selection and fallback to automatic negotiation", () => {
  assert.deepEqual(getDefaultStreamPreferences(), {
    codec: "auto",
    fallbackCodec: "auto",
    colorQuality: "8bit_420",
  });
});

test("normalizes H264 stream preferences away from high bit-depth modes", () => {
  assert.deepEqual(normalizeStreamPreferences("H264", "10bit_420"), {
    codec: "H264",
    colorQuality: "8bit_420",
    migrated: true,
  });
  assert.deepEqual(normalizeStreamPreferences("H265", "10bit_420"), {
    codec: "H265",
    colorQuality: "10bit_420",
    migrated: false,
  });
});

test("normalizes legacy and invalid saved codec preferences", () => {
  assert.equal(normalizeCodecPreference("H265"), "H265");
  assert.equal(normalizeCodecPreference("auto"), "auto");
  assert.equal(normalizeCodecPreference("VP9"), "auto");
  assert.equal(normalizeCodecPreference(undefined), "auto");
  assert.equal(normalizeFallbackCodecPreference("AV1"), "AV1");
  assert.equal(normalizeFallbackCodecPreference("invalid"), "auto");
  assert.equal(normalizeFallbackCodecPreference(null), "auto");
});

test("reports unsupported native streamer status on unknown platforms only", () => {
  assert.equal(isNativeStreamerSupportedPlatform("win32"), true);
  assert.equal(isNativeStreamerSupportedPlatform("linux"), true);
  assert.equal(isNativeStreamerSupportedPlatform("darwin"), true);
  assert.equal(isNativeStreamerSupportedPlatform("android"), false);
});

test("external renderer is selectable on Windows and required on macOS", () => {
  assert.equal(isNativeExternalRendererSupported("win32"), true);
  assert.equal(isNativeExternalRendererSupported("windows"), true);
  assert.equal(isNativeExternalRendererSupported("linux"), false);
  assert.equal(isNativeExternalRendererSupported("darwin"), true);
  assert.equal(isNativeExternalRendererRequired("darwin"), true);
  assert.equal(isNativeExternalRendererRequired("win32"), false);
  assert.equal(isNativeDirectXBackendSupported("win32"), true);
  assert.equal(isNativeDirectXBackendSupported("linux"), false);
  assert.equal(normalizeNativeExternalRendererForPlatform(true, "linux"), false);
  assert.equal(normalizeNativeExternalRendererForPlatform(true, "win32"), true);
  assert.equal(normalizeNativeExternalRendererForPlatform(false, "win32"), false);
  assert.equal(normalizeNativeExternalRendererForPlatform(false, "darwin"), true);

  const status = createUnsupportedNativeStreamerStatus();
  assert.equal(status.detected, false);
  assert.equal(status.available, false);
  assert.equal(status.supportsOfferAnswer, false);
  assert.equal(status.message, NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE);
  assert.equal(status.runtime.message, NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE);
});

test("NVST transport is limited to native sessions on supported desktop platforms", () => {
  assert.equal(isNvstTransportSupported("win32"), true);
  assert.equal(isNvstTransportSupported("darwin"), true);
  assert.equal(isNvstTransportSupported("linux"), true);
  assert.equal(isNvstTransportSupported("android"), false);
  assert.equal(normalizeTransportModeForPlatform("nvst", "win32", "native"), "nvst");
  assert.equal(normalizeTransportModeForPlatform("nvst", "darwin", "native"), "nvst");
  assert.equal(normalizeTransportModeForPlatform("nvst", "linux", "native"), "nvst");
  assert.equal(normalizeTransportModeForPlatform("nvst", "linux", "web"), "webrtc");
  assert.equal(normalizeTransportModeForPlatform("nvst", "android", "native"), "webrtc");
  assert.equal(normalizeTransportModeForPlatform("webrtc", "win32", "native"), "webrtc");
});
