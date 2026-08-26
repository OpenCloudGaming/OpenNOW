/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  buildGfnCloudMatchHeaders,
  buildGfnGraphQlHeaders,
  buildGfnLcarsHeaders,
  buildGfnNvstClientHeaders,
  GFN_BIFROST_CLIENT_VERSION,
  GFN_CLIENT_VERSION,
  gfnBifrostUserAgentForPlatform,
  gfnUserAgentForPlatform,
  LCARS_CLIENT_ID,
} from "./clientHeaders";
import {
  STEAM_DECK_DEVICE_IDENTITY,
  configureIdentifyAsSteamDeck,
  resolveGfnDeviceIdentity,
} from "./deviceIdentity";

test("resolveGfnDeviceIdentity defaults to host desktop DESKTOP/UNKNOWN", () => {
  configureIdentifyAsSteamDeck(() => false);
  const identity = resolveGfnDeviceIdentity({ identifyAsSteamDeck: false, platform: "win32" });
  assert.equal(identity.deviceOs, "WINDOWS");
  assert.equal(identity.deviceType, "DESKTOP");
  assert.equal(identity.deviceMake, "UNKNOWN");
  assert.equal(identity.deviceModel, "UNKNOWN");
  assert.equal(identity.clientPlatformName, "Windows");
});

test("desktop identity uses native platform names", () => {
  const darwin = resolveGfnDeviceIdentity({ platform: "darwin" });
  assert.equal(darwin.clientPlatformName, "MacOSX");
  assert.equal(darwin.deviceMake, "Apple");
  if (process.platform === "darwin") {
    assert.notEqual(darwin.deviceModel, "UNKNOWN");
  }
  assert.equal(resolveGfnDeviceIdentity({ platform: "linux" }).clientPlatformName, "Linux");
});

test("Linux requests use the packaged native CEF product identity", () => {
  const userAgent = gfnUserAgentForPlatform("linux");
  assert.match(userAgent, /\(X11; Linux x86_64\)/);
  assert.match(userAgent, /NVIDIACEFClient\/HEAD\/7b92719716/);
  assert.match(userAgent, new RegExp(`GFN-PC/${GFN_CLIENT_VERSION.replaceAll(".", "\\.")}`));
});

test("macOS User-Agent identifies as the packaged GFN-PC CEF client", () => {
  const userAgent = gfnUserAgentForPlatform("darwin");
  assert.match(userAgent, /\(Macintosh; Intel Mac OS X 10_15_7\)/);
  assert.match(userAgent, /NVIDIACEFClient\/HEAD\/7b92719716/);
  assert.match(userAgent, new RegExp(`GFN-PC/${GFN_CLIENT_VERSION.replaceAll(".", "\\.")}`));
});

test("CloudMatch HTTP identity includes official Bifrost client identity", () => {
  const headers = buildGfnCloudMatchHeaders({ token: "token", deviceId: "device" });
  const userAgent = gfnBifrostUserAgentForPlatform();
  assert.equal(headers["NV-Client-Type"], "NATIVE");
  assert.equal(headers["NV-Client-Streamer"], "NVIDIA-CLASSIC");
  assert.equal(headers["x-nv-client-identity"], userAgent);
  assert.equal(headers["NV-Client-Version"], GFN_CLIENT_VERSION);
  assert.equal(headers["Content-Type"], "text/plain");
  assert.equal(headers["User-Agent"], userAgent);
  assert.match(headers["User-Agent"] ?? "", new RegExp(`GFN-PC/${GFN_BIFROST_CLIENT_VERSION}`));
  assert.equal(headers.Origin, undefined);
  assert.equal(headers.Referer, undefined);
  assert.equal(headers["nv-browser-type"], undefined);
});

test("Bifrost GridServer HTTP identity advertises the native PC client", () => {
  const headers = buildGfnNvstClientHeaders({ deviceId: "device" });
  const userAgent = gfnBifrostUserAgentForPlatform();
  assert.equal(headers["NV-Client-Type"], "NATIVE");
  assert.equal(headers["NV-Client-Streamer"], "NVIDIA-CLASSIC");
  assert.equal(headers["x-nv-client-identity"], userAgent);
  assert.equal(headers["X-Device-Id"], "device");
  assert.equal(headers["NV-Client-Version"], GFN_CLIENT_VERSION);
  assert.equal(headers["User-Agent"], userAgent);
});

test("CloudMatch defaults to the stable LCARS client identity", () => {
  const headers = buildGfnCloudMatchHeaders({ token: "token", deviceId: "device" });
  assert.equal(headers["NV-Client-ID"], LCARS_CLIENT_ID);
});

test("resolveGfnDeviceIdentity Steam Deck profile matches official headers", () => {
  const identity = resolveGfnDeviceIdentity({ identifyAsSteamDeck: true });
  assert.deepEqual(identity, STEAM_DECK_DEVICE_IDENTITY);
  assert.equal(identity.deviceOs, "STEAMOS");
  assert.equal(identity.deviceType, "CONSOLE");
  assert.equal(identity.deviceMake, "VALVE");
  assert.equal(identity.deviceModel, "STEAMDECK");
  assert.equal(identity.clientPlatformName, "SteamOS");
});

test("CloudMatch/LCARS/GraphQL headers honor Steam Deck identity", () => {
  configureIdentifyAsSteamDeck(() => true);

  const cloudMatch = buildGfnCloudMatchHeaders({
    token: "token",
    clientId: "client",
    deviceId: "device",
  });
  assert.equal(cloudMatch["NV-Device-OS"], "STEAMOS");
  assert.equal(cloudMatch["NV-Device-Type"], "CONSOLE");
  assert.equal(cloudMatch["NV-Device-Make"], "VALVE");
  assert.equal(cloudMatch["NV-Device-Model"], "STEAMDECK");

  const lcars = buildGfnLcarsHeaders({
    token: "token",
    clientType: "NATIVE",
    clientStreamer: "NVIDIA-CLASSIC",
  });
  assert.equal(lcars["nv-device-os"], "STEAMOS");
  assert.equal(lcars["nv-device-type"], "CONSOLE");
  assert.equal(lcars["nv-device-make"], "VALVE");
  assert.equal(lcars["nv-device-model"], "STEAMDECK");

  const graphQl = buildGfnGraphQlHeaders("token");
  assert.equal(graphQl["nv-device-os"], "STEAMOS");
  assert.equal(graphQl["nv-device-type"], "CONSOLE");
  assert.equal(graphQl["nv-device-make"], "VALVE");
  assert.equal(graphQl["nv-device-model"], "STEAMDECK");

  configureIdentifyAsSteamDeck(() => false);
});

test("explicit identifyAsSteamDeck option overrides settings reader", () => {
  configureIdentifyAsSteamDeck(() => false);
  const headers = buildGfnCloudMatchHeaders({
    token: "token",
    clientId: "client",
    deviceId: "device",
    identifyAsSteamDeck: true,
  });
  assert.equal(headers["NV-Device-OS"], "STEAMOS");
  assert.equal(headers["NV-Device-Type"], "CONSOLE");
});
