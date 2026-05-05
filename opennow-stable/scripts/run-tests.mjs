import { appendFile } from "node:fs/promises";
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

function plural(count, singular, pluralName = `${singular}s`) {
  return `${count} ${count === 1 ? singular : pluralName}`;
}

function parseNodeTestSummary(output) {
  const summary = {};
  for (const line of output.split(/\r?\n/)) {
    const match = line.match(/(?:ℹ|#)\s+(tests|pass|fail|skipped|todo|cancelled)\s+(\d+)/);
    if (match) summary[match[1]] = Number.parseInt(match[2], 10);
  }
  return Number.isInteger(summary.tests) ? summary : null;
}

async function appendGitHubSummary({ tests, output, exitCode }) {
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryPath) return;

  const parsed = parseNodeTestSummary(output);
  const fileIcon = exitCode === 0 ? "✅" : "❌";
  const resultIcon = parsed?.fail || parsed?.cancelled || exitCode !== 0 ? "❌" : "✅";
  const passedFiles = exitCode === 0 ? tests.length : 0;
  const totalResults = parsed
    ? parsed.pass + parsed.fail + parsed.skipped + parsed.todo + parsed.cancelled
    : null;
  const otherParts = parsed
    ? [
      ["fail", parsed.fail],
      ["skipped", parsed.skipped],
      ["todo", parsed.todo],
      ["cancelled", parsed.cancelled],
    ].filter(([, count]) => count > 0).map(([name, count]) => plural(count, name))
    : [];

  const lines = [
    "## Node Test Report",
    "",
    "### Summary",
    "",
    parsed
      ? `Test Files: ${fileIcon} ${plural(passedFiles, "pass", "passes")} · ${plural(tests.length, "total")}`
      : `Test Files: ${fileIcon} ${plural(tests.length, "discovered")} · exit code ${exitCode}`,
    parsed
      ? `Test Results: ${resultIcon} ${plural(parsed.pass, "pass", "passes")} · ${plural(totalResults, "total")}`
      : "Test Results: ❌ summary parsing unavailable; see action log for details",
  ];

  if (otherParts.length > 0) {
    lines.push(`Other: ${otherParts.join(" · ")}`);
  }

  lines.push("", "Job summary generated at run-time", "");
  await appendFile(summaryPath, `${lines.join("\n")}\n`);
}

const tests = (await discoverTests("src"))
  .map((path) => path.split(sep).join("/"))
  .sort();

if (tests.length === 0) {
  console.error("No test files found under src/**/*.test.ts");
  process.exit(1);
}

let output = "";
const child = spawn(process.platform === "win32" ? "npx.cmd" : "npx", ["tsx", "--test", ...tests], {
  stdio: ["inherit", "pipe", "pipe"],
});

child.stdout.on("data", (chunk) => {
  output += chunk.toString();
  process.stdout.write(chunk);
});

child.stderr.on("data", (chunk) => {
  output += chunk.toString();
  process.stderr.write(chunk);
});

child.on("exit", async (code, signal) => {
  const exitCode = signal ? 1 : code ?? 1;
  if (signal) {
    console.error(`Test runner terminated by signal ${signal}`);
  }

  try {
    await appendGitHubSummary({ tests, output, exitCode });
  } catch (error) {
    console.error("Failed to append GitHub test summary:", error);
  }

  process.exit(exitCode);
});
