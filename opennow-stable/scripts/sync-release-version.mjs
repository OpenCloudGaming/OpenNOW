#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const semverPattern = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

function normalizeVersion(rawVersion) {
  if (!rawVersion) {
    throw new Error('Release version is required as an argument or RELEASE_VERSION environment variable.');
  }

  let version = rawVersion.trim();
  if (version.startsWith('opennow-stable-v')) {
    version = version.slice('opennow-stable-v'.length);
  } else if (version.startsWith('v')) {
    version = version.slice(1);
  }

  if (!semverPattern.test(version)) {
    throw new Error(`Invalid semver version: ${rawVersion}`);
  }

  return version;
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

async function writeJson(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

function parseArgs(argv) {
  const args = argv.slice(2);
  const check = args.includes('--check');
  const versionArg = args.find((arg) => arg !== '--check');
  return { check, versionArg };
}

function validateCurrentVersions(packageJson, packageLock, version) {
  const mismatches = [];

  if (packageJson.version !== version) {
    mismatches.push(`package.json version is ${packageJson.version}`);
  }

  if (packageLock.version !== version) {
    mismatches.push(`package-lock.json version is ${packageLock.version}`);
  }

  if (packageLock.packages?.['']?.version !== version) {
    mismatches.push(`package-lock.json packages[""].version is ${packageLock.packages?.['']?.version}`);
  }

  if (mismatches.length > 0) {
    throw new Error(`Committed package versions do not match release version ${version}: ${mismatches.join('; ')}.`);
  }
}

try {
  const { check, versionArg } = parseArgs(process.argv);
  const version = normalizeVersion(versionArg ?? process.env.RELEASE_VERSION);
  const scriptDir = dirname(fileURLToPath(import.meta.url));
  const appDir = join(scriptDir, '..');
  const packageJsonPath = join(appDir, 'package.json');
  const packageLockPath = join(appDir, 'package-lock.json');

  const packageJson = await readJson(packageJsonPath);
  const packageLock = await readJson(packageLockPath);

  if (check) {
    validateCurrentVersions(packageJson, packageLock, version);
    console.log(`Package versions already match ${version}.`);
    process.exit(0);
  }

  packageJson.version = version;
  packageLock.version = version;
  packageLock.packages ??= {};
  packageLock.packages[''] ??= {};
  packageLock.packages[''].version = version;

  await writeJson(packageJsonPath, packageJson);
  await writeJson(packageLockPath, packageLock);
  console.log(`Synced package versions to ${version}.`);
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
