"use strict";

const fs = require("node:fs");
const path = require("node:path");

const PROOF_FILE_RE = /^blake3-512:[0-9a-fA-F]+\.proof$/;

function resolveDependencyProofPaths(projectRoot) {
  const root = path.resolve(
    typeof projectRoot === "string" && projectRoot !== "" ? projectRoot : process.cwd(),
  );
  const proofPaths = new Set();
  const visitedNodeModules = new Set();
  const visitedPackages = new Set();

  walkNodeModules(path.join(root, "node_modules"), {
    proofPaths,
    visitedNodeModules,
    visitedPackages,
  });

  return Array.from(proofPaths).sort();
}

function walkNodeModules(nodeModulesDir, state) {
  const realNodeModules = realDirectoryPath(nodeModulesDir);
  if (realNodeModules === null || state.visitedNodeModules.has(realNodeModules)) return;
  state.visitedNodeModules.add(realNodeModules);

  let entries;
  try {
    entries = fs.readdirSync(realNodeModules, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue;

    const entryPath = path.join(realNodeModules, entry.name);
    if (entry.name.startsWith("@")) {
      walkScopedPackages(entryPath, state);
      continue;
    }

    collectPackage(entryPath, state);
  }
}

function walkScopedPackages(scopeDir, state) {
  const realScopeDir = realDirectoryPath(scopeDir);
  if (realScopeDir === null) return;

  let entries;
  try {
    entries = fs.readdirSync(realScopeDir, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    collectPackage(path.join(realScopeDir, entry.name), state);
  }
}

function collectPackage(packageDir, state) {
  const realPackageDir = realDirectoryPath(packageDir);
  if (realPackageDir === null || state.visitedPackages.has(realPackageDir)) return;
  state.visitedPackages.add(realPackageDir);

  collectPackageProofFiles(realPackageDir, state.proofPaths);
  walkNodeModules(path.join(realPackageDir, "node_modules"), state);
}

function collectPackageProofFiles(packageDir, proofPaths) {
  let entries;
  try {
    entries = fs.readdirSync(packageDir, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    const entryPath = path.join(packageDir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === ".git") continue;
      collectPackageProofFiles(entryPath, proofPaths);
      continue;
    }
    if (entry.isFile() && PROOF_FILE_RE.test(entry.name)) {
      proofPaths.add(entryPath);
    }
  }
}

function realDirectoryPath(candidate) {
  try {
    const realPath = fs.realpathSync(candidate);
    return fs.statSync(realPath).isDirectory() ? realPath : null;
  } catch {
    return null;
  }
}

module.exports = {
  resolveDependencyProofPaths,
};
