"use strict";

const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { test } = require("node:test");

const KIT_ROOT = path.join(__dirname, "..");
const RPC_MAIN = path.join(KIT_ROOT, "src", "main.js");

function makeProject() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "provekit-ts-dep-proofs-"));
  fs.writeFileSync(path.join(root, "package.json"), "{\"private\":true}\n");
  fs.mkdirSync(path.join(root, "node_modules"), { recursive: true });
  return root;
}

function writePackage(root, packageName) {
  const packageRoot = packageName.startsWith("@")
    ? path.join(root, "node_modules", ...packageName.split("/"))
    : path.join(root, "node_modules", packageName);
  fs.mkdirSync(packageRoot, { recursive: true });
  fs.writeFileSync(
    path.join(packageRoot, "package.json"),
    `${JSON.stringify({ name: packageName, version: "1.0.0" })}\n`,
  );
  return packageRoot;
}

function writeProof(packageRoot, hex) {
  const proofPath = path.join(packageRoot, `blake3-512:${hex}.proof`);
  fs.writeFileSync(proofPath, "synthetic dependency proof\n");
  return proofPath;
}

function invokeResolveDependencyProofs(projectRoot) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [RPC_MAIN, "--rpc"], {
      cwd: KIT_ROOT,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`rpc exited with code ${code}: ${stderr}`));
        return;
      }
      const line = stdout.trim().split("\n").filter(Boolean)[0];
      if (!line) {
        reject(new Error(`rpc returned no response; stderr: ${stderr}`));
        return;
      }
      resolve(JSON.parse(line));
    });
    child.stdin.end(`${JSON.stringify({
      jsonrpc: "2.0",
      id: 7,
      method: "provekit.plugin.resolve_dependency_proofs",
      params: { project_root: projectRoot },
    })}\n`);
  });
}

test("resolve_dependency_proofs returns absolute readable proofs from node_modules packages", async () => {
  const projectRoot = makeProject();
  try {
    const depOne = writePackage(projectRoot, "dep-one");
    const depTwo = writePackage(projectRoot, "@scope/dep-two");
    const proofOne = fs.realpathSync(writeProof(depOne, "a".repeat(128)));
    const proofTwo = fs.realpathSync(writeProof(depTwo, "b".repeat(128)));
    fs.symlinkSync(depOne, path.join(projectRoot, "node_modules", "dep-one-alias"), "dir");

    const response = await invokeResolveDependencyProofs(projectRoot);

    assert.equal(response.jsonrpc, "2.0");
    assert.equal(response.id, 7);
    assert.equal(response.error, undefined);
    const proofPaths = response.result.proof_paths.toSorted();
    assert.deepEqual(proofPaths, [proofOne, proofTwo].toSorted());
    for (const proofPath of proofPaths) {
      assert.equal(path.isAbsolute(proofPath), true);
      assert.equal(fs.statSync(proofPath).isFile(), true);
    }
  } finally {
    fs.rmSync(projectRoot, { recursive: true, force: true });
  }
});

test("resolve_dependency_proofs returns an empty array when dependencies have no proofs", async () => {
  const projectRoot = makeProject();
  try {
    writePackage(projectRoot, "dep-without-proof");

    const response = await invokeResolveDependencyProofs(projectRoot);

    assert.equal(response.jsonrpc, "2.0");
    assert.equal(response.id, 7);
    assert.equal(response.error, undefined);
    assert.deepEqual(response.result.proof_paths, []);
  } finally {
    fs.rmSync(projectRoot, { recursive: true, force: true });
  }
});
