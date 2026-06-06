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

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("create copy destination");
    for entry in std::fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("read source dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to)
                .unwrap_or_else(|e| panic!("copy {} to {}: {e}", from.display(), to.display()));
        }
    }
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
fn all_mode_rejects_empty_specimen_root() {
    let root = temp_dir("empty-all-root");
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-supply-chain-rails"))
        .arg(&root)
        .arg("--all")
        .output()
        .expect("run provekit-supply-chain-rails --all empty root");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "--all should reject an empty specimen root\nstdout={}\nstderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("no Supply Chain Rails specimens found"),
        "stderr={stderr}"
    );
}

#[test]
#[ignore = "superseded: drives the retired `lower` JS-lowerer (#1476). Pending emit-era redesign; do not delete."]
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
    assert!(report["redRails"]["witness"]["evidenceCid"]
        .as_str()
        .expect("witness evidenceCid")
        .starts_with("blake3-512:"));
    assert_eq!(
        report["redRails"]["witness"]["unsupportedSemantics"],
        serde_json::json!([])
    );
    assert_eq!(
        report["redRails"]["witness"]["findings"][0]["expression"],
        "process.env.SAFE_JSON_TOKEN"
    );
    assert_eq!(
        report["redRails"]["witness"]["sourceSpans"][0]["lineStart"],
        8
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
#[ignore = "blocked on lower->emit migration: `provekit mint` can't discharge the `package.no-install-side-effect` witness because it is still produced by supply-chain-js-lowerer.rs (a `lower`-era kit) and has no emit witness emitter (#1476). Re-enable when the supply-chain demo's witness production is redone on emit. See docs/audits/2026-05-25-architecture-ground-truth.md. Do not delete."]
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
    // Requires the real `slsa-verifier` native receipt tool on PATH; skip when
    // absent (lean CI does not install it).
    if std::process::Command::new("slsa-verifier")
        .arg("version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipping package_inspection: slsa-verifier not on PATH");
        return;
    }
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

#[test]
#[ignore = "drives the supply-chain-npm lifter (run-supply-chain-npm-lifter.sh -> tsx), which the TypeScript bankruptcy (#1936) deleted: the run now fails 'tsx not found' before reaching the missing-tarball path the assertion checks. Sibling tsx-driven tests are already ignored (#1476); re-enable when the npm supply-chain demo is redone off the deleted TS lifter. Do not delete."]
fn package_inspection_rejects_missing_npm_tarball() {
    let root = repo_root();
    let provekit = build_provekit(&root);
    let source = root
        .join("menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie");
    let package = temp_dir("missing-package-tgz");
    copy_dir(&source, &package);
    let kit_root = root.join("menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc");
    for rel in [
        ".provekit/lift/supply-chain-npm/manifest.toml",
        ".provekit/lower/javascript/manifest.toml",
        ".provekit/lower/package-manifest/manifest.toml",
    ] {
        let path = package.join(rel);
        let text = std::fs::read_to_string(&path).expect("read copied manifest");
        let text = text
            .replace(
                "../../kit-rpc/run-supply-chain-npm-lifter.sh",
                &kit_root
                    .join("run-supply-chain-npm-lifter.sh")
                    .display()
                    .to_string(),
            )
            .replace(
                "../../kit-rpc/run-supply-chain-js-lowerer.sh",
                &kit_root
                    .join("run-supply-chain-js-lowerer.sh")
                    .display()
                    .to_string(),
            );
        std::fs::write(&path, text).expect("write copied manifest");
    }
    std::fs::remove_file(package.join("package.tgz")).expect("remove package.tgz");

    let output = Command::new(&provekit)
        .args(["package", "inspect"])
        .arg(&package)
        .arg("--json")
        .arg("--quiet")
        .env(
            "PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR",
            root.join("implementations/rust/target/supply-chain-rails-kit-rpc-test"),
        )
        .current_dir(&root)
        .output()
        .expect("run provekit package inspect");

    assert!(
        !output.status.success(),
        "package inspect should reject missing package.tgz\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("package.tgz"),
        "missing tarball error should name package.tgz\nstderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
