// SPDX-License-Identifier: Apache-2.0
//
// print-lift-plugin-protocol-cids — extraction binary for cross-kit bridges.
//
// Walks the canonical `lift_plugin_protocol` slab and prints the
// `(contract_name, contractCid)` map for the protocol contract set as
// stable-ordered NDJSON on stdout, one record per line:
//
//   {"name": "<contract_name>", "cid": "blake3-512:..."}
//
// Used by peer kits and the Rust conformance harness to pin the shared
// protocol-contract target. The CID is the signer-independent contractCid
// from `provekit-claim-envelope::contract_cid`, not the surrounding signed
// attestation CID.
//
// Usage:
//   cargo run --release -p provekit-self-contracts \
//     --bin print-lift-plugin-protocol-cids
//
// Output is byte-deterministic: the underlying mint uses fixed
// `signer_seed = [0x42; 32]` and a pinned `produced_at`. Re-running this
// against a quiescent tree must produce identical CIDs, run to run.

use std::process::ExitCode;

use provekit_self_contracts::{
    lift_plugin_protocol_contract_cids, LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES,
};

fn main() -> ExitCode {
    let cids = match lift_plugin_protocol_contract_cids() {
        Ok(cids) => cids,
        Err(e) => {
            eprintln!("lift_plugin_protocol_contract_cids: {e}");
            return ExitCode::from(1);
        }
    };

    let mut missing: Vec<&str> = Vec::new();
    for name in LIFT_PLUGIN_PROTOCOL_CONTRACT_NAMES {
        match cids.get(*name) {
            Some(cid) => {
                // NDJSON: one record per line, keys in fixed order.
                println!("{{\"name\":\"{name}\",\"cid\":\"{cid}\"}}");
            }
            None => missing.push(name),
        }
    }

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
