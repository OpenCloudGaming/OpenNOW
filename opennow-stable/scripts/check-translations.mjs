import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const repoRoot = join(import.meta.dirname, "..", "..");
const rendererSrc = join(repoRoot, "opennow-stable", "src", "renderer", "src");
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
    if (!/\.(ts|tsx)$/.test(entry.name)) return [];
    return [path];
  });
}

const sourceLocale = JSON.parse(readFileSync(sourceLocalePath, "utf8"));
const sourceKeys = new Set(flattenKeys(sourceLocale));
const usedKeys = new Set();
const keyPattern = /\bt\(\s*["']([^"']+)["']/g;

for (const file of listSourceFiles(rendererSrc)) {
  const source = readFileSync(file, "utf8");
  let match;
  while ((match = keyPattern.exec(source)) !== null) {
    usedKeys.add(match[1]);
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
  console.log(`Translation keys ok (${usedKeys.size} used keys checked).`);
}
