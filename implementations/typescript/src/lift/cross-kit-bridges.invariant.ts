// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for cross-kit bridges between the TypeScript lift adapters
// and the Rust kit's `lift_plugin_protocol` contracts.
//
// Phase 2 of the cross-kit bridge work. The Rust kit (PR #84) ships 10
// machine-enforceable contracts encoding the rules of
// `protocol/specs/2026-04-30-lift-plugin-protocol.md` v1.2.0. Every peer
// kit's lift-plugin implementation MUST satisfy those rules. This file
// mints, in the TS kit, 10 counterpart `ContractDeclaration`s. Each is
// named `ts_<rust_contract_name>` and asserts "ts-kit's lift adapter
// satisfies Rust's <rule>".
//
// Each counterpart yields its own envelope CID because its name and
// formula make the canonical bytes unique. Those envelope CIDs are what
// the Phase-2 BridgeDeclarations (constructed in the sibling
// `cross-kit-bridges.test.ts`) point at as `targetContractCid`.
//
// SCOPE: this file emits the 10 counterpart contracts only. Bridges live
// in the sibling .test.ts because the kit's `bridge()` collector takes a
// `targetContractCid: string` directly, but the counterpart envelope CIDs
// are not known until after the contracts are minted. Phase 3 will wire
// bridge minting into the orchestrator (it currently filters for
// `kind === "contract"` and would silently drop a `bridge()` call here).
// Until then, the test is the single point that constructs, canonical-
// encodes, and pins the 10 bridges.
//
// The Rust contract CIDs were extracted from `mint_self_proof`'s
// `MintResult.contract_cids` map for the post-PR-#84 self-contracts
// bundle (commit 605b04c). They are pinned here so Phase-2 work can
// reference them without re-running the Rust mint. If a future Rust mint
// drifts these CIDs, the sibling `cross-kit-bridges.test.ts` will fail
// (the bridge bytes change, so the pinned bridge CID changes). That's
// the desired behavior: drift surfaces here as a test failure, not as a
// silent dangling pointer.

import {
  contract,
  eq,
  str,
  type IrTerm,
} from "../ir/symbolic/index.js";

// Single-arg constructor helper. Mirrors the convention used by every
// other `.invariant.ts` slab in the kit (zod, vitest-tests, etc.).
function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

/**
 * Rust contract CIDs from the lift_plugin_protocol slab in the rust
 * self-contracts bundle (post-PR-#84). Pinned here so Phase-2 bridges can
 * reference them without re-running the Rust mint.
 */
export const RUST_LIFT_PLUGIN_CONTRACT_CIDS: ReadonlyMap<string, string> =
  new Map([
    [
      "lift_plugin_initialize_protocol_version_match",
      "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a",
    ],
    [
      "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
      "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099",
    ],
    [
      "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
      "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0",
    ],
    [
      "lift_plugin_lift_request_surface_is_string",
      "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a",
    ],
    [
      "lift_plugin_lift_request_source_paths_nonempty",
      "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22",
    ],
    [
      "lift_plugin_lift_request_source_paths_each_nonempty",
      "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822",
    ],
    [
      "lift_plugin_lift_request_surface_in_capabilities",
      "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51",
    ],
    [
      "lift_plugin_lift_response_kind_in_set",
      "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0",
    ],
    [
      "lift_plugin_lift_response_ir_document_array",
      "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20",
    ],
    [
      "lift_plugin_diagnostic_field_is_array",
      "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35",
    ],
  ]);

/** Source layer for Phase-2 bridges: the upstream Rust self-contracts kit. */
export const RUST_KIT_LAYER = "rust-kit";

/** Target layer for Phase-2 bridges: this TypeScript kit. */
export const TS_KIT_LAYER = "typescript-kit";

/**
 * Phase-3 placeholder for the ts lift module's .proof bundle CID. The
 * ts lift binary is not yet bundled into a signed .proof; once it is,
 * `targetProofCid` will pin the specific bundle and lock the bridge
 * against attestation swap. Until then, this sentinel keeps the field
 * present and machine-readable without lying about a real CID.
 */
export const DEFERRED_TS_LIFT_BINARY_CID = "deferred:phase-3-proof-bundle";

/** Notes attached to every Phase-2 bridge declaration. */
export const PHASE_2_BRIDGE_NOTES =
  "lift-plugin-protocol conformance bridge; phase 2";

/**
 * Build the counterpart contract name for a given Rust contract name.
 * Each counterpart asserts "ts-kit's lift adapter satisfies Rust's
 * <rust_contract_name>".
 */
export function counterpartContractName(rustContractName: string): string {
  return `ts_${rustContractName}`;
}

/** Build the bridge declaration name for a given Rust contract name. */
export function bridgeName(rustContractName: string): string {
  return `bridge_to_${rustContractName}`;
}

export function invariants(): void {
  for (const rustContractName of RUST_LIFT_PLUGIN_CONTRACT_CIDS.keys()) {
    const counterpartName = counterpartContractName(rustContractName);

    // Counterpart contract: a named declaration asserting that the ts
    // lift adapter satisfies the Rust rule. The post is a uniqueness-
    // bearing equality whose ctor name encodes the conformance claim.
    // Z3 has no semantics for the ctor; the contract's value is the
    // named-membership shape, which the bridge below pins into a closed
    // source-target pair.
    contract(counterpartName, {
      post: eq(
        ctor1(
          "ts_lift_satisfies_rust_contract",
          str(rustContractName),
        ),
        ctor1("true_const", str("")),
      ),
    });
  }
}
