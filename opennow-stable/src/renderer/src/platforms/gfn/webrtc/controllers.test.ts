/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  DecoderPressureController,
  type DecoderPressureSignal,
  type DecoderPressureState,
} from "./decoderPressureController";
import {
  GamepadController,
  selectGamepadPollIntervalMs,
  shouldSendGamepadPacket,
} from "./gamepadController";
import { InputChannelPolicyController } from "./inputChannelPolicy";
import { InputEncoder } from "../inputProtocol";

const pressureSignal: DecoderPressureSignal = {
  active: true,
  reason: "backlog_and_drop",
  backlogFrames: 50,
  dropRatePercent: 7,
};

test("decoder recovery waits for three pressure polls and clears after six stable polls", async () => {
  const states: DecoderPressureState[] = [];
  let keyframeRequests = 0;
  const controller = new DecoderPressureController({
    log: () => {},
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      keyframeRequests++;
    },
    setMaxBitrateKbps: async () => true,
    onStateChange: (state) => states.push(state),
    now: () => 2_000,
  });

  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  assert.equal(keyframeRequests, 0);

  await controller.recover(pressureSignal);
  assert.equal(keyframeRequests, 1);
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 1,
    recoveryAction: "signaling_keyframe",
  });

  const stableSignal = { ...pressureSignal, active: false, reason: "stable" };
  for (let index = 0; index < 5; index++) {
    await controller.recover(stableSignal);
  }
  assert.equal(states.at(-1)?.active, true);

  await controller.recover(stableSignal);
  assert.deepEqual(states.at(-1), {
    active: false,
    recoveryAttempts: 0,
    recoveryAction: "none",
  });
});

test("decoder recovery preserves bitrate state when no wire update is applied", async () => {
  const states: DecoderPressureState[] = [];
  const logs: string[] = [];
  const requestedBitrates: number[] = [];
  let updateApplied = false;
  let now = 2_000;
  const peerConnection = {
    localDescription: { type: "answer", sdp: "v=0\r\n" },
    getSenders: () => [],
  } as unknown as RTCPeerConnection;
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => peerConnection,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      throw new Error("unavailable");
    },
    setMaxBitrateKbps: async (kbps) => {
      requestedBitrates.push(kbps);
      return updateApplied;
    },
    onStateChange: (state) => states.push(state),
    now: () => now,
  });
  controller.initializeBitrate(10_000);

  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);

  assert.deepEqual(requestedBitrates, [8_500]);
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 0,
    recoveryAction: "none",
  });
  assert.equal(logs.some((message) => message.includes("bitrate ceiling stepped down")), false);

  updateApplied = true;
  now = 4_000;
  await controller.recover(pressureSignal);

  assert.deepEqual(requestedBitrates, [8_500, 8_500]);
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 1,
    recoveryAction: "bitrate_step_down",
  });
  assert.equal(
    logs.some((message) => message.includes("bitrate ceiling stepped down 10000 -> 8500 kbps")),
    true,
  );
});

test("input policy preserves native, partially-reliable, and fallback routes", () => {
  const nativePackets: Array<{ payload: Uint8Array; partiallyReliable: boolean }> = [];
  const reliablePackets: Uint8Array[] = [];
  const channelPackets: Uint8Array[] = [];
  let nativeActive = true;
  let channelOpen = true;
  const channel = {
    get readyState() {
      return channelOpen ? "open" : "closed";
    },
    send: (payload: Uint8Array) => channelPackets.push(payload),
  } as unknown as RTCDataChannel;
  const controller = new InputChannelPolicyController(
    {
      partialReliableThresholdMs: 300,
      hidDeviceMask: 0xffff,
      enablePartiallyReliableTransferGamepad: 0xffff,
      enablePartiallyReliableTransferHid: 0xffff,
    },
    {
      isNativeInputActive: () => nativeActive,
      getPartiallyReliableChannel: () => channel,
      sendNativeInput: (payload, partiallyReliable) => {
        nativePackets.push({ payload, partiallyReliable });
      },
      sendReliable: (payload) => reliablePackets.push(payload),
    },
  );
  const payload = new Uint8Array([1, 2, 3]);

  controller.sendPartiallyReliable(payload);
  assert.deepEqual(nativePackets, [{ payload, partiallyReliable: true }]);

  nativeActive = false;
  controller.sendPartiallyReliable(payload);
  assert.equal(channelPackets.length, 1);

  channelOpen = false;
  controller.sendPartiallyReliable(payload);
  assert.deepEqual(reliablePackets, [payload]);
});

test("gamepad polling and keepalive decisions preserve adaptive timing", () => {
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: false,
    visible: true,
    connectedCount: 1,
    inputBlocked: false,
  }), 100);
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: true,
    visible: true,
    connectedCount: 1,
    inputBlocked: true,
  }), 16);
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: true,
    visible: true,
    connectedCount: 1,
    inputBlocked: false,
  }), 4);
  assert.equal(shouldSendGamepadPacket(false, 99), false);
  assert.equal(shouldSendGamepadPacket(false, 100), true);
  assert.equal(shouldSendGamepadPacket(true, 0), true);
});

test("standard controller haptics advertise and apply legacy and Oc rumble", () => {
  const effects: Array<{
    startDelay: number;
    duration: number;
    weakMagnitude: number;
    strongMagnitude: number;
  }> = [];
  const gamepad = {
    connected: true,
    id: "Xbox Wireless Controller",
    index: 0,
    vibrationActuator: {
      playEffect: async (_type: string, options: {
        startDelay: number;
        duration: number;
        weakMagnitude: number;
        strongMagnitude: number;
      }) => {
        effects.push(options);
      },
    },
  } as unknown as Gamepad;
  const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { getGamepads: () => [gamepad] },
  });

  try {
    const encoder = new InputEncoder();
    encoder.setProtocolVersion(3);
    const reliable: Uint8Array[] = [];
    const controller = new GamepadController({
      inputEncoder: encoder,
      isInputReady: () => true,
      isInputPaused: () => false,
      isNativeInputActive: () => false,
      isNativeElectronInputBridge: () => false,
      isReliableChannelOpen: () => true,
      canSendPartiallyReliableGamepad: () => true,
      sendPartiallyReliable: () => {},
      sendReliable: (payload) => reliable.push(payload),
      onConnectedGamepadsChanged: () => {},
      log: () => {},
    });

    controller.refreshHapticsAdvertisement();
    assert.equal(reliable.length, 1);
    assert.deepEqual(Array.from(reliable[0]?.slice(-6) ?? []), [13, 0, 0, 0, 0, 1]);

    controller.handleHapticsMessage(new Uint8Array([
      0x0b, 0x01,
      0x01, 0x00, 0x06, 0x00, 0x00, 0x00,
      0x00, 0x80, 0xff, 0xff,
    ]));
    assert.equal(effects.length, 1);
    assert.equal(effects[0]?.duration, 500);
    assert.ok(Math.abs((effects[0]?.weakMagnitude ?? 0) - (0x8000 / 0xffff)) < 0.0001);
    assert.equal(effects[0]?.strongMagnitude, 1);

    controller.handleHapticsMessage(new Uint8Array([
      0x22,
      0x11, 0x00, 0x00, 0x00,
      0x06, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]));
    assert.equal(effects.length, 2);
    assert.deepEqual(effects[1], {
      startDelay: 0,
      duration: 0,
      weakMagnitude: 0,
      strongMagnitude: 0,
    });
  } finally {
    if (originalNavigator) {
      Object.defineProperty(globalThis, "navigator", originalNavigator);
    } else {
      Reflect.deleteProperty(globalThis, "navigator");
    }
  }
});
