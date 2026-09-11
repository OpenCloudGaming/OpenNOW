import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const repoRoot = join(import.meta.dirname, "..");
const qmlSrc = join(repoRoot, "opennow-qt", "qml");
const localesDir = join(repoRoot, "locales");
const sourceLocalePath = join(localesDir, "en.json");

function flattenKeys(value, prefix = "") {
  if (typeof value === "string") {
    return [prefix];
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return [];
  }
  return Object.entries(value).flatMap(([key, child]) => flattenKeys(child, prefix ? `${prefix}.${key}` : key));
}

function listSourceFiles(dir) {
  const entries = readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listSourceFiles(path);
    if (!entry.isFile() || !entry.name.endsWith(".qml")) return [];
    return [path];
  });
}

const sourceLocale = JSON.parse(readFileSync(sourceLocalePath, "utf8"));
const sourceKeys = new Set(flattenKeys(sourceLocale));
const sourceTexts = new Set([...sourceKeys].map((key) =>
  key.split(".").reduce((value, part) => value[part], sourceLocale)));
const usedKeys = new Set();
const usedTexts = new Set();
const keyPattern = /\bI18n\.text\(\s*("(?:\\.|[^"\\])*")/g;
const textPattern = /\b(?:qsTr|I18n\.source)\(\s*("(?:\\.|[^"\\])*")/g;

for (const file of listSourceFiles(qmlSrc)) {
  const source = readFileSync(file, "utf8");
  let match;
  while ((match = keyPattern.exec(source)) !== null) {
    usedKeys.add(JSON.parse(match[1]));
  }
  while ((match = textPattern.exec(source)) !== null) {
    const text = JSON.parse(match[1]);
    if (text.length > 0) usedTexts.add(text);
  }
}

const missing = [...usedKeys].filter((key) => !sourceKeys.has(key)).sort();
if (missing.length > 0) {
  console.error("Missing translation keys in locales/en.json:");
  for (const key of missing) {
    console.error(`- ${key}`);
  }
  process.exitCode = 1;
}

const missingTexts = [...usedTexts].filter((text) => !sourceTexts.has(text)).sort();
if (missingTexts.length > 0) {
  console.error("Missing QML source text in locales/en.json:");
  for (const text of missingTexts) {
    console.error(`- ${text}`);
  }
  process.exitCode = 1;
}

for (const fileName of readdirSync(localesDir).sort()) {
  if (!fileName.endsWith(".json") || fileName === "en.json") continue;
  const path = join(localesDir, fileName);
  if (!statSync(path).isFile()) continue;
  const raw = readFileSync(path, "utf8");
  if (raw.trim().length === 0) continue;
  try {
    JSON.parse(raw);
  } catch (error) {
    console.error(`${relative(repoRoot, path)} is not valid JSON:`, error);
    process.exitCode = 1;
  }
}

if (!process.exitCode) {
  console.log(`Translations ok (${usedTexts.size} QML source texts and ${usedKeys.size} keys checked).`);
}
