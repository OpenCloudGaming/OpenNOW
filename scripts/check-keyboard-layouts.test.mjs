import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

const readQml = (path) => readFileSync(new URL(`../opennow-qt/qml/${path}`, import.meta.url), "utf8");
const settings = readQml("state/settings/SettingsState.qml");
const facade = readQml("state/ShellStore.qml");
const desktop = readQml("desktop/settings/pages/DesktopSettingsControlsPage.qml");
const consoleSettings = readQml("screens/SettingsScreen.qml");
const expectedItems = [
  { label: "English (US)", value: "en-US" },
  { label: "English (UK)", value: "en-GB" },
  { label: "Türkçe (Q)", value: "tr-TR" },
  { label: "Deutsch", value: "de-DE" },
  { label: "Français", value: "fr-FR" },
  { label: "Español", value: "es-ES" },
  { label: "Español (Latinoamérica)", value: "es-MX" },
  { label: "Italiano", value: "it-IT" },
  { label: "Português (Portugal)", value: "pt-PT" },
  { label: "Português (Brasil)", value: "pt-BR" },
  { label: "Polski", value: "pl-PL" },
  { label: "Dansk", value: "da-DK" },
  { label: "Norsk", value: "nb-NO" },
  { label: "Svenska", value: "sv-SE" },
  { label: "Suomi", value: "fi-FI" },
  { label: "Русский", value: "ru-RU" },
  { label: "Українська", value: "uk-UA" },
  { label: "日本語", value: "ja-JP" },
  { label: "한국어", value: "ko-KR" },
  { label: "中文（简体）", value: "zh-CN" },
  { label: "中文（繁體）", value: "zh-TW" },
];

function layoutItems() {
  const declaration = settings.match(/readonly property var keyboardLayoutItems:\s*(\[[\s\S]*?\n\s*\])/);
  assert.ok(declaration, "SettingsState must own a readonly keyboard layout list");
  return JSON.parse(JSON.stringify(runInNewContext(declaration[1])));
}

test("the canonical list preserves all 21 unique locale and native-label pairs", () => {
  const items = layoutItems();
  assert.equal(items.length, 21);
  assert.equal(new Set(items.map((item) => item.value)).size, items.length);
  assert.deepEqual(items, expectedItems);
  assert.deepEqual(items.find((item) => item.value === "ru-RU"), { label: "Русский", value: "ru-RU" });
  assert.deepEqual(items.find((item) => item.value === "uk-UA"), { label: "Українська", value: "uk-UA" });
});

test("the desktop choice consumes the canonical list through the ShellStore alias", () => {
  assert.match(facade, /property alias keyboardLayoutItems:\s*settingsOwner\.keyboardLayoutItems\b/);
  const choice = desktop.match(/DesktopSettingsChoice\s*\{\s*objectName:\s*"keyboardLayoutChoice"([\s\S]*?)\n\s*\}/);
  assert.ok(choice, "the desktop keyboard picker must exist");
  assert.match(choice[1], /items:\s*ShellStore\.keyboardLayoutItems\s*\n/);
  assert.match(choice[1], /valueSetting\("keyboardLayout", "en-US"\)/);
  assert.match(choice[1], /setChoice\("keyboardLayout", value\)/);
});

test("the console choice derives aligned values and labels from the same canonical list", () => {
  const choice = consoleSettings.match(/rows\.push\((choice\("Keyboard layout",[\s\S]*?)\)\s*\n/);
  assert.ok(choice, "the console keyboard picker must exist");
  assert.match(choice[1], /ShellStore\.keyboardLayoutItems\.map\(item => item\.value\)/);
  assert.match(choice[1], /ShellStore\.keyboardLayoutItems\.map\(item => item\.label\)/);
  const result = runInNewContext(choice[1], {
    ShellStore: { keyboardLayoutItems: layoutItems() },
    choice: (title, description, key, values, labels) => ({ key, values, labels }),
  });
  assert.equal(result.key, "keyboardLayout");
  assert.deepEqual(result.values, expectedItems.map((item) => item.value));
  assert.deepEqual(result.labels, expectedItems.map((item) => item.label));
});

test("every offered layout has a native physical-key table", () => {
  const tables = readFileSync(new URL("../opennow-qt/src/streaming/PhysicalKeyMapData.h", import.meta.url), "utf8");
  const locales = [...tables.matchAll(/\{"([a-z]{2}-[A-Z]{2})",/g)].map((match) => match[1]);
  assert.deepEqual(locales.sort(), layoutItems().map((item) => item.value).sort());
});

test("both stream surfaces use the session layout rather than the live preference", () => {
  for (const path of ["desktop/stream/DesktopStreamScreen.qml", "screens/StreamScreen.qml"]) {
    const stream = readQml(path);
    assert.match(stream, /keyboardLayout: String\(\(ShellStore\.activeSession \|\| \{\}\)\.keyboardLayout \|\| "en-US"\)/);
    assert.doesNotMatch(stream, /keyboardLayout:.*ShellStore\.settings/);
  }
});
