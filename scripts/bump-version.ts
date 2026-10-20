// Bumps the app version in every file that carries it.
// Usage: bun run bump <major|minor|patch|X.Y.Z>

import { join } from "node:path";

const root = join(import.meta.dir, "..");

// Group 1 is everything before the version, group 2 the version, group 3 what follows it.
const files = [
  { path: "package.json", pattern: /("version": ")([^"]+)(")/ },
  { path: "src-tauri/tauri.conf.json", pattern: /("version": ")([^"]+)(")/ },
  { path: "src-tauri/Cargo.toml", pattern: /(^version = ")([^"]+)(")/m },
  { path: "src-tauri/Cargo.lock", pattern: /(name = "cvd"\nversion = ")([^"]+)(")/ },
];

const SEMVER = /^\d+\.\d+\.\d+$/;

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function bump(current: string, kind: string): string {
  const [major, minor, patch] = current.split(".").map(Number);
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

const arg = Bun.argv[2];
if (!arg || !(arg === "major" || arg === "minor" || arg === "patch" || SEMVER.test(arg))) {
  fail("usage: bun run bump <major|minor|patch|X.Y.Z>");
}

const entries = await Promise.all(
  files.map(async (file) => {
    const text = await Bun.file(join(root, file.path)).text();
    const match = text.match(file.pattern);
    if (!match) fail(`no version found in ${file.path}`);
    return { ...file, text, current: match[2] };
  }),
);

// package.json is the source of truth for relative bumps.
const current = entries[0].current;
const next = SEMVER.test(arg) ? arg : bump(current, arg);

for (const entry of entries) {
  if (entry.current !== current) {
    console.warn(`warning: ${entry.path} was at ${entry.current}, not ${current}`);
  }
}

for (const entry of entries) {
  const updated = entry.text.replace(entry.pattern, (_, before, _old, after) => `${before}${next}${after}`);
  await Bun.write(join(root, entry.path), updated);
  console.log(`${entry.path}: ${entry.current} -> ${next}`);
}
