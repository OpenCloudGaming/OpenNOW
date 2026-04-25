import fs from "node:fs";

const version = process.argv[2] || process.env.RELEASE_VERSION;

if (!version) {
  console.error("Usage: node .github/scripts/set-opennow-version.mjs <version>");
  process.exit(1);
}

const semverPattern = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
if (!semverPattern.test(version)) {
  console.error(`Invalid semver version: ${version}`);
  process.exit(1);
}

const updateJson = (path, updater) => {
  const data = JSON.parse(fs.readFileSync(path, "utf8"));
  updater(data);
  fs.writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`);
};

updateJson("opennow-stable/package.json", (pkg) => {
  pkg.version = version;
});

updateJson("opennow-stable/package-lock.json", (lockfile) => {
  lockfile.version = version;
  if (lockfile.packages?.[""]) {
    lockfile.packages[""].version = version;
  }
});
