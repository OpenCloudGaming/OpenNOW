import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

const readQml = (path) => readFileSync(new URL(`../opennow-qt/qml/${path}`, import.meta.url), "utf8");
const settings = readQml("state/settings/SettingsState.qml");
const facade = readQml("state/ShellStore.qml");
const desktop = readQml("desktop/settings/pages/DesktopSettingsControlsPage.qml");
const consoleSettings = readQml("screens/SettingsScreen.qml");
const expectedIds = [
  "en-US", "en-GB", "tr-TR", "de-DE", "fr-FR", "es-ES_tradnl", "es-MX",
  "it-IT", "pt-PT", "pt-BR", "pl-PL", "da-DK", "nb-NO", "sv-SE", "fi-FI",
  "ru-RU", "uk-UA", "ja-106", "ko-KR", "zh-CN", "zh-TW",
];
const language = readFileSync(new URL("../native/opennow-core/src/language.rs", import.meta.url), "utf8");
const keyboardTable = language.match(/const KEYBOARDS:[\s\S]*?=\s*&\[([\s\S]*?)\n\];/);
assert.ok(keyboardTable, "the core must own the keyboard descriptors");
const descriptors = [...keyboardTable[1].matchAll(/\("([^"]+)", "([^"]+)", &\[([^\]]*)\]\)/g)]
  .map(([, value, label, aliases]) => ({ value, label, aliases: JSON.parse(`[${aliases}]`) }));

function layoutItems(saved = "en-US", choices = descriptors) {
  const declaration = settings.match(/readonly property var keyboardLayoutItems:\s*\{([\s\S]*?)\n    \}/);
  assert.ok(declaration, "SettingsState must derive its choices from the core descriptors");
  return JSON.parse(JSON.stringify(runInNewContext(`
    String.prototype.arg = function(value) { return this.replace("%1", value); };
    (function() { ${declaration[1]} })()
  `, {
    keyboardLayouts: choices,
    settings: { keyboardLayout: saved },
    i18n: { source: (text) => text },
    qsTr: (text) => text,
  })));
}

test("the core offers all 21 layouts and the UI preserves its descriptors", () => {
  const items = layoutItems();
  assert.equal(items.length, 21);
  assert.equal(new Set(items.map((item) => item.value)).size, items.length);
  assert.deepEqual(items.map((item) => item.value), expectedIds);
  assert.deepEqual(items, descriptors.map(({ value, label }) => ({ value, label })));
  assert.equal(items.find((item) => item.value === "ru-RU").label, "Russian");
  assert.equal(items.find((item) => item.value === "uk-UA").label, "Ukrainian");
});

test("saved aliases and unavailable choices remain visible without changing preferences", () => {
  for (const [saved, canonical] of [["ja-JP", "ja-106"], ["Japanese106", "ja-106"], ["es-ES", "es-ES_tradnl"]]) {
    const [item] = layoutItems(saved);
    assert.equal(item.value, saved);
    assert.equal(item.disabled, true);
    assert.equal(item.detail, `Saved legacy ID; requests ${canonical}`);
  }
  assert.equal(layoutItems("unknown")[0].detail, "Saved; layout not recognized");
  assert.equal(layoutItems("ja-106", [])[0].detail, "Saved; keyboard choices unavailable");
});

test("the desktop choice consumes the canonical list through the ShellStore alias", () => {
  assert.match(facade, /property alias keyboardLayoutItems:\s*settingsOwner\.keyboardLayoutItems\b/);
  const choice = desktop.match(/DesktopSettingsChoice\s*\{\s*objectName:\s*"keyboardLayoutChoice"([\s\S]*?)\n\s*\}/);
  assert.ok(choice, "the desktop keyboard picker must exist");
  assert.match(choice[1], /items:\s*ShellStore\.keyboardLayoutItems\s*\n/);
  assert.match(choice[1], /valueSetting\("keyboardLayout", "en-US"\)/);
  assert.match(choice[1], /setChoice\("keyboardLayout", value\)/);
});

test("the console choice consumes the same descriptors and disabled-state metadata", () => {
  const choice = consoleSettings.match(/rows\.push\((descriptorChoice\(qsTr\("Keyboard layout"\),[\s\S]*?)\)\s*\n/);
  assert.ok(choice, "the console keyboard picker must exist");
  assert.match(choice[1], /"keyboardLayout", ShellStore\.keyboardLayoutItems/);
  const items = layoutItems("ja-JP");
  const result = runInNewContext(choice[1], {
    ShellStore: { keyboardLayoutItems: items, settingsOwnerState: { keyboardLayoutDescription: "fixture" } },
    qsTr: (text) => text,
    descriptorChoice: (title, description, key, descriptors) => ({ key, descriptors }),
  });
  assert.equal(result.key, "keyboardLayout");
  assert.deepEqual(result.descriptors, items);
});

test("every offered layout has a native physical-key table", () => {
  const tables = readFileSync(new URL("../opennow-qt/src/streaming/PhysicalKeyMapData.h", import.meta.url), "utf8");
  const locales = [...tables.matchAll(/\{"([a-z]{2}-[A-Z]{2})",/g)].map((match) => match[1]);
  const tableAliases = { "ja-106": "ja-JP", "es-ES_tradnl": "es-ES" };
  assert.deepEqual(locales.sort(), descriptors.map((item) => tableAliases[item.value] || item.value).sort());
});

test("both stream surfaces use the session layout rather than the live preference", () => {
  for (const path of ["desktop/stream/DesktopStreamScreen.qml", "screens/StreamScreen.qml"]) {
    const stream = readQml(path);
    assert.match(stream, /keyboardLayout: String\(\(ShellStore\.activeSession \|\| \{\}\)\.keyboardLayout \|\| "en-US"\)/);
    assert.doesNotMatch(stream, /keyboardLayout:.*ShellStore\.settings/);
  }
});
