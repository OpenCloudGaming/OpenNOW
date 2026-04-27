import test from "node:test";
import assert from "node:assert/strict";

import { chooseUpdaterKind, normalizeLinuxPackageType } from "./updaterPlatform";

function commandSet(...commands: string[]): (command: string) => boolean {
  const available = new Set(commands);
  return (command) => available.has(command);
}

test("normalizes supported Linux package types", () => {
  assert.equal(normalizeLinuxPackageType("deb"), "deb");
  assert.equal(normalizeLinuxPackageType(" RPM\n"), "rpm");
  assert.equal(normalizeLinuxPackageType("pacman"), "pacman");
  assert.equal(normalizeLinuxPackageType("AppImage"), null);
  assert.equal(normalizeLinuxPackageType(undefined), null);
});

test("uses pacman updater for converted deb installs on pacman systems", () => {
  assert.equal(
    chooseUpdaterKind({
      platform: "linux",
      packageType: "deb",
      hasCommand: commandSet("pacman"),
    }),
    "pacman",
  );
});

test("keeps default updater when Debian tools are available", () => {
  assert.equal(
    chooseUpdaterKind({
      platform: "linux",
      packageType: "deb",
      hasCommand: commandSet("apt", "pacman"),
    }),
    "default",
  );
  assert.equal(
    chooseUpdaterKind({
      platform: "linux",
      packageType: "deb",
      hasCommand: commandSet("dpkg", "pacman"),
    }),
    "default",
  );
});

test("keeps default updater outside the converted deb case", () => {
  assert.equal(
    chooseUpdaterKind({
      platform: "darwin",
      packageType: "deb",
      hasCommand: commandSet("pacman"),
    }),
    "default",
  );
  assert.equal(
    chooseUpdaterKind({
      platform: "linux",
      packageType: "pacman",
      hasCommand: commandSet("pacman"),
    }),
    "default",
  );
});
