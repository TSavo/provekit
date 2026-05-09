// SPDX-License-Identifier: Apache-2.0

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn build_provekit(root: &std::path::Path) -> PathBuf {
    let provekit = root.join("implementations/rust/target/debug/provekit");
    let mut build = Command::new("cargo");
    build
        .args([
            "build",
            "--quiet",
            "--manifest-path",
            "implementations/rust/provekit-cli/Cargo.toml",
            "--bin",
            "provekit",
        ])
        .current_dir(root);
    let build_output = build.output().expect("build provekit");
    assert!(
        build_output.status.success(),
        "build provekit failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );
    provekit
}

fn temp_dir(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("provekit-scr-{stamp}-{name}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_provekit_json(root: &std::path::Path, provekit: &std::path::Path, args: &[&str]) -> Value {
    let output = Command::new(provekit)
        .args(args)
        .arg("--json")
        .arg("--quiet")
        .env(
            "PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR",
            root.join("implementations/rust/target/supply-chain-rails-kit-rpc-test"),
        )
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("run provekit {}: {e}", args.join(" ")));
    assert!(
        output.status.success(),
        "provekit {} failed\nstdout={}\nstderr={}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "parse provekit JSON: {e}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn runner_help_is_self_contained() {
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-supply-chain-rails"))
        .arg("--help")
        .output()
        .expect("spawn provekit-supply-chain-rails --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-supply-chain-rails --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("provekit-supply-chain-rails"));
    assert!(stdout.contains("--all"));
    assert!(!stdout.contains("provekit zoo"));
}

#[test]
fn all_exhibits_show_conventional_green_then_provekit_red() {
    let root = repo_root();
    let provekit = build_provekit(&root);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit-supply-chain-rails"))
        .arg("--all")
        .arg("--json")
        .env("PROVEKIT_CLI", provekit)
        .env("PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI", "1")
        .env(
            "PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR",
            root.join("implementations/rust/target/supply-chain-rails-kit-rpc-test"),
        )
        .current_dir(&root)
        .output()
        .expect("run supply chain rails");

    assert!(
        output.status.success(),
        "runner failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).expect("runner JSON");
    assert_eq!(payload["ok"], true);
    let report = &payload["reports"][0];
    assert_eq!(
        report["ordinarySupplyChainReceipts"]["conventionalReceipts"]["slsaVerificationSummary"]
            ["verdict"],
        "green"
    );
    assert_eq!(
        report["ordinarySupplyChainReceipts"]["conventionalReceipts"]["inTotoPipeline"]["verdict"],
        "green"
    );
    assert_eq!(
        report["redRails"]["witness"]["reasonCode"],
        "env-secret-read"
    );
    assert_eq!(
        report["redRails"]["contractSet"]["missingContracts"],
        serde_json::json!(["runtime.no-env-secret-read"])
    );
    assert_eq!(report["redRails"]["binary"]["reason"], "binaryCid mismatch");
    assert_eq!(
        report["redRails"]["ciInputClosure"]["reason"],
        "inputClosureCid mismatch"
    );
}

#[test]
fn package_inspection_contract_set_matches_lifted_mint_contract_set() {
    let root = repo_root();
    let provekit = build_provekit(&root);
    let package = "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.1";
    let out = temp_dir("baseline-mint");

    let inspected = run_provekit_json(&root, &provekit, &["package", "inspect", package]);
    let minted = run_provekit_json(
        &root,
        &provekit,
        &[
            "mint",
            "--project",
            package,
            "--out",
            out.to_str().expect("temp path utf8"),
            "--no-attest",
        ],
    );

    assert_eq!(
        inspected["release"]["contractSetCid"],
        minted["contractSetCid"],
        "package inspection must derive release.contractSetCid from the same lifted contract set that provekit mint uses"
    );
    assert_ne!(
        inspected["release"]["contractSetCid"],
        "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "supply-chain contract rails must not be hand-authored placeholder CIDs"
    );
}

#[test]
fn package_inspection_uses_real_package_and_external_receipt_tools() {
    let root = repo_root();
    let provekit = build_provekit(&root);
    let package =
        "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie";

    let inspected = run_provekit_json(&root, &provekit, &["package", "inspect", package]);

    assert_eq!(inspected["artifact"]["path"], "package.tgz");
    assert_eq!(inspected["artifact"]["format"], "npm-pack-tarball");
    assert_eq!(inspected["artifact"]["gzip"], true);
    assert!(
        inspected["artifact"]["entries"]
            .as_array()
            .expect("artifact entries array")
            .iter()
            .any(|entry| entry.as_str() == Some("package/package.json")),
        "package inspection should report npm tar entries"
    );

    assert_eq!(
        inspected["conventionalReceipts"]["slsaVerificationSummary"]["tool"],
        "slsa-verifier"
    );
    assert_eq!(
        inspected["conventionalReceipts"]["slsaVerificationSummary"]["command"][1],
        "verify-vsa"
    );
    assert_eq!(
        inspected["conventionalReceipts"]["slsaVerificationSummary"]["verdict"],
        "green"
    );
    assert_eq!(
        inspected["conventionalReceipts"]["slsaVerificationSummary"]["predicateType"],
        "https://slsa.dev/verification_summary/v1"
    );

    assert_eq!(
        inspected["conventionalReceipts"]["inTotoPipeline"]["tool"],
        "in-toto-verify"
    );
    assert_eq!(
        inspected["conventionalReceipts"]["inTotoPipeline"]["verdict"],
        "green"
    );
    assert_eq!(
        inspected["conventionalReceipts"]["inTotoPipeline"]["step"],
        "safe-json-pack"
    );
}
