import { existsSync } from "node:fs";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const docsDir = resolve(here, "..");
const repoRoot = resolve(docsDir, "../..");

const files = readdirSync(docsDir).filter((name) => name.endsWith(".md"));
const cite = /`((?:native|opennow-qt)\/[^`\s]+)`/g;

let missing = 0;
let checked = 0;
for (const name of files) {
  const text = readFileSync(join(docsDir, name), "utf8");
  for (const match of text.matchAll(cite)) {
    const rel = match[1].replace(/:.+$/, "");
    const abs = resolve(repoRoot, rel);
    checked += 1;
    if (!existsSync(abs)) {
      missing += 1;
      console.error(`missing ${rel} (cited in ${name})`);
    }
  }
}

if (checked === 0) {
  console.error("no repo path cites found");
  process.exit(1);
}
if (missing > 0) {
  process.exit(1);
}
console.log(`ok ${checked} cites in ${files.length} markdown files`);
