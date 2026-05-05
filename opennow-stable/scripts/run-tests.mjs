import { readdir } from "node:fs/promises";
import { join, sep } from "node:path";
import { spawn } from "node:child_process";

async function discoverTests(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const files = await Promise.all(entries.map(async (entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return discoverTests(path);
    return entry.isFile() && entry.name.endsWith(".test.ts") ? [path] : [];
  }));
  return files.flat();
}

const tests = (await discoverTests("src"))
  .map((path) => path.split(sep).join("/"))
  .sort();

if (tests.length === 0) {
  console.error("No test files found under src/**/*.test.ts");
  process.exit(1);
}

const child = spawn(process.platform === "win32" ? "npx.cmd" : "npx", ["tsx", "--test", ...tests], {
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    console.error(`Test runner terminated by signal ${signal}`);
    process.exit(1);
  }
  process.exit(code ?? 1);
});
