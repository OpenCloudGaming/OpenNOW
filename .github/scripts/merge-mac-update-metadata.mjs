#!/usr/bin/env node

import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import path from "node:path";

const artifactsDir = path.resolve(process.argv[2] ?? "release-artifacts");
const metadataPaths = ["latest-mac-x64.yml", "latest-mac-arm64.yml"]
  .map((file) => path.join(artifactsDir, file))
  .filter((file) => existsSync(file));

if (metadataPaths.length === 0) {
  console.log("No macOS update metadata found to merge.");
  process.exit(0);
}

function parseScalar(rawValue) {
  const value = rawValue.trim();
  if (/^\d+$/.test(value)) {
    return Number(value);
  }
  if ((value.startsWith("\"") && value.endsWith("\"")) || (value.startsWith("'") && value.endsWith("'"))) {
    try {
      return JSON.parse(value);
    } catch {
      return value.slice(1, -1);
    }
  }
  return value;
}

function parseLatestMac(filePath) {
  const metadata = { files: [] };
  let currentFile = null;

  for (const rawLine of readFileSync(filePath, "utf8").split(/\r?\n/)) {
    const fileStart = rawLine.match(/^\s*-\s+url:\s*(.+)$/);
    if (fileStart) {
      currentFile = { url: parseScalar(fileStart[1]) };
      metadata.files.push(currentFile);
      continue;
    }

    const fileProperty = rawLine.match(/^\s{4}([A-Za-z0-9_-]+):\s*(.+)$/);
    if (currentFile && fileProperty) {
      currentFile[fileProperty[1]] = parseScalar(fileProperty[2]);
      continue;
    }

    const topLevel = rawLine.match(/^([A-Za-z0-9_-]+):\s*(.+)$/);
    if (topLevel) {
      if (topLevel[1] !== "files") {
        metadata[topLevel[1]] = parseScalar(topLevel[2]);
      }
      currentFile = null;
    }
  }

  if (metadata.files.length === 0 && metadata.path && metadata.sha512) {
    metadata.files.push({
      url: metadata.path,
      sha512: metadata.sha512,
    });
  }

  return metadata;
}

const metadata = metadataPaths.map(parseLatestMac);
const version = metadata.find((entry) => entry.version)?.version;
if (!version) {
  throw new Error("Cannot merge macOS metadata without a version.");
}

for (const entry of metadata) {
  if (entry.version && entry.version !== version) {
    throw new Error(`Mismatched macOS metadata versions: ${entry.version} !== ${version}`);
  }
}

const seenUrls = new Set();
const files = [];
for (const entry of metadata) {
  for (const file of entry.files) {
    if (!file.url || seenUrls.has(file.url)) {
      continue;
    }
    seenUrls.add(file.url);
    files.push(file);
  }
}

if (files.length === 0) {
  throw new Error("Cannot merge macOS metadata without update files.");
}

const fallbackFile = files.find((file) => String(file.url).includes("x64")) ?? files[0];
const releaseDate = metadata.find((entry) => entry.releaseDate)?.releaseDate ?? new Date().toISOString();
const scalar = (value) => (typeof value === "number" ? String(value) : JSON.stringify(String(value)));

let output = `version: ${scalar(version)}\nfiles:\n`;
for (const file of files) {
  output += `  - url: ${scalar(file.url)}\n`;
  for (const key of ["sha512", "size", "blockMapSize"]) {
    if (file[key] !== undefined) {
      output += `    ${key}: ${scalar(file[key])}\n`;
    }
  }
}
output += `path: ${scalar(fallbackFile.url)}\n`;
if (fallbackFile.sha512 !== undefined) {
  output += `sha512: ${scalar(fallbackFile.sha512)}\n`;
}
output += `releaseDate: ${scalar(releaseDate)}\n`;

writeFileSync(path.join(artifactsDir, "latest-mac.yml"), output);
for (const filePath of metadataPaths) {
  unlinkSync(filePath);
}
