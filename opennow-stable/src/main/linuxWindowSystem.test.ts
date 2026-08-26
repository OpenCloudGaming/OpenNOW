import assert from "node:assert/strict";
import test from "node:test";

import { resolveLinuxWindowSystem } from "./linuxWindowSystem";

test("explicit Linux Ozone platform wins over the desktop environment", () => {
  assert.equal(
    resolveLinuxWindowSystem("x11", {
      XDG_SESSION_TYPE: "wayland",
      WAYLAND_DISPLAY: "wayland-0",
    }),
    "x11",
  );
  assert.equal(resolveLinuxWindowSystem("wayland", { DISPLAY: ":0" }), "wayland");
});

test("automatic Linux window system follows a native Wayland session", () => {
  assert.equal(
    resolveLinuxWindowSystem("auto", {
      DISPLAY: ":0",
      XDG_SESSION_TYPE: "wayland",
      WAYLAND_DISPLAY: "wayland-0",
    }),
    "wayland",
  );
  assert.equal(
    resolveLinuxWindowSystem(undefined, { WAYLAND_DISPLAY: "wayland-1" }),
    "wayland",
  );
});

test("automatic Linux window system keeps X11 sessions on X11", () => {
  assert.equal(
    resolveLinuxWindowSystem(undefined, {
      DISPLAY: ":0",
      XDG_SESSION_TYPE: "x11",
    }),
    "x11",
  );
  assert.equal(resolveLinuxWindowSystem(undefined, {}), "x11");
});
