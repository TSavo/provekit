// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root ancestor")
        .to_path_buf()
}

#[test]
fn self_check_on_rust_std_shim_emits_scoreboard_shape_and_hard_invariants() {
    let repo = repo_root();
    let walk_rpc = repo
        .join("implementations")
        .join("rust")
        .join("target")
        .join("debug")
        .join("provekit-walk-rpc");
    let provekit_lift = repo
        .join("implementations")
        .join("rust")
        .join("target")
        .join("debug")
        .join("provekit-lift");
    assert!(
        walk_rpc.exists(),
        "build the walk RPC binary first: cargo build -p provekit-walk --bin provekit-walk-rpc"
    );
    assert!(
        provekit_lift.exists(),
        "build the lift RPC binary first: cargo build -p provekit-lift --bin provekit-lift"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .current_dir(&repo)
        .args([
            "self-check",
            "--target",
            "examples/provekit-shim-rust-std",
            "--json",
        ])
        .output()
        .expect("run provekit self-check");

    assert!(
        output.status.success(),
        "self-check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let scoreboard: Value =
        serde_json::from_slice(&output.stdout).expect("self-check stdout is JSON");
    assert_eq!(
        scoreboard["target"], "examples/provekit-shim-rust-std",
        "target path is repo-relative and stable"
    );
    assert!(
        scoreboard["catalogCid"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "catalogCid is present"
    );
    assert!(
        scoreboard["lift"]["fnContracts"].as_u64().unwrap_or(0) > 0,
        "shim lifts function contracts"
    );
    assert!(scoreboard["lift"]["bodyDischargeEligible"].is_u64());
    assert!(scoreboard["lift"]["bodyDischargeIneligible"].is_object());
    assert!(scoreboard["bridges"]["emitted"].is_u64());
    assert!(scoreboard["bridges"]["liftGaps"].is_object());
    assert_eq!(scoreboard["silentlyDropped"], 0);
    assert_eq!(scoreboard["dischargeSplit"]["falsePass"], 0);
    assert!(scoreboard["dischargeSplit"]["panicSafe"].is_u64());
    assert!(scoreboard["dischargeSplit"]["reflexive"].is_u64());
    assert!(scoreboard["dischargeSplit"]["vacuous"].is_u64());
    assert!(scoreboard["dischargeSplit"]["undecidable"].is_u64());
    assert!(scoreboard["panicCensus"].is_array());
}
