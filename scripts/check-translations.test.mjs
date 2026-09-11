import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

function checkCatalog(t, qml, locales = {}) {
  const root = mkdtempSync(join(tmpdir(), "opennow-locales-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "scripts"));
  mkdirSync(join(root, "locales"));
  mkdirSync(join(root, "opennow-qt", "qml", "nested"), { recursive: true });
  copyFileSync(new URL("./check-translations.mjs", import.meta.url), join(root, "scripts", "check-translations.mjs"));
  writeFileSync(join(root, "opennow-qt", "qml", "nested", "Example.qml"), qml);
  writeFileSync(join(root, "opennow-qt", "qml", "ignored.tsx"), 't("missing.legacy.key")');
  const catalogs = {
    "en.json": JSON.stringify({ common: { back: "Back", quoted: 'Say "hello"', multiline: "Line\nTwo" } }),
    "de.json": JSON.stringify({ common: { back: "Zurück" } }),
    ...locales,
  };
  for (const [name, contents] of Object.entries(catalogs)) {
    writeFileSync(join(root, "locales", name), contents);
  }
  return spawnSync(process.execPath, [join(root, "scripts", "check-translations.mjs")], { encoding: "utf8" });
}

test("checks nested Qt sources and keyed translations without Electron or npm dependencies", (t) => {
  const result = checkCatalog(t, String.raw`
    text: qsTr("Back")
    label: I18n.source("Say \"hello\"", I18n.revision)
    hint: qsTr("Line\nTwo")
    keyed: I18n.text("common.back")
  `);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /3 QML source texts and 1 keys checked/);
});

test("rejects missing Qt source text", (t) => {
  const result = checkCatalog(t, 'text: qsTr("Not translated")');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Missing QML source text/);
  assert.match(result.stderr, /Not translated/);
});

test("rejects missing keyed translations", (t) => {
  const result = checkCatalog(t, 'text: I18n.text("common.missing")');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Missing translation keys/);
  assert.match(result.stderr, /common.missing/);
});

test("rejects malformed translated catalogs", (t) => {
  const result = checkCatalog(t, 'text: qsTr("Back")', { "de.json": "{" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /de.json is not valid JSON/);
});

test("rejects malformed English source catalogs", (t) => {
  const result = checkCatalog(t, 'text: qsTr("Back")', { "en.json": "{" });
  assert.equal(result.status, 1);
});

test("allows untranslated catalogs and dynamic runtime translation inputs", (t) => {
  const result = checkCatalog(t, 'text: I18n.source(modelData.label, I18n.revision)', {
    "de.json": "",
    "fr.json": "{}",
  });
  assert.equal(result.status, 0, result.stderr);
});
