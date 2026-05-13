// SPDX-License-Identifier: Apache-2.0
//
// Phase-2 cross-kit bridge tests: ts kit attestation that the ts lift
// adapter satisfies the Rust kit's `lift_plugin_protocol` contracts.
//
// For each of the 10 contracts in the Rust self-contracts bundle (see
// `implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs`),
// this test:
//
//   1. Re-runs the sibling `cross-kit-bridges.invariant.ts` slab to
//      capture the 10 counterpart `ContractDeclaration`s.
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

import {
  beginCollecting,
  _resetCollector,
  type ContractDeclaration,
  type BridgeDeclaration,
  type Declaration,
} from "../ir/symbolic/property.js";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeCid } from "../canonicalizer/hash.js";
import { mintContract } from "../claimEnvelope/mint.js";
import { generateKeypair } from "../producerKeys/index.js";

import {
  invariants as crossKitBridgesInvariants,
  RUST_LIFT_PLUGIN_CONTRACT_CIDS,
  RUST_KIT_LAYER,
  TS_KIT_LAYER,
  DEFERRED_TS_LIFT_BINARY_CID,
  PHASE_2_BRIDGE_NOTES,
  counterpartContractName,
  bridgeName,
} from "./cross-kit-bridges.invariant.js";
import {
  PRODUCED_BY,
  DECLARED_AT,
} from "../bin/mint-ts-self-contracts.mjs";

// ---------------------------------------------------------------------------
// Pinned counterpart envelope CIDs (10, one per Rust lift_plugin_protocol
// contract). These are the CIDs the ts self-contracts bundle WILL contain
// once the cross-kit-bridges slab is wired in (commit body cites the new
// bundle CID for `make mint-ts`).
// ---------------------------------------------------------------------------
const PINNED_COUNTERPART_CIDS: ReadonlyMap<string, string> = new Map([
  [
    "ts_lift_plugin_initialize_protocol_version_match",
    "blake3-512:4dc1d7f681f936bc56a04a364a09319adae3d0c143c391395d2d28ca8cd3533d890d4f1a1c0551b5e3748b6245a9fd5791dcdc193fcc7109ebd9efc7d3a3e9c5",
  ],
  [
    "ts_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "blake3-512:67389b3a5300286d9fdf9dfc674f51352ff071b6c0af1443edd45cd6b4e444d89ce9b6a207e21803eb0fb945b14080aaf35321e2f47a0a52782b110fa37cc111",
  ],
  [
    "ts_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "blake3-512:3e44dd76904de2d152c99e5b6975b4f43dd6deee6a5de0ae083dc0f427ee3be9d287f6030aef578ba044291765bc8dee25871df2bf5d1708ab0dca95467f050d",
  ],
  [
    "ts_lift_plugin_lift_request_surface_is_string",
    "blake3-512:19998ebf887b19a24bb3c14e4e9aa36c10ef2b0c4f02b16b0ca42ccd3a58baf118ea35f4ea1749a19a5ece58a0818008e68acc97a50282f44b5ef362af5e4c78",
  ],
  [
    "ts_lift_plugin_lift_request_source_paths_nonempty",
    "blake3-512:2d13d33c5a2d748e71eab103357f79f4afe5648eaef6ce083b96ca9f9ba1819b0bbc79edee27ed57e277f7e2cdf4129030990e54c668e5639e8ea48f9024a1e0",
  ],
  [
    "ts_lift_plugin_lift_request_source_paths_each_nonempty",
    "blake3-512:e2c45ccc34809a042a23473b1dd21df931423833295c63c0e153a6ac0dab15f4d7e6d697b623c14c84edfdb9fac7cf67c6e8ca85764eee7834cd52e45867d689",
  ],
  [
    "ts_lift_plugin_lift_request_surface_in_capabilities",
    "blake3-512:8e8a355fefd51e7d658e6aeb3c26101c1ccb4c901a682ed8cd53a224fee77e7394d6c80e05effcdd8408e1e51187830b1c1d08322a77b7831676b480477d553d",
  ],
  [
    "ts_lift_plugin_lift_response_kind_in_set",
    "blake3-512:8cec4d85e981ba1a0af34ba1cc633a756c64682010723c9d773c512c8c8b0e948ff03e47aaf0ee08aa16962af5e0454be3df311aa0ab988779b7d50e4f528ff7",
  ],
  [
    "ts_lift_plugin_lift_response_ir_document_array",
    "blake3-512:f1b9528a392a4c9f2358214bb3600f7c5665ddcab007888f53db69145edb2d1897f4c95dcd90c01eb8b2aa4dfb1b9e16e7edf573911263675576307ac62b1b00",
  ],
  [
    "ts_lift_plugin_diagnostic_field_is_array",
    "blake3-512:dec16ad4a6ce72026364d6f7453c5b090486d5bb6c65ee5698657b49bf6caf9f743e63a5f8a58dc7bf7fc09d22f43c54735e25fbe39a4714624bb6d7ba23c283",
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
    "blake3-512:f1b22667687c179adc1be8731f2096a5b197023be1081fb3bb3ca9528c867dd81374c0a04006c81b60e1ef378ebfb4294483fbd024f17ae9b9e1fc72c166745d",
  ],
  [
    "bridge_to_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "blake3-512:65ba953c63b98a83a1fb670a35e5b02f3fa57421898fde2ab63912994850564ebd1f605d5515cf508843a2d19217de0059c0dbccc726d3eda0a5f56f951bf83f",
  ],
  [
    "bridge_to_lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "blake3-512:4facd5bf5f854511287b07074863c074b7615d0090ff175aa4327e2029ada01c6890efa86065e33eada15df7b53bab2fcd6e7b2ac006e2f35ed596f1f87a3e3a",
  ],
  [
    "bridge_to_lift_plugin_lift_request_surface_is_string",
    "blake3-512:150385a77549be3cfca758670450c678768be44dd39813e9712d9703ec4753cd2bcda4dc32c5acbf1fca2aece9954a77e5e2812ebc2807a7a839b55e7a8c2c08",
  ],
  [
    "bridge_to_lift_plugin_lift_request_source_paths_nonempty",
    "blake3-512:72f29af7b43a26e035ae4655582164e02c346c8f5dd8c1e0c628b0444c838d26f00f38f02567571eef742b924b5814a5a0d3d12c9df9d9e893e1d5aab8175d86",
  ],
  [
    "bridge_to_lift_plugin_lift_request_source_paths_each_nonempty",
    "blake3-512:e744a961423ae416283b010c6e5b502327bdf5353ecbbef74919745fba924db54d7df6b8c11b84aa26692386912976f590ec24ac9d46d4737b60e851c50de7ba",
  ],
  [
    "bridge_to_lift_plugin_lift_request_surface_in_capabilities",
    "blake3-512:422f55e5850c948ba48084a3345e66dd0226abb1745b83c1225106c08b91657dc6517a29a881f8dab53f80dabd4008b94eaf5c3cf67ab82539beab3a15c8fa1a",
  ],
  [
    "bridge_to_lift_plugin_lift_response_kind_in_set",
    "blake3-512:987cc215b870be26ab3e3518f6010751955edb8947b0fabd4bd5a8891d509efa1200210f636d28cf9e73b13ab1a9fcc8c66e1d32c537eb31b9198f2ecbaa64a0",
  ],
  [
    "bridge_to_lift_plugin_lift_response_ir_document_array",
    "blake3-512:2a61099df72b0727f4923cf37d474fc4e1f222d6ac6f4eb4094b18bf16d681b4c658e9ccb513c5490076b63c6408f11c288657ba03e147c73bba4b8337fdb029",
  ],
  [
    "bridge_to_lift_plugin_diagnostic_field_is_array",
    "blake3-512:3bae049f63073dadfbd9d699cc77842aa811b8093951c73968ea9d357a115d91017dc0cabb9efce1a0cc967c07f808d1648206f8fb5e9dc9333583138993f61a",
  ],
]);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function collectCounterpartContracts(): ContractDeclaration[] {
  _resetCollector();
  const finish = beginCollecting();
  crossKitBridgesInvariants();
  const decls: Declaration[] = finish();
  return decls.filter(
    (d): d is ContractDeclaration => d.kind === "contract",
  );
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
    // RUST_LIFT_PLUGIN_CONTRACT_CIDS map. If this fails the slab's
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
