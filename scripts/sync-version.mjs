#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const version = (
  process.env.ARG_0 ||
  execSync("git describe --tags --abbrev=0", { cwd: root, encoding: "utf8" }).trim()
).replace(/^v/, "");

if (!version || version === "0.0.0") {
  console.error("No version found");
  process.exit(1);
}

function updateJson(filepath) {
  const content = JSON.parse(readFileSync(filepath, "utf8"));
  content.version = version;
  writeFileSync(filepath, JSON.stringify(content, null, 2) + "\n");
}

function updateToml(filepath) {
  let content = readFileSync(filepath, "utf8");
  content = content.replace(/^(version\s*=\s*")[\d.]+(".*)/m, `$1${version}$2`);
  writeFileSync(filepath, content);
}

const targets = [
  resolve(root, "package.json"),
  resolve(root, "src-tauri/tauri.conf.json"),
  resolve(root, "src-tauri/Cargo.toml"),
];

for (const t of targets) {
  if (t.endsWith(".toml")) updateToml(t);
  else updateJson(t);
}

execSync(`git add ${targets.join(" ")}`, { cwd: root, stdio: "inherit" });

console.log(`Synced version ${version} to all packages`);
