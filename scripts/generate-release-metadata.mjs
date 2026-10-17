#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, "..");

function run(command, args) {
  return execFileSync(command, args, {
    cwd: projectRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function fail(message) {
  console.error(`Release metadata generation failed: ${message}`);
  process.exit(1);
}

const packageJson = JSON.parse(
  fs.readFileSync(path.join(projectRoot, "package.json"), "utf8")
);
const tauriConfig = JSON.parse(
  fs.readFileSync(path.join(projectRoot, "src-tauri", "tauri.conf.json"), "utf8")
);
const cargoToml = fs.readFileSync(
  path.join(projectRoot, "src-tauri", "Cargo.toml"),
  "utf8"
);
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

if (!cargoVersion || cargoVersion !== packageJson.version || tauriConfig.version !== packageJson.version) {
  fail("package.json, Cargo.toml, and tauri.conf.json versions must match");
}

const lockfilePath = path.join(projectRoot, "src-tauri", "Cargo.lock");
if (!fs.existsSync(lockfilePath)) {
  fail("src-tauri/Cargo.lock is missing");
}
try {
  run("git", ["ls-files", "--error-unmatch", "src-tauri/Cargo.lock"]);
} catch {
  fail("src-tauri/Cargo.lock must be committed before creating a release");
}

const dirtyState = run("git", ["status", "--porcelain"]);
if (dirtyState) {
  fail("the Git working tree must be clean before creating release metadata");
}

const commit = run("git", ["rev-parse", "HEAD"]);
let gitTag;
try {
  gitTag = run("git", ["describe", "--tags", "--exact-match", "HEAD"]);
} catch {
  fail(`HEAD must have the annotated release tag v${packageJson.version}`);
}
if (gitTag !== `v${packageJson.version}`) {
  fail(`release tag ${gitTag} does not match version v${packageJson.version}`);
}

const bundleDir = path.join(projectRoot, "src-tauri", "target", "release", "bundle", "deb");
const generatedFilename = `${tauriConfig.productName}_${packageJson.version}_amd64.deb`;
const artifactPath = path.join(bundleDir, generatedFilename);
if (!fs.existsSync(artifactPath)) {
  fail(`release artifact not found: ${path.relative(projectRoot, artifactPath)}`);
}

const debVersion = run("dpkg-deb", ["-f", artifactPath, "Version"]);
const architecture = run("dpkg-deb", ["-f", artifactPath, "Architecture"]);
const packageName = run("dpkg-deb", ["-f", artifactPath, "Package"]);
if (debVersion !== packageJson.version) {
  fail(`Debian package version ${debVersion} does not match ${packageJson.version}`);
}

const bytes = fs.readFileSync(artifactPath);
const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
const md5 = crypto.createHash("md5").update(bytes).digest("hex");
const canonicalFilename = `${packageName}_${debVersion}_${architecture}.deb`;
const canonicalPath = path.join(bundleDir, canonicalFilename);

if (canonicalPath !== artifactPath) {
  try {
    const existing = fs.lstatSync(canonicalPath);
    if (!existing.isSymbolicLink()) {
      fail(`refusing to replace non-symlink canonical artifact: ${canonicalFilename}`);
    }
    fs.unlinkSync(canonicalPath);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  fs.symlinkSync(generatedFilename, canonicalPath);
}

const checksumTargets = [generatedFilename, canonicalFilename];
fs.writeFileSync(
  path.join(bundleDir, "SHA256SUMS"),
  `${checksumTargets.map((name) => `${sha256}  ${name}`).join("\n")}\n`
);
fs.writeFileSync(
  path.join(bundleDir, "MD5SUMS"),
  `${checksumTargets.map((name) => `${md5}  ${name}`).join("\n")}\n`
);

const tauriCliPackage = JSON.parse(
  fs.readFileSync(path.join(projectRoot, "node_modules", "@tauri-apps", "cli", "package.json"), "utf8")
);
const repository = run("git", ["config", "--get", "remote.origin.url"]);
const manifest = {
  schemaVersion: "1.0.0",
  releaseStatus: "release",
  productName: tauriConfig.productName,
  packageName,
  version: debVersion,
  architecture,
  targetOs: "Ubuntu Linux",
  supportedReleases: ["Ubuntu 24.04 LTS (Noble Numbat)", "Ubuntu 26.04 LTS"],
  sourceRevision: { commit, gitTag, repository },
  toolchain: {
    rustc: run("rustc", ["--version"]).split(" ")[1],
    cargo: run("cargo", ["--version"]).split(" ")[1],
    node: process.versions.node,
    npm: run("npm", ["--version"]),
    tauriCli: tauriCliPackage.version,
  },
  signing: {
    isSigned: false,
    signatureType: null,
    notes: "Unsigned package. Verify SHA-256 against a checksum obtained from a trusted source.",
  },
  artifacts: [
    {
      filename: generatedFilename,
      canonicalFilename,
      format: "deb",
      sizeBytes: bytes.length,
      sha256,
      md5,
    },
  ],
};

fs.writeFileSync(
  path.join(bundleDir, "release-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`
);

console.log(`Release metadata generated for ${gitTag} (${commit})`);
console.log(`SHA-256: ${sha256}`);
