#!/usr/bin/env node
/**
 * Test-safety guard: tests must never be able to reach the installed application's data
 * (database, notes, attachments, backups). All test data lives in marker-protected roots under the
 * system temp directory (see src-tauri/tests/common/mod.rs and src-tauri/src/storage/paths.rs).
 *
 * This script fails when Rust test code names the real app-data location or resolves it through
 * the environment. It scans every integration test under src-tauri/tests and the `#[cfg(test)]`
 * modules inside src-tauri/src.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const FORBIDDEN = [
  [/com\.personalnotepad/, "the installed app's identifier / data folder name"],
  [/app_data_dir/, "Tauri's real app-data resolver"],
  [/\.local\/share/, "the real XDG data directory"],
  [/XDG_DATA_HOME/, "the real XDG data directory (environment)"],
  [/env::var(?:_os)?\s*\(\s*"HOME"/, "the user's home directory (environment)"],
  [/home_dir\s*\(/, "the user's home directory"],
  [/default_data_dir\s*\(/, "the installed app's default data directory"],
  [/resolve_app_data_root\s*\(/, "app-data resolution (tests must build isolated roots instead)"],
];

function* rustFiles(dir) {
  for (const entry of readdirSync(dir)) {
    if (entry === "target" || entry === "node_modules") continue;
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) yield* rustFiles(full);
    else if (full.endsWith(".rs")) yield full;
  }
}

/** The part of a source file that only exists in test builds. */
function testRegion(file, text) {
  if (file.includes(`${path.sep}tests${path.sep}`)) return { start: 0, body: text };
  const marker = /#\[cfg\(test\)\]\s*\n\s*(?:#\[[^\]]*\]\s*\n\s*)*(?:pub\s+)?mod\s+\w+\s*\{/g;
  let first = null;
  for (const match of text.matchAll(marker)) {
    first = match.index;
    break;
  }
  if (first === null) return null;
  return { start: text.slice(0, first).split("\n").length - 1, body: text.slice(first) };
}

const violations = [];
let scanned = 0;
for (const base of ["src-tauri/tests", "src-tauri/src"]) {
  for (const file of rustFiles(path.join(root, base))) {
    const region = testRegion(file, readFileSync(file, "utf8"));
    if (!region) continue;
    scanned += 1;
    region.body.split("\n").forEach((line, index) => {
      const code = line.replace(/\/\/.*$/, "");
      for (const [pattern, why] of FORBIDDEN) {
        if (pattern.test(code)) {
          violations.push(`${path.relative(root, file)}:${region.start + index + 1}  ${why}\n    ${line.trim()}`);
        }
      }
    });
  }
}

console.log("Test-safety guard: Rust test code must never touch the installed app's data");
if (violations.length > 0) {
  console.error(`FAIL: ${violations.length} unsafe reference(s) found in test code:\n`);
  for (const violation of violations) console.error(`  ${violation}`);
  process.exit(1);
}
console.log(`PASS (${scanned} test files/modules scanned)`);
