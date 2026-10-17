#!/usr/bin/env node

/**
 * Frontend Code Quality and Hygiene Linter (Task 23)
 *
 * Enforces code cleanliness, security boundaries, and syntax validity across
 * frontend TypeScript, React, and CSS assets:
 * 1. Executes TypeScript type checker (tsc --noEmit).
 * 2. Checks for forbidden debugger statements.
 * 3. Checks for unresolved git merge conflict markers.
 * 4. Checks for sensitive data leaks (e.g. raw console.log in crypto/security modules).
 * 5. Returns nonzero exit status on violations with actionable diagnostics.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, "..");
const srcDir = path.join(projectRoot, "src");

console.log("🔍 Personal Notepad Frontend Linting & Code Hygiene Gate");
console.log("=======================================================");

let hasErrors = false;

// 1. Run TypeScript type check
process.stdout.write("Checking TypeScript compilation (tsc --noEmit)... ");
try {
  const configPath = ts.findConfigFile(projectRoot, ts.sys.fileExists, "tsconfig.json");
  if (!configPath) throw new Error("tsconfig.json was not found");

  const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
  if (configFile.error) {
    throw new Error(ts.flattenDiagnosticMessageText(configFile.error.messageText, "\n"));
  }

  const parsedConfig = ts.parseJsonConfigFileContent(
    configFile.config,
    ts.sys,
    projectRoot,
    { noEmit: true },
    configPath
  );
  const program = ts.createProgram({
    rootNames: parsedConfig.fileNames,
    options: parsedConfig.options,
    projectReferences: parsedConfig.projectReferences,
  });
  const diagnostics = [...parsedConfig.errors, ...ts.getPreEmitDiagnostics(program)];

  if (diagnostics.length > 0) {
    console.log("FAIL ❌");
    console.error(
      ts.formatDiagnosticsWithColorAndContext(diagnostics, {
        getCanonicalFileName: (fileName) => fileName,
        getCurrentDirectory: () => projectRoot,
        getNewLine: () => "\n",
      })
    );
    hasErrors = true;
  } else {
    console.log("PASS ✅");
  }
} catch (error) {
  console.log("FAIL ❌");
  console.error("\nTypeScript compiler reported errors:");
  console.error(error instanceof Error ? error.message : String(error));
  hasErrors = true;
}

// 2. Scan source files for hygiene rules
process.stdout.write("Scanning source files for hygiene and security rules... ");

function walkDir(dir, fileList = []) {
  const files = fs.readdirSync(dir);
  for (const file of files) {
    const fullPath = path.join(dir, file);
    const stat = fs.statSync(fullPath);
    if (stat.isDirectory()) {
      walkDir(fullPath, fileList);
    } else if (/\.(tsx?|css|mjs|js)$/.test(file)) {
      fileList.push(fullPath);
    }
  }
  return fileList;
}

const sourceFiles = walkDir(srcDir);
const violations = [];

const MERGE_CONFLICT_REGEX = /^(<{7}|={7}|>{7})(\s|$)/m;
const DEBUGGER_REGEX = /\bdebugger\s*;/;

for (const file of sourceFiles) {
  const relPath = path.relative(projectRoot, file);
  const content = fs.readFileSync(file, "utf-8");
  const lines = content.split("\n");

  // A. Check for unresolved merge conflict markers
  if (MERGE_CONFLICT_REGEX.test(content)) {
    violations.push({
      file: relPath,
      rule: "no-merge-conflict-markers",
      message: "Unresolved git merge conflict markers detected.",
    });
  }

  // B. Check for debugger statements
  for (let i = 0; i < lines.length; i++) {
    if (DEBUGGER_REGEX.test(lines[i])) {
      violations.push({
        file: relPath,
        line: i + 1,
        rule: "no-debugger",
        message: "Unexpected 'debugger' statement.",
      });
    }

    // C. Check for raw console logs in sensitive security/crypto modules
    if (
      (relPath.includes("features/security") || relPath.includes("services/crypto")) &&
      /\bconsole\.(log|debug|info)\s*\(/.test(lines[i])
    ) {
      violations.push({
        file: relPath,
        line: i + 1,
        rule: "no-security-console-log",
        message: "Forbidden console log in security/crypto module (leak risk).",
      });
    }
  }
}

if (violations.length === 0) {
  console.log(`PASS ✅ (${sourceFiles.length} files scanned)`);
} else {
  console.log("FAIL ❌");
  console.error(`\nFound ${violations.length} hygiene violation(s):`);
  for (const v of violations) {
    const location = v.line ? `${v.file}:${v.line}` : v.file;
    console.error(`  - [${v.rule}] ${location}: ${v.message}`);
  }
  hasErrors = true;
}

if (hasErrors) {
  console.error("\n❌ Frontend lint checks FAILED.");
  process.exit(1);
} else {
  console.log("\n✅ All frontend lint and hygiene checks PASSED cleanly.");
  process.exit(0);
}
