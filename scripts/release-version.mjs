import { appendFileSync, readFileSync } from "node:fs";
import assert from "node:assert/strict";

const read = (file) =>
  readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
const { version } = JSON.parse(read("package.json"));
assert.match(
  version,
  /^\d+\.\d+\.\d+$/,
  "Release version must be stable semver",
);
assert.equal(
  process.env.GITHUB_REF,
  `refs/tags/v${version}`,
  "Run this workflow on the matching version tag",
);
const manifest = read("src-tauri/Cargo.toml").split(/\n\[/)[0];
assert.equal(
  manifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
  version,
  "Cargo package version mismatch",
);
const lock = read("src-tauri/Cargo.lock")
  .split("[[package]]")
  .find((entry) => /^name = "bangumi-manager"$/m.test(entry));
assert.equal(
  lock?.match(/^version = "([^"]+)"/m)?.[1],
  version,
  "Cargo lock version mismatch",
);
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
assert.ok(
  config.version === "../package.json" || config.version === version,
  "Tauri version mismatch",
);
console.log(`Validated release v${version}`);
if (process.env.GITHUB_OUTPUT)
  appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`);
