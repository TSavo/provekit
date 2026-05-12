#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
//
// mint-ts-self-contracts-rpc: RPC entry point for the typescript-self-contracts
// lift surface.
//
// Speaks the lift-plugin protocol (pep/1.7.0) over NDJSON on stdio.
// The Rust CLI spawns this with `--rpc` and exchanges:
//   -> initialize
//   <- {name, version, capabilities}
//   -> lift
//   <- {kind:"proof-envelope", filename_cid, contract_set_cid, bytes_base64}
//   -> shutdown
//   <- null
//
// Invocation (from implementations/typescript/):
//   node --experimental-require-module src/bin/mint-ts-self-contracts-rpc.cjs
//
// Why --experimental-require-module:
//   @ipld/dag-cbor (used by proofEnvelope) is ESM-only. Node 22+ allows
//   require() of ESM modules under this flag. tsx/cjs then handles the
//   TypeScript -> CJS transpilation for all other source files.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md

"use strict";

// Load tsx CJS transform FIRST so subsequent require() calls on .ts/.mts
// files are transpiled inline. Must precede all project imports.
require("tsx/cjs");

const { runMintSelfContracts } = require("./mint-ts-self-contracts.mts");
const { mkdtempSync, rmSync, readFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const readline = require("node:readline");

// The Rust dispatcher appends --rpc to the command; we accept it and ignore
// it (this file IS the RPC entry point, so --rpc is implied).

function writeRPC(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function handleLine(line) {
  line = line.trim();
  if (!line) return;

  let req;
  try {
    req = JSON.parse(line);
  } catch (e) {
    writeRPC({
      jsonrpc: "2.0",
      id: null,
      error: { code: -32700, message: "Parse error: " + String(e) },
    });
    return;
  }

  const { id, method } = req;

  if (method === "initialize") {
    writeRPC({
      jsonrpc: "2.0",
      id,
      result: {
        name: "typescript-self-contracts",
        version: "1.0.0",
        protocol_version: "pep/1.7.0",
        capabilities: {
          authoring_surfaces: ["typescript-self-contracts"],
          ir_version: "v1.1.0",
          emits_signed_mementos: true,
        },
      },
    });
    return;
  }

  if (method === "lift") {
    const tmpDir = mkdtempSync(join(tmpdir(), "provekit-ts-rpc-"));
    try {
      const result = runMintSelfContracts(tmpDir);
      const proofBytes = readFileSync(result.path);
      const b64 = proofBytes.toString("base64");
      writeRPC({
        jsonrpc: "2.0",
        id,
        result: {
          kind: "proof-envelope",
          filename_cid: result.cid,
          contract_set_cid: result.contractSetCid,
          bytes_base64: b64,
          diagnostics: [],
        },
      });
    } catch (e) {
      writeRPC({
        jsonrpc: "2.0",
        id,
        error: { code: 1005, message: "LIFT_FAILED: " + String(e) },
      });
    } finally {
      try {
        rmSync(tmpDir, { recursive: true, force: true });
      } catch (_) {}
    }
    return;
  }

  if (method === "shutdown") {
    writeRPC({ jsonrpc: "2.0", id, result: null });
    rl.close();
    process.exit(0);
    return;
  }

  writeRPC({
    jsonrpc: "2.0",
    id,
    error: { code: -32601, message: "METHOD_NOT_FOUND: " + method },
  });
}

const rl = readline.createInterface({
  input: process.stdin,
  output: undefined,
  terminal: false,
});

rl.on("line", handleLine);

// Stdin EOF = graceful shutdown (architect rule #3 in #176).
rl.on("close", () => {
  process.exit(0);
});
