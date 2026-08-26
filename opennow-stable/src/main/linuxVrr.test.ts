/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import {
  parseHyprlandVrr,
  parseXrandrOutputs,
  resolveLinuxVrrWindowSystem,
  resolveX11Vrr,
} from "./linuxVrr";

test("Hyprland VRR requires compositor permission and a high-refresh display", () => {
  const monitors = JSON.stringify([{
    name: "DP-2",
    description: "VRR Display",
    refreshRate: 165.08,
    focused: true,
    disabled: false,
    dpmsStatus: true,
    vrr: false,
  }]);
  const disabled = parseHyprlandVrr('{"int":0}', monitors);
  const enabled = parseHyprlandVrr('{"int":1}', monitors);

  assert.equal(disabled.displayCapable, false);
  assert.match(disabled.reason, /vrr-disabled/);
  assert.equal(enabled.displayCapable, true);
  assert.equal(enabled.active, false);
  assert.equal(enabled.refreshHz, 165.08);
});

test("Hyprland reports VRR active only when the compositor activates it", () => {
  const result = parseHyprlandVrr('{"int":2}', JSON.stringify([{
    name: "DP-1",
    refreshRate: 144,
    focused: true,
    vrr: true,
  }]));

  assert.equal(result.displayCapable, true);
  assert.equal(result.active, true);
});

test("Hyprland content-type-only VRR is rejected until SDL can advertise game content", () => {
  const result = parseHyprlandVrr('{"int":3}', JSON.stringify([{
    name: "DP-1",
    refreshRate: 240,
    focused: true,
    vrr: false,
  }]));

  assert.equal(result.displayCapable, false);
  assert.match(result.reason, /content-type-required/);
});

test("RandR parser keeps the active mode and standard VRR property", () => {
  const outputs = parseXrandrOutputs(`DP-0 connected primary 2560x1440+0+0
\tvrr_capable: 1
   2560x1440     165.00*+ 144.00
HDMI-0 disconnected
`);

  assert.deepEqual(outputs, [{
    name: "DP-0",
    primary: true,
    refreshHz: 165,
    vrrCapable: true,
  }]);
  assert.equal(resolveX11Vrr("DP-0 connected primary\n\tvrr_capable: 1\n", false).displayCapable, true);
});

test("NVIDIA X11 fallback is limited to one high-refresh display", () => {
  const oneDisplay = "DP-0 connected primary\n   2560x1440     165.00*+\n";
  const twoDisplays = `${oneDisplay}HDMI-0 connected\n   1920x1080     144.00*+\n`;

  assert.equal(resolveX11Vrr(oneDisplay, true).displayCapable, true);
  assert.equal(resolveX11Vrr(twoDisplays, true).displayCapable, false);
});

test("Linux window system resolution prefers Wayland", () => {
  assert.equal(resolveLinuxVrrWindowSystem({ WAYLAND_DISPLAY: "wayland-1", DISPLAY: ":0" }), "wayland");
  assert.equal(resolveLinuxVrrWindowSystem({ DISPLAY: ":0" }), "x11");
  assert.equal(resolveLinuxVrrWindowSystem({}), "unknown");
});
