// SPDX-License-Identifier: Apache-2.0
//
// Native TypeScript source for cross-kit bridge constants between the
// TypeScript lift adapters and the Rust kit's lift-plugin-protocol contracts.

/**
 * Rust contract CIDs from the lift_plugin_protocol slab in the rust
 * self-contracts bundle. Pinned here so Phase-2 bridges can reference them
 * without re-running the Rust mint.
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
 * Phase-3 placeholder for the ts lift module's .proof bundle CID.
 */
export const DEFERRED_TS_LIFT_BINARY_CID = "deferred:phase-3-proof-bundle";

/** Notes attached to every Phase-2 bridge declaration. */
export const PHASE_2_BRIDGE_NOTES =
  "lift-plugin-protocol conformance bridge; phase 2";

/**
 * Build the counterpart contract name for a given Rust contract name.
 */
export function counterpartContractName(rustContractName: string): string {
  return `ts_${rustContractName}`;
}

/** Build the bridge declaration name for a given Rust contract name. */
export function bridgeName(rustContractName: string): string {
  return `bridge_to_${rustContractName}`;
}

export function tsLiftSatisfiesRustContract(rustContractName: string): string {
  return RUST_LIFT_PLUGIN_CONTRACT_CIDS.has(rustContractName) ? "true" : "false";
}

export function trueConst(): string {
  return "true";
}
