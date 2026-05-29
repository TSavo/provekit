"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { test } = require("node:test");

const { dispatch } = require("../src/rpc");

function makeProject() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "provekit-ts-pg-dep-proofs-"));
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
  fs.writeFileSync(proofPath, "synthetic pg dependency proof\n");
  return proofPath;
}

test("resolve_dependency_proofs returns node_modules proof bytes", () => {
  const projectRoot = makeProject();
  try {
    const proofPath = writeProof(writePackage(projectRoot, "pg-dep"), "c".repeat(128));

    const response = dispatch({
      jsonrpc: "2.0",
      id: 7,
      method: "provekit.plugin.resolve_dependency_proofs",
      params: { project_root: projectRoot },
    });

    assert.equal(response.jsonrpc, "2.0");
    assert.equal(response.id, 7);
    assert.equal(response.error, undefined);
    const proof = response.result.proofs[0];
    assert.equal(proof.cid, path.basename(proofPath, ".proof"));
    assert.equal(Buffer.from(proof.bytes_base64, "base64").toString("utf8"), "synthetic pg dependency proof\n");
    assert.equal(proof.source, `typescript-package:${path.basename(proofPath)}`);
  } finally {
    fs.rmSync(projectRoot, { recursive: true, force: true });
  }
});
