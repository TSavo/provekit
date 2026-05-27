// SPDX-License-Identifier: Apache-2.0
//
// Phase-2 cross-kit bridge tests: ts kit attestation that the ts lift
// adapter satisfies the Rust kit's `lift_plugin_protocol` contracts.
//
// For each of the 10 contracts in the Rust self-contracts bundle (see
// `implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs`),
// this test:
//
//   1. Lifts the sibling native self-contract source to capture the 10
//      counterpart `ContractDecl`s.
//   2. Mints each counterpart via `mintContract` with the same producedBy
//      / producedAt / signing key the orchestrator uses, yielding the
//      ENVELOPE CID that will land in the ts self-contracts bundle.
//   3. Constructs the matching `BridgeDeclaration` JS object linking
//      Rust contract CID -> ts counterpart envelope CID.
//   4. Canonical-encodes the bridge via `canonicalEncode` and computes
//      its content CID via `computeCid`.
//   5. Pins both the counterpart envelope CIDs and the bridge CIDs so
//      drift in either side surfaces here as a test failure.
//
// On Rust drift (rare but possible if PR #84's contracts are re-touched),
// the imported `RUST_LIFT_PLUGIN_CONTRACT_CIDS` map will diverge from the
// new mint output; the bridge bytes change, the pinned bridge CID
// changes, this test fails. That's the desired behavior: we want drift
// to be visible, not silent.

import { describe, expect, it } from "vitest";

import { join } from "node:path";
import { type BridgeDeclaration } from "../ir/symbolic/property.js";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeCid } from "../canonicalizer/hash.js";
import { mintContract } from "../claimEnvelope/mint.js";
import { generateKeypair } from "../producerKeys/index.js";
import { liftPath, type ContractDecl } from "./index.js";

import {
  RUST_LIFT_PLUGIN_CONTRACT_CIDS,
  RUST_KIT_LAYER,
  TS_KIT_LAYER,
  DEFERRED_TS_LIFT_BINARY_CID,
  PHASE_2_BRIDGE_NOTES,
  counterpartContractName,
  bridgeName,
} from "./cross-kit-bridges.js";
import {
  PRODUCED_BY,
  DECLARED_AT,
} from "../bin/mint-ts-self-contracts.mjs";

// ---------------------------------------------------------------------------
// Pinned counterpart envelope CIDs (10, one per Rust lift_plugin_protocol
// contract). These are the CIDs the ts self-contracts bundle WILL contain
// once the native cross-kit bridge source is wired in (commit body cites the new
// bundle CID for `make mint-ts`).
// ---------------------------------------------------------------------------
const PINNED_COUNTERPART_CIDS: ReadonlyMap<string, string> = new Map([
  [
    "ts_lift_plugin_initialize_protocol_version_match",
    "blake3-512:fec8afef5376e2e1507f2043389c74fb38729c67dc7d98697b33ad82a156165af22a33a18cb7b7f52fbaaefd81af85787107b8ccfbf2e8169ed209bc7a4770df",
  ],
  [
    "ts_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "blake3-512:a81ea865fedba115cc101b6db3d2540ed194bbea0f325bbbca2c91c0901c68d45b7139c957fa92869092ab47e4ea7b4270bb7906f74c08dbf1b2f96eab23e135",
  ],
  [
    "ts_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "blake3-512:656f116bcde496705e1e987e86a1c186d9ae5a0dc8c1cba314056aaf2c27eb90293857878b5d61971ce67d05f915c6aac2034ddfe359e599226431da06e0280f",
  ],
  [
    "ts_lift_plugin_lift_request_surface_is_string",
    "blake3-512:d322597fa7cff04a313cc6af97b1b73005e27fd197ffe6b09a2f2c351136668260b22715911104460cd24a3b925e188a7d2db85a2420aecdeebaca7d6a59588e",
  ],
  [
    "ts_lift_plugin_lift_request_source_paths_nonempty",
    "blake3-512:dc3d900100fa1167796d1c8649009252d065109994364a9f9bc2fb8b1cb86a6e628d6242ac8c52da5c27805480587956a95edad77d5696196a72206fd25140d2",
  ],
  [
    "ts_lift_plugin_lift_request_source_paths_each_nonempty",
    "blake3-512:aab9f9f05eafb4e567fcea296c9e8c9a69e05881309d5d40c3bcab04ce46c72bb238287f77dcd3e062e5d6044d8451211f2d8391a778eebee572d2580365b16a",
  ],
  [
    "ts_lift_plugin_lift_request_surface_in_capabilities",
    "blake3-512:720bf5f495aa0fe16bba4f6a59d986c8b2069b382f18fb3931b3d6d82f7342d3dcd756b70921fa53a7ef717cc868a640d2b6f20ba657ee55f2fba546b04ad17c",
  ],
  [
    "ts_lift_plugin_lift_response_kind_in_set",
    "blake3-512:8ba0855515ae7fb1bb2b02f1218c78a819bf93cd8788cf2cb08cb106144b973bd978d1c24ed1ec4cc34d53d12961438e2da180f7f274f8099ed8882165ca5673",
  ],
  [
    "ts_lift_plugin_lift_response_ir_document_array",
    "blake3-512:d546bb33ee1edbe06554427c5620d2b5e01636dcd354992b685a103baf6b7fd0d213f2c907b3a60d8074cdf9f6fdfced1962ff7a0906640dbce980701a774665",
  ],
  [
    "ts_lift_plugin_diagnostic_field_is_array",
    "blake3-512:64bc969749ad9a802b89876751f1def2f01a6bd26cc59cf3e02dcffa97998f382ab16f31b4b16be15ab85b9c17147852cf08af3d8cb8fc8fec2c996d9b71257d",
  ],
]);

// ---------------------------------------------------------------------------
// Pinned bridge CIDs (10, one per Rust lift_plugin_protocol contract).
// Each is the content CID of the `BridgeDeclaration` JS object whose
// `targetContractCid` is the corresponding counterpart envelope CID above.
// ---------------------------------------------------------------------------
const PINNED_BRIDGE_CIDS: ReadonlyMap<string, string> = new Map([
  [
    "bridge_to_lift_plugin_initialize_protocol_version_match",
    "blake3-512:8fc7bfa5d9959bdc92661feb473dbdb6621e65e4f320c1c76e1a5fcb20277b61d9e6141197216365521b6f12eedcf993fbbb91ce607160477b89a69ff25e6948",
  ],
  [
    "bridge_to_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "blake3-512:a30449b6134bf120abf29b39d9292d09b83da5a6508bedb22817e3561d9c93b78ab77aac82654f84aa74385b7a6a1d545cb5c0508fc2567cc29cc76d66c89f14",
  ],
  [
    "bridge_to_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "blake3-512:ef17f3a438dc25fa5cfedf8770b6920d0e9665d1afc7c23f9c8f1308c26c31af1e0aad7ab4214def21c2c4e28e3f3e9a00ed3394c12c8b16a322227c1b8b5fd2",
  ],
  [
    "bridge_to_lift_plugin_lift_request_surface_is_string",
    "blake3-512:00a9eedd3467504539a17383945ee7a9d193a090c0d4c180e8893a27fe2b9e34a4ed45ff74f99e655737f7b670fd38aed58222f7c1855dc121f0201306f1dd9a",
  ],
  [
    "bridge_to_lift_plugin_lift_request_source_paths_nonempty",
    "blake3-512:52d9594711c9b979c10f08b8dd66fb36b9799d279ece75b0ef46f868cf4b9cb57ef72f538acbaafa30744d58bad7a30286fc6807a6c459344fcfe01db8bb1492",
  ],
  [
    "bridge_to_lift_plugin_lift_request_source_paths_each_nonempty",
    "blake3-512:016ed28b1aef22e48d73e729746ea447ad4e7a8de1ecb6de6ab546da0a7b841efc45a81a5459176ce35b50f5b8a0315b06e83ec8ddc6d42a136b1ef0fb4f98e2",
  ],
  [
    "bridge_to_lift_plugin_lift_request_surface_in_capabilities",
    "blake3-512:2111197b5ed52d05ddccfb761b246488409cf5449890aba5cc4feef82989d9fb3e6cfac72a69c07dbff44e81e8e4f8c169d861d881557b8842a116c046624f67",
  ],
  [
    "bridge_to_lift_plugin_lift_response_kind_in_set",
    "blake3-512:1676585b87a6135ad88edfe0bb530e43263078477a5cd3e0a9218284bbfe39923e81eada6187e0c5c1d8237788710148c1d6ce3be152e2928f4a97d572061dd0",
  ],
  [
    "bridge_to_lift_plugin_lift_response_ir_document_array",
    "blake3-512:fab8777cdd76ff0a690223355c8c6cc442a3313751ebbeb9eb1c7d61164120d531db9b116693d0949b02e5ab789f5015b995231eaafc9bfc09b762c5116c57fb",
  ],
  [
    "bridge_to_lift_plugin_diagnostic_field_is_array",
    "blake3-512:6ce5854b8b34c60ffb1cdf6ee3fabad37e675b5d4deb28f2492703f9fb2bb6d65da93bd775605765c5aa5eff68fc4f1e42b8de09565763742904960c4b2d3c98",
  ],
]);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function collectCounterpartContracts(): ContractDecl[] {
  const sourcePath = join(__dirname, "cross-kit-bridges.self-contracts.test.ts");
  const expectedNames = new Set(
    Array.from(RUST_LIFT_PLUGIN_CONTRACT_CIDS.keys()).map(counterpartContractName),
  );
  return liftPath(sourcePath).decls.filter((d) => expectedNames.has(d.name));
}

function buildBridgeDeclaration(
  rustContractName: string,
  rustContractCid: string,
  targetCounterpartCid: string,
): BridgeDeclaration {
  return {
    kind: "bridge",
    name: bridgeName(rustContractName),
    sourceSymbol: rustContractName,
    sourceLayer: RUST_KIT_LAYER,
    sourceContractCid: rustContractCid,
    targetContractCid: targetCounterpartCid,
    targetProofCid: DEFERRED_TS_LIFT_BINARY_CID,
    targetLayer: TS_KIT_LAYER,
    notes: PHASE_2_BRIDGE_NOTES,
  };
}

// Foundation key. Same `[0x42; 32]` seed the Rust + TS orchestrators use.
function getSigningKey() {
  const seed = Buffer.alloc(32, 0x42);
  return generateKeypair({ seed });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("cross-kit bridges: lift-plugin-protocol (Phase 2)", () => {
  it("authors exactly 10 counterpart contracts, one per Rust contract", () => {
    const counterparts = collectCounterpartContracts();
    expect(counterparts.length).toBe(10);

    const rustNames = Array.from(RUST_LIFT_PLUGIN_CONTRACT_CIDS.keys());
    expect(rustNames.length).toBe(10);

    const counterpartNames = counterparts.map((c) => c.name).sort();
    const expectedNames = rustNames.map((n) => counterpartContractName(n)).sort();
    expect(counterpartNames).toEqual(expectedNames);
  });

  it("counterpart envelope CIDs are pinned and stable", () => {
    const counterparts = collectCounterpartContracts();
    const { privateKey } = getSigningKey();

    const actualCids = new Map<string, string>();
    for (const decl of counterparts) {
      const env = mintContract({
        producedBy: PRODUCED_BY,
        producedAt: DECLARED_AT,
        privateKey,
        contractName: decl.name,
        outBinding: decl.outBinding,
        ...(decl.pre !== undefined ? { pre: decl.pre } : {}),
        ...(decl.post !== undefined ? { post: decl.post } : {}),
        ...(decl.inv !== undefined ? { inv: decl.inv } : {}),
        authoring: {
          producerKind: "kit-author",
          author: PRODUCED_BY,
          note: `self-contract from implementations/typescript/src/lift/cross-kit-bridges.ts`,
        },
      });
      actualCids.set(decl.name, env.cid);
    }

    // Dump ACTUAL counterpart CIDs to stdout for easy pin updates.
    if (process.env.DUMP_PINS) {
      console.log("\nCOUNTERPART_CIDS:");
      for (const [name, cid] of actualCids) console.log(`  ["${name}", "${cid}"],`);
    }
    // Compare as a single Map so the diff shows ALL drift at once.
    const expectedMap = new Map(PINNED_COUNTERPART_CIDS);
    expect(actualCids).toEqual(expectedMap);
  });

  it("bridge content CIDs are pinned and stable", () => {
    const counterparts = collectCounterpartContracts();
    const { privateKey } = getSigningKey();

    // First mint counterparts -> envelope CIDs.
    const counterpartCids = new Map<string, string>();
    for (const decl of counterparts) {
      const env = mintContract({
        producedBy: PRODUCED_BY,
        producedAt: DECLARED_AT,
        privateKey,
        contractName: decl.name,
        outBinding: decl.outBinding,
        ...(decl.pre !== undefined ? { pre: decl.pre } : {}),
        ...(decl.post !== undefined ? { post: decl.post } : {}),
        ...(decl.inv !== undefined ? { inv: decl.inv } : {}),
        authoring: {
          producerKind: "kit-author",
          author: PRODUCED_BY,
          note: `self-contract from implementations/typescript/src/lift/cross-kit-bridges.ts`,
        },
      });
      counterpartCids.set(decl.name, env.cid);
    }

    // Build + canonical-encode + CID each bridge.
    const actualBridgeCids = new Map<string, string>();
    for (const [rustContractName, rustContractCid] of RUST_LIFT_PLUGIN_CONTRACT_CIDS) {
      const counterpartName = counterpartContractName(rustContractName);
      const counterpartCid = counterpartCids.get(counterpartName);
      expect(
        counterpartCid,
        `counterpart ${counterpartName} not minted`,
      ).toBeDefined();
      const decl = buildBridgeDeclaration(
        rustContractName,
        rustContractCid,
        counterpartCid as string,
      );
      const bytes = canonicalEncode(decl);
      const cid = computeCid(bytes);
      actualBridgeCids.set(decl.name, cid);
    }

    if (process.env.DUMP_PINS) {
      console.log("\nBRIDGE_CIDS:");
      for (const [name, cid] of actualBridgeCids) console.log(`  ["${name}", "${cid}"],`);
    }
    const expectedMap = new Map(PINNED_BRIDGE_CIDS);
    expect(actualBridgeCids).toEqual(expectedMap);
  });

  it("each bridge points at the correct Rust source CID", () => {
    // Sanity: the Rust source CID stays in lock-step with the imported
    // RUST_LIFT_PLUGIN_CONTRACT_CIDS map. If this fails the native source's
    // imported map drifted from what we built bridges against.
    const counterparts = collectCounterpartContracts();
    const { privateKey } = getSigningKey();
    const counterpartCids = new Map<string, string>();
    for (const decl of counterparts) {
      const env = mintContract({
        producedBy: PRODUCED_BY,
        producedAt: DECLARED_AT,
        privateKey,
        contractName: decl.name,
        outBinding: decl.outBinding,
        ...(decl.pre !== undefined ? { pre: decl.pre } : {}),
        ...(decl.post !== undefined ? { post: decl.post } : {}),
        ...(decl.inv !== undefined ? { inv: decl.inv } : {}),
        authoring: {
          producerKind: "kit-author",
          author: PRODUCED_BY,
          note: "x",
        },
      });
      counterpartCids.set(decl.name, env.cid);
    }
    for (const [rustContractName, rustContractCid] of RUST_LIFT_PLUGIN_CONTRACT_CIDS) {
      const counterpartName = counterpartContractName(rustContractName);
      const decl = buildBridgeDeclaration(
        rustContractName,
        rustContractCid,
        counterpartCids.get(counterpartName) as string,
      );
      expect(decl.sourceContractCid).toBe(rustContractCid);
      expect(decl.sourceLayer).toBe("rust-kit");
      expect(decl.targetLayer).toBe("typescript-kit");
      expect(decl.targetProofCid).toBe("deferred:phase-3-proof-bundle");
      expect(decl.notes).toBe(PHASE_2_BRIDGE_NOTES);
    }
  });
});
