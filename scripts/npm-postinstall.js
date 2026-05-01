#!/usr/bin/env node
/**
 * npm postinstall hook for provekit.
 *
 * When provekit is installed as a dependency (npm install provekit),
 * this script runs automatically and verifies the project's proof graph.
 *
 * Architecture: provekit is a protocol, not a codebase. This hook
 * integrates the protocol into npm's native toolchain. After any
 * package installation, the proof graph is verified. Breaking changes
 * are caught at install time, not runtime.
 *
 * Behavior:
 *   - If no .proof files exist in the project: silent no-op
 *   - If .proof files exist: runs `provekit verify`
 *   - If verification fails: exits non-zero, aborting the install
 *   - Can be disabled: PROVEKIT_SKIP_VERIFY=1 npm install
 *
 * The default mode is trust (fast hash lookup). To force solver
 * re-verification, set PROVEKIT_FULL_VERIFY=1.
 */

const { execSync } = require("child_process");
const { existsSync } = require("fs");
const { join } = require("path");

// Skip if explicitly disabled
if (process.env.PROVEKIT_SKIP_VERIFY === "1") {
  process.exit(0);
}

// Only run in project installs, not global or provekit's own install
const isGlobalInstall = !process.env.INIT_CWD;
const isSelfInstall = process.cwd().includes("node_modules/provekit");
if (isGlobalInstall || isSelfInstall) {
  process.exit(0);
}

const projectRoot = process.env.INIT_CWD || process.cwd();

// Check if the project has any .proof files
function hasProofFiles(dir) {
  if (!existsSync(dir)) return false;
  const entries = require("fs").readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isFile() && entry.name.endsWith(".proof")) return true;
    if (entry.isDirectory() && entry.name !== "node_modules") {
      // Quick check: does this dir have .proof files?
      const subDir = join(dir, entry.name);
      try {
        const subEntries = require("fs").readdirSync(subDir);
        if (subEntries.some((f) => f.endsWith(".proof"))) return true;
      } catch {
        // ignore
      }
    }
  }
  return false;
}

// Check if any dependency has .proof files
function hasDependencyProofs(projectRoot) {
  const nodeModules = join(projectRoot, "node_modules");
  if (!existsSync(nodeModules)) return false;

  const packages = require("fs").readdirSync(nodeModules, { withFileTypes: true });
  for (const pkg of packages) {
    if (!pkg.isDirectory()) continue;
    const pkgPath = join(nodeModules, pkg.name);
    try {
      const files = require("fs").readdirSync(pkgPath);
      if (files.some((f) => f.endsWith(".proof"))) return true;
    } catch {
      // ignore
    }
  }
  return false;
}

const shouldVerify = hasProofFiles(projectRoot) || hasDependencyProofs(projectRoot);
if (!shouldVerify) {
  process.exit(0);
}

console.log("[provekit] Verifying proof graph...");

try {
  const args = ["verify"];
  if (process.env.PROVEKIT_FULL_VERIFY === "1") {
    args.push("--full");
  }

  execSync(`npx provekit ${args.join(" ")}`, {
    cwd: projectRoot,
    stdio: "inherit",
    encoding: "utf-8",
  });
  console.log("[provekit] Verification passed.");
} catch (error) {
  console.error("[provekit] Verification failed. Aborting install.");
  console.error("[provekit] Set PROVEKIT_SKIP_VERIFY=1 to skip verification.");
  process.exit(1);
}
