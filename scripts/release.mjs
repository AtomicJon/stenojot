#!/usr/bin/env node
import { execSync } from "child_process";
import { readFileSync, writeFileSync } from "fs";
import { fileURLToPath } from "url";
import { resolve, dirname } from "path";

const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Usage: yarn release <X.Y.Z>");
  process.exit(1);
}

const tag = `v${version}`;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// Abort if the working tree has uncommitted changes.
const dirty = execSync("git status --porcelain", { encoding: "utf8" }).trim();
if (dirty) {
  console.error("Working tree is dirty. Commit or stash changes first.");
  process.exit(1);
}

// Abort if the tag already exists (avoids committing then failing on git tag).
const existingTags = execSync("git tag --list", { encoding: "utf8" });
if (existingTags.split("\n").includes(tag)) {
  console.error(`Tag ${tag} already exists.`);
  process.exit(1);
}

// Update package.json
const pkgPath = resolve(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
console.log(`Updated package.json → ${version}`);

// Update src-tauri/Cargo.toml — only the [package] section's version field
const cargoPath = resolve(root, "src-tauri/Cargo.toml");
let cargo = readFileSync(cargoPath, "utf8");
const cargoUpdated = cargo.replace(
  /^(version\s*=\s*)"[^"]*"/m,
  `$1"${version}"`
);
if (cargoUpdated === cargo) {
  console.error("Failed to update version in src-tauri/Cargo.toml — no match found.");
  process.exit(1);
}
writeFileSync(cargoPath, cargoUpdated);
console.log(`Updated src-tauri/Cargo.toml → ${version}`);

// Update src-tauri/tauri.conf.json
const tauriConfPath = resolve(root, "src-tauri/tauri.conf.json");
const tauriConf = JSON.parse(readFileSync(tauriConfPath, "utf8"));
tauriConf.version = version;
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");
console.log(`Updated src-tauri/tauri.conf.json → ${version}`);

// Sync Cargo.lock with the updated Cargo.toml
const run = (cmd) => execSync(cmd, { stdio: "inherit", cwd: root });

run("cargo generate-lockfile --manifest-path src-tauri/Cargo.toml");

run("git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json");
run(`git commit -m "chore: release ${tag}"`);
run(`git tag ${tag}`);
run("git push");
run("git push --tags");

console.log(`\nReleased ${tag}`);
