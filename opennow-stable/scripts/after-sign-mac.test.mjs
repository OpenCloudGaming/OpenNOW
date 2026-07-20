import assert from "node:assert/strict";
import { chmod, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import afterSign from "./after-sign-mac.mjs";

test("re-signs only the outer macOS app bundle", { skip: process.platform !== "darwin" }, async () => {
  const root = await mkdtemp(join(tmpdir(), "opennow-after-sign-"));
  const binDir = join(root, "bin");
  const argsFile = join(root, "codesign-args");
  const appOutDir = join(root, "output");
  const originalEnv = { ...process.env };

  try {
    await mkdir(binDir);
    await mkdir(join(appOutDir, "OpenNOW.app"), { recursive: true });
    await writeFile(join(binDir, "codesign"), '#!/bin/sh\nprintf "%s\\n" "$@" >> "$CODESIGN_ARGS_FILE"\n');
    await chmod(join(binDir, "codesign"), 0o755);
    process.env.PATH = `${binDir}:${process.env.PATH}`;
    process.env.CODESIGN_ARGS_FILE = argsFile;
    process.env.CSC_IDENTITY_AUTO_DISCOVERY = "false";

    await afterSign({
      appOutDir,
      packager: {
        appInfo: { id: "com.zortos.opennow.stable", productFilename: "OpenNOW" },
      },
    });

    const args = (await readFile(argsFile, "utf8")).trim().split("\n");
    assert.equal(args.length, 6);
    assert.equal(args.includes("--deep"), false);
    assert.deepEqual(args.slice(0, 4), ["--force", "--sign", "-", "--requirements"]);
    assert.equal(args[4], '=designated => identifier "com.zortos.opennow.stable"');
    assert.equal(args[5], join(appOutDir, "OpenNOW.app"));
  } finally {
    process.env = originalEnv;
    await rm(root, { recursive: true, force: true });
  }
});
