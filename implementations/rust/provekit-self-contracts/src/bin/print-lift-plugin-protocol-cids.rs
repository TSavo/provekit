// SPDX-License-Identifier: Apache-2.0
//
// print-lift-plugin-protocol-cids — extraction binary for cross-kit bridges.
//
// Walks `author_all_invariants()` -> `mint_self_proof()` and prints the
// `(contract_name, cid)` map for the lift_plugin_protocol slab as
// stable-ordered NDJSON on stdout, one record per line:
//
//   {"name": "<contract_name>", "cid": "blake3-512:..."}
//
// Used by peer kits (python, go, ts, csharp) to mint cross-kit bridge
// declarations whose `sourceContractCid` field pins the rust contract by
// its envelope CID. The CID is the hash of the signed CBOR claim envelope
// produced by `mint_contract` (NOT a JCS-of-the-formula hash); peers cannot
// recompute it locally without replicating the full sign pipeline, so this
// binary is the extraction point.
//
// Usage:
//   cargo run --release -p provekit-self-contracts \
//     --bin print-lift-plugin-protocol-cids
//
// Output is byte-deterministic: the underlying mint uses fixed
// `signer_seed = [0x42; 32]` and a pinned `produced_at`. Re-running this
// against a quiescent tree must produce identical CIDs, run to run.

use std::process::ExitCode;

use provekit_self_contracts::mint_self_proof;

/// The set of contract names belonging to the lift_plugin_protocol slab,
/// in the order they're authored in `lift_plugin_protocol::invariants()`.
/// Hard-coded so that this binary FAILS LOUD if the slab adds/removes a
/// contract without updating peer kits' bridges.
const LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES: &[&str] = &[
    "lift_plugin_initialize_protocol_version_match",
    "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "lift_plugin_lift_request_surface_is_string",
    "lift_plugin_lift_request_source_paths_nonempty",
    "lift_plugin_lift_request_source_paths_each_nonempty",
    "lift_plugin_lift_request_surface_in_capabilities",
    "lift_plugin_lift_response_kind_in_set",
    "lift_plugin_lift_response_ir_document_array",
    "lift_plugin_diagnostic_field_is_array",
    // C8: lifter must emit call-edge stream alongside contracts (spec #114 R1).
    "lift_emits_call_edge_stream",
];

fn main() -> ExitCode {
    let tmp = std::env::temp_dir().join(format!(
        "provekit-print-lift-plugin-cids-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);

    let mint = match mint_self_proof(&tmp) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("mint_self_proof: {e}");
            let _ = std::fs::remove_dir_all(&tmp);
            return ExitCode::from(1);
        }
    };

    let mut missing: Vec<&str> = Vec::new();
    for name in LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES {
        match mint.contract_cids.get(*name) {
            Some(cid) => {
                // NDJSON: one record per line, keys in fixed order.
                println!("{{\"name\":\"{name}\",\"cid\":\"{cid}\"}}");
            }
            None => missing.push(name),
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);

    if !missing.is_empty() {
        eprintln!(
            "ERROR: lift_plugin_protocol slab missing {} expected contract(s):",
            missing.len()
        );
        for name in &missing {
            eprintln!("  - {name}");
        }
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
