/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo, GameVariant } from "./gfn";
import {
  OWNED_LIBRARY_STATUSES,
  buildNativeStreamerSessionContext,
  isEpicStore,
  isGameInLibrary,
  isOwnedLibraryStatus,
  isOwnedVariant,
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
      nativeStreamerBackend: "gstreamer",
      nativeCloudGsyncMode: "auto",
      nativeTransitionDiagnostics: {
        forceQueueMode: "adaptive",
      },
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
  assert.equal(context.session.negotiatedStreamProfile?.codec, "H265");
  assert.equal(context.settings.enableCloudGsync, false);
  assert.equal(context.settings.nativeTransitionDiagnostics?.forceQueueMode, "adaptive");
});
