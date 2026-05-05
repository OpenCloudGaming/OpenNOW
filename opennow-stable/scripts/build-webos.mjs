import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { build as esbuild } from "esbuild";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const outputDir = join(root, "dist-webos");
const appinfoPath = join(root, "webos", "appinfo.json");
const packagePath = join(root, "package.json");
const serviceDir = join(root, "webos", "services", "com.zortos.opennow.stable.service");

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
      env: process.env,
    });

    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
      }
    });
    child.on("error", reject);
  });
}

const packageJson = JSON.parse(await readFile(packagePath, "utf8"));
await esbuild({
  entryPoints: [join(root, "webos", "service-src", "service.ts")],
  outfile: join(serviceDir, "service.js"),
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node18",
  sourcemap: false,
  external: ["webos-service"],
  logLevel: "info",
  alias: {
    "@shared": join(root, "src", "shared"),
  },
});
await run("vite", ["build", "--config", "webos.vite.config.ts"]);

const appinfo = JSON.parse(await readFile(appinfoPath, "utf8"));
appinfo.version = packageJson.version;
await writeFile(join(outputDir, "appinfo.json"), `${JSON.stringify(appinfo, null, 2)}\n`, "utf8");

const logoSource = join(root, "..", "logo.png");
await cp(logoSource, join(outputDir, "icon.png"));
await cp(logoSource, join(outputDir, "largeIcon.png"));

const serviceSource = join(root, "webos", "services");
if (existsSync(serviceSource)) {
  await mkdir(join(outputDir, "services"), { recursive: true });
  await cp(serviceSource, join(outputDir, "services"), { recursive: true });
}

console.log(`webOS app staged in ${outputDir}`);
