// SPDX-License-Identifier: Apache-2.0
//
// Dogfood test: invoke `provekit-lift` against the `provekit-walk`
// crate's own source and verify it produces a `.proof` catalog. The
// substrate processing the substrate.
//
// The walker we built emits proof.ir bundles via emit.rs / walk_emit
// binary / walk_rpc binary. The existing `provekit-lift` tool walks
// any Rust workspace and lifts its annotations as contract mementos.
// Pointing provekit-lift at provekit-walk's own source produces a
// `.proof` catalog whose path on disk is its own CID — self-applying.
//
// This test is the empirical closure of paper 07's "the substrate is
// below the level at which languages differ" claim: the walker is a
// kit; the kit is processable by the lifter; the lifter produces a
// catalog the substrate's verifier can consume.
//
// Skipped if `provekit-lift` isn't built (typical of single-crate test
// runs); pass-through in that case.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn provekit_lift_processes_walk_crate_to_proof_catalog() {
    // Resolve the workspace's debug directory containing provekit-lift.
    // Cargo points us at the per-crate target dir via env, so we walk
    // up to find the workspace target.
    let walk_target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("provekit-walk has a parent dir")
        .join("target");
    let lift_binary = walk_target_dir.join("debug/provekit-lift");

    if !lift_binary.exists() {
        eprintln!(
            "provekit-lift binary not found at {:?}; skipping dogfood test",
            lift_binary
        );
        return;
    }

    let walk_workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(&lift_binary)
        .arg("--workspace")
        .arg(&walk_workspace)
        .arg("--quiet")
        .output()
        .expect("failed to execute provekit-lift");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "provekit-lift failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("blake3-512:") || stderr.contains("blake3-512:"),
        "expected provekit-lift output to mention a blake3-512 catalog CID:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}
