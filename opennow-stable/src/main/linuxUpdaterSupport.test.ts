/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { getLinuxUpdaterSupport } from "./linuxUpdaterSupport";

const noCommands = () => false;

test("uses the AppImage updater path when running from an AppImage", () => {
  const support = getLinuxUpdaterSupport({
    platform: "linux",
    env: { APPIMAGE: "/home/user/OpenNOW.AppImage" },
    commandExists: noCommands,
  });

  assert.equal(support.supported, true);
  assert.equal(support.packageKind, "appimage");
});

test("uses the deb updater path only when Debian package tools are available", () => {
  const support = getLinuxUpdaterSupport({
    platform: "linux",
    env: {},
    commandExists: (command) => command === "dpkg",
    readOsRelease: () => "ID=debian\n",
  });

  assert.equal(support.supported, true);
  assert.equal(support.packageKind, "deb");
});

test("does not use the deb updater path on non-Debian distributions", () => {
  const support = getLinuxUpdaterSupport({
    platform: "linux",
    env: {},
    commandExists: (command) => command === "dpkg",
    readOsRelease: () => "ID=cachyos\nID_LIKE=arch\n",
  });

  assert.equal(support.supported, false);
  assert.equal(support.packageKind, "unsupported");
});

test("disables Linux auto updates instead of selecting deb on systems without dpkg or apt", () => {
  const support = getLinuxUpdaterSupport({
    platform: "linux",
    env: {},
    commandExists: noCommands,
    readOsRelease: () => "ID=cachyos\nID_LIKE=arch\n",
  });

  assert.equal(support.supported, false);
  assert.equal(support.packageKind, "unsupported");
  assert.match(support.message ?? "", /Download the AppImage/);
});

test("does not apply Linux package checks to other platforms", () => {
  const support = getLinuxUpdaterSupport({
    platform: "darwin",
    env: {},
    commandExists: noCommands,
  });

  assert.equal(support.supported, true);
  assert.equal(support.packageKind, "native");
});
