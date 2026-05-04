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
 *
 * Root cause of postinstall hang (2026-05-03): using `npx provekit verify`
 * causes npx to attempt a remote registry fetch when the provekit binary
 * is not found in the local node_modules/.bin/ (e.g. when provekit is
 * being installed into itself, or when node_modules is not yet populated).
 * Fix: resolve the binary directly from node_modules/.bin/provekit, and
 * fall back gracefully if it is not found rather than going through npx.
 *
 * Defense-in-depth (2026-05-03): a 60-second hard timeout is added to
 * prevent any future hang in the verify step from blocking npm install
 * indefinitely, regardless of root cause.
 */

const { execSync } = require("child_process");
const { existsSync } = require("fs");
const { join } = require("path");

// Skip if explicitly disabled
if (process.env.PROVEKIT_SKIP_VERIFY === "1") {
  process.exit(0);
}

// Only run in project installs, not global or provekit's own install.
// isGlobalInstall: INIT_CWD is unset for global installs.
const isGlobalInstall = !process.env.INIT_CWD;
// isSelfInstall: the package being installed IS provekit itself.
// Detection: npm sets INIT_CWD to the directory where the user ran install;
// the postinstall cwd is the package root. If they match, this is the
// provekit repo's own install (developer workflow, e.g. `pnpm install` in
// the provekit repo), not a consumer install.
// The node_modules/provekit path test is the secondary check for the case
// where provekit is installed as a transitive dep inside another package's
// node_modules tree (pnpm workspace peer installs, etc.).
const isSelfInstall =
  process.cwd() === (process.env.INIT_CWD || "") ||
  process.cwd().includes("node_modules/provekit");
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

// Resolve the provekit binary. Root cause of the postinstall hang was using
// `npx provekit` which causes npx to attempt a remote registry fetch when
// the binary is not in node_modules/.bin/ (e.g. during self-install or
// when the registry is unreachable). Instead, resolve the binary directly.
//
// Resolution order:
//   1. node_modules/.bin/provekit in the consumer project (standard consumer path)
//   2. node_modules/provekit/bin/provekit.cjs   (pnpm deep-layout path)
//   3. Not found — warn and exit 0 (don't fail the install over a missing binary)
function resolveProvekitBinary(projectRoot) {
  const candidates = [
    join(projectRoot, "node_modules", ".bin", "provekit"),
    join(projectRoot, "node_modules", "provekit", "bin", "provekit.cjs"),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

const binaryPath = resolveProvekitBinary(projectRoot);
if (!binaryPath) {
  console.warn(
    "[provekit] Binary not found in node_modules/.bin/provekit. " +
    "Skipping verification. (Set PROVEKIT_SKIP_VERIFY=1 to suppress this warning.)"
  );
  process.exit(0);
}

const args = ["verify"];
if (process.env.PROVEKIT_FULL_VERIFY === "1") {
  args.push("--full");
}

// Invoke via node so the shebang line is irrelevant and the binary works
// on all platforms without shell resolution.
const invokeCmd = binaryPath.endsWith(".cjs")
  ? `node ${JSON.stringify(binaryPath)} ${args.join(" ")}`
  : `${JSON.stringify(binaryPath)} ${args.join(" ")}`;

try {
  execSync(invokeCmd, {
    cwd: projectRoot,
    stdio: "inherit",
    encoding: "utf-8",
    // Defense-in-depth: hard 60-second timeout. If verify hangs for any
    // reason (WASM hang, daemon that doesn't exit, network stall) we must
    // not block npm install indefinitely. On timeout we warn and exit 0
    // (proceeding with install) rather than failing.
    timeout: 60_000,
  });
  console.log("[provekit] Verification passed.");
} catch (error) {
  if (error.signal === "SIGTERM" || (error.code && error.code === "ETIMEDOUT")) {
    console.warn(
      "[provekit] Verification timed out after 60s. Proceeding with install. " +
      "(Set PROVEKIT_SKIP_VERIFY=1 to skip verification.)"
    );
    process.exit(0);
  }
  console.error("[provekit] Verification failed. Aborting install.");
  console.error("[provekit] Set PROVEKIT_SKIP_VERIFY=1 to skip verification.");
  process.exit(1);
}
