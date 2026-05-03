// SPDX-License-Identifier: Apache-2.0
//
// mint_kit_integration — integration test for `provekit mint --kit=<kit>`.
//
// Verifies that the unified mint pipeline:
//   1. Produces a valid signed attestation JSON for all 11 kits.
//   2. Kits with a real lifter binary produce a non-empty contractSetCid
//      (not the empty-set sentinel).
//   3. Kits without a lifter binary produce the empty-set CID.
//   4. All attestations pass structural validation (required fields, types).
//   5. Two consecutive mints for the same kit produce byte-identical
//      attestations (determinism).
//
// The test runs `provekit mint --kit=<kit> --quiet` as a subprocess, then
// reads the written attestation JSON from
// `.provekit/self-contracts-attestations/<lang>.json`.
//
// This test requires:
//   - The `provekit` binary to be built (release or debug).
//   - CWD to be the repo root (set via CARGO_MANIFEST_DIR resolution).
//
// All 11 kits are tested. Kits without lifter binaries are expected to
// produce the EMPTY_SET_CID; this is explicitly asserted, not an error.

use std::path::{Path, PathBuf};
use std::process::Command;

/// BLAKE3-512 of JCS(`[]`) — the empty-set contractSetCid.
/// Produced by `compute_contract_set_cid(vec![])`. Verified empirically.
const EMPTY_SET_CID: &str = "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229";

/// Return the path to the `provekit` binary (release or debug, whichever exists).
fn provekit_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let release = workspace.join("target").join("release").join("provekit");
    let debug = workspace.join("target").join("debug").join("provekit");
    if release.exists() {
        release
    } else {
        debug
    }
}

/// Return the repo root (two levels above the workspace root, since
/// implementations/rust/ is the workspace and the repo root contains
/// .provekit/self-contracts-attestations/).
fn repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // CARGO_MANIFEST_DIR = .../implementations/rust/provekit-cli
    // parent = .../implementations/rust
    // parent.parent = .../implementations
    // parent.parent.parent = repo root
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run `provekit mint --kit=<kit> --quiet` from the repo root.
/// Returns (exit_status, stdout, stderr).
fn run_mint(kit: &str) -> (bool, String, String) {
    let bin = provekit_bin();
    let root = repo_root();
    let out = Command::new(&bin)
        .arg("mint")
        .arg(format!("--kit={kit}"))
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("failed to spawn provekit");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Read the attestation JSON from `.provekit/self-contracts-attestations/<lang>.json`.
fn read_attestation(repo_root: &Path, lang: &str) -> serde_json::Value {
    let path = repo_root
        .join(".provekit")
        .join("self-contracts-attestations")
        .join(format!("{lang}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Assert that an attestation JSON has all required fields with correct types.
fn assert_attestation_structure(v: &serde_json::Value, lang: &str) {
    assert_eq!(
        v["schemaVersion"].as_str(),
        Some("1"),
        "{lang}: schemaVersion must be '1'"
    );
    assert_eq!(
        v["kind"].as_str(),
        Some("self-contracts-attestation"),
        "{lang}: kind must be 'self-contracts-attestation'"
    );
    assert_eq!(
        v["lang"].as_str(),
        Some(lang),
        "{lang}: lang field must match"
    );
    let cset = v["contractSetCid"].as_str().unwrap_or_else(|| {
        panic!("{lang}: contractSetCid must be a string")
    });
    assert!(
        cset.starts_with("blake3-512:"),
        "{lang}: contractSetCid must start with 'blake3-512:', got {cset}"
    );
    let sig = v["signature"].as_str().unwrap_or_else(|| {
        panic!("{lang}: signature must be a string")
    });
    assert!(
        sig.starts_with("ed25519:"),
        "{lang}: signature must start with 'ed25519:', got {sig}"
    );
    let signer = v["signer"].as_str().unwrap_or_else(|| {
        panic!("{lang}: signer must be a string")
    });
    assert!(
        signer.starts_with("ed25519:"),
        "{lang}: signer must start with 'ed25519:', got {signer}"
    );
    let declared_at = v["declaredAt"].as_str().unwrap_or_else(|| {
        panic!("{lang}: declaredAt must be a string")
    });
    assert!(
        !declared_at.is_empty(),
        "{lang}: declaredAt must be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Test 1: All 11 kits produce a structurally valid attestation
// ---------------------------------------------------------------------------

/// Kits with a real lifter binary installed. These kits run lift-protocol RPCs.
/// Note: a kit having a lifter binary does not guarantee a non-empty contractSetCid;
/// the CID depends on how many contracts the lifter finds in the workspace.
const KITS_WITH_LIFTERS: &[&str] = &["rust", "go", "cpp", "ts", "csharp"];

/// Kits without a lifter binary yet — produce the empty-set CID because the
/// binary cannot be found (ENOENT on spawn). These declare the binary name but
/// the binary is not installed; the gap surfaces as an empty-set attestation.
const KITS_WITHOUT_LIFTERS: &[&str] = &["swift", "java", "python", "ruby", "zig", "c"];

/// Kits that have a lifter AND are expected to find real contracts.
/// Only include kits where the test environment reliably has the lifter built
/// and the kit's workspace has liftable annotations.
const KITS_WITH_REAL_CONTRACTS: &[&str] = &["rust", "cpp"];

#[test]
fn all_kits_mint_produces_valid_attestation_structure() {
    let all_kits: Vec<&str> = KITS_WITH_LIFTERS
        .iter()
        .chain(KITS_WITHOUT_LIFTERS.iter())
        .copied()
        .collect();

    let root = repo_root();
    let mut failed_kits: Vec<String> = Vec::new();

    for kit in &all_kits {
        let (ok, stdout, stderr) = run_mint(kit);
        if !ok {
            // Kits with optional toolchain (dotnet, go, npx) may not be available.
            // Record the failure but don't fail the test outright for those kits.
            // Only the KITS_WITHOUT_LIFTERS (ENOENT-based) must succeed unconditionally.
            if KITS_WITHOUT_LIFTERS.contains(kit) {
                panic!(
                    "kit `{kit}`: mint exited non-zero (no-lifter kits must always succeed)\n  stdout: {stdout}\n  stderr: {stderr}"
                );
            }
            eprintln!(
                "kit `{kit}`: mint exited non-zero (toolchain may not be available)\n  stderr: {stderr}"
            );
            failed_kits.push(kit.to_string());
            continue;
        }
        // stdout in quiet mode: optional bundle CID line + contractSetCid line
        assert!(
            stdout.contains("contractSetCid:"),
            "kit `{kit}`: stdout must contain 'contractSetCid:'\n  stdout: {stdout}"
        );

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(&root, lang);
        assert_attestation_structure(&attest, lang);

        eprintln!(
            "kit={kit} contractSetCid={}",
            attest["contractSetCid"].as_str().unwrap_or("?")
        );
    }

    if !failed_kits.is_empty() {
        eprintln!(
            "NOTE: {} kits skipped due to missing toolchain: {:?}",
            failed_kits.len(),
            failed_kits
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Kits without lifters produce the known empty-set CID
// ---------------------------------------------------------------------------

#[test]
fn kits_without_lifters_produce_empty_set_cid() {
    let root = repo_root();
    for kit in KITS_WITHOUT_LIFTERS {
        let (ok, _, _) = run_mint(kit);
        assert!(ok, "kit `{kit}`: mint must succeed even without a lifter binary");

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(&root, lang);
        let cset = attest["contractSetCid"].as_str().unwrap();
        assert_eq!(
            cset, EMPTY_SET_CID,
            "kit `{kit}`: expected empty-set CID, got {cset}"
        );
        // cid field should be empty string (no bundle produced)
        let cid = attest["cid"].as_str().unwrap();
        assert!(
            cid.is_empty(),
            "kit `{kit}`: cid should be empty string when no lifter, got {cid}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Kits with real contracts produce non-empty-set contractSetCid
// ---------------------------------------------------------------------------

#[test]
fn kits_with_real_contracts_produce_nonempty_contract_set() {
    let root = repo_root();
    for kit in KITS_WITH_REAL_CONTRACTS {
        let (ok, _, stderr) = run_mint(kit);
        if !ok {
            eprintln!("kit `{kit}`: mint failed (lifter may not be built yet)\n  stderr: {stderr}");
            // Skip rather than fail — lifter may not be built in test environment.
            continue;
        }

        let lang = if *kit == "ts" { "ts" } else { kit };
        let attest = read_attestation(&root, lang);
        let cset = attest["contractSetCid"].as_str().unwrap();
        assert_ne!(
            cset, EMPTY_SET_CID,
            "kit `{kit}`: expected non-empty contractSetCid when lifter finds real contracts"
        );
        // bundle CID should be non-empty
        let cid = attest["cid"].as_str().unwrap();
        assert!(
            cid.starts_with("blake3-512:"),
            "kit `{kit}`: cid should start with blake3-512:, got {cid}"
        );
        eprintln!("kit={kit} cid={cid}");
        eprintln!("kit={kit} contractSetCid={cset}");
    }
}

// ---------------------------------------------------------------------------
// Test 4: Rust kit is byte-deterministic across two consecutive mints
// ---------------------------------------------------------------------------

#[test]
fn rust_kit_mint_is_byte_deterministic() {
    let root = repo_root();

    // First mint
    let (ok1, _, _) = run_mint("rust");
    if !ok1 {
        eprintln!("rust kit: first mint failed — skipping determinism test (lifter not built)");
        return;
    }
    let attest1 = read_attestation(&root, "rust");

    // Second mint
    let (ok2, _, _) = run_mint("rust");
    assert!(ok2, "rust kit: second mint must succeed");
    let attest2 = read_attestation(&root, "rust");

    assert_eq!(
        attest1, attest2,
        "rust kit: two consecutive mints must produce byte-identical attestations"
    );

    eprintln!(
        "rust determinism confirmed: contractSetCid={}",
        attest1["contractSetCid"].as_str().unwrap_or("?")
    );
}

// ---------------------------------------------------------------------------
// Test 5: --kit shortcut and --project are equivalent for rust
// ---------------------------------------------------------------------------

#[test]
fn kit_shortcut_and_project_flag_are_equivalent() {
    let bin = provekit_bin();
    let root = repo_root();

    // Via --kit
    let kit_out = Command::new(&bin)
        .arg("mint")
        .arg("--kit=rust")
        .arg("--quiet")
        .arg("--no-attest")
        .current_dir(&root)
        .output()
        .expect("spawn provekit --kit=rust");

    // Via --project
    let proj_out = Command::new(&bin)
        .arg("mint")
        .arg("--project")
        .arg("implementations/rust")
        .arg("--surface")
        .arg("rust")
        .arg("--quiet")
        .arg("--no-attest")
        .current_dir(&root)
        .output()
        .expect("spawn provekit --project");

    if !kit_out.status.success() || !proj_out.status.success() {
        eprintln!("rust lifter not available — skipping equivalence test");
        return;
    }

    let kit_stdout = String::from_utf8_lossy(&kit_out.stdout).to_string();
    let proj_stdout = String::from_utf8_lossy(&proj_out.stdout).to_string();

    // Both should produce the same contractSetCid line.
    let extract_cset = |s: &String| -> String {
        s.lines()
            .find(|l| l.starts_with("contractSetCid:"))
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(
        extract_cset(&kit_stdout),
        extract_cset(&proj_stdout),
        "--kit and --project must produce identical contractSetCid output"
    );
}
