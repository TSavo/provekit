// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_ir_types::MigrateReceiptEnvelope;
use serde_json::Value;

const FIXTURE_STATE_CID: &str = "blake3-512:295e0fd280088fc1e5e00d7bade11a2bf850c932180622e28f2fc92e64f97cd5bd757a73acf07f888b7c523e8efb65d8f0d01d50bc02740e5d771e750485d8f4";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("provekit-cli has rust workspace parent")
        .parent()
        .expect("rust workspace has implementations parent")
        .parent()
        .expect("implementations dir has repo parent")
        .to_path_buf()
}

#[test]
fn verifier_requires_empirically_witnessed_cross_language_consensus() {
    let repo = repo_root();
    let temp = tempfile::tempdir().expect("temp output");

    let sqlite_receipt = temp.path().join("sqlite3").join("migrate-receipt.proof");
    assert_success(
        "python-sqlite3",
        &run_migrate(
            &repo,
            "python-sqlite3",
            sqlite_receipt.parent().unwrap(),
            &sqlite_receipt,
        ),
    );

    let aiosqlite_receipt = temp.path().join("aiosqlite").join("migrate-receipt.proof");
    assert_success(
        "python-aiosqlite",
        &run_migrate(
            &repo,
            "python-aiosqlite",
            aiosqlite_receipt.parent().unwrap(),
            &aiosqlite_receipt,
        ),
    );

    assert!(
        sql_query_witness_count(&[&sqlite_receipt, &aiosqlite_receipt]) >= 8,
        "the #877 cross-language receipts must carry enough sql-query witnesses for the policy vector"
    );

    let catalog = temp.path().join(".provekit").join("promotions");
    fs::create_dir_all(&catalog).expect("create promotion catalog");
    let promotion = catalog.join("sql-query-consensus.proof");
    let policy = temp.path().join("consensus-policy.json");
    write_consensus_policy(&policy);
    assert_success(
        "witness consensus",
        &Command::new(cargo())
            .current_dir(repo.join("implementations").join("rust"))
            .arg("run")
            .arg("-p")
            .arg("provekit-cli")
            .arg("--")
            .arg("witness")
            .arg("consensus")
            .arg("--concept")
            .arg("concept:sql-query")
            .arg("--require-fixture")
            .arg(FIXTURE_STATE_CID)
            .arg("--min-witnesses")
            .arg("8")
            .arg("--consensus-policy")
            .arg(&policy)
            .arg("--catalog")
            .arg(&sqlite_receipt)
            .arg("--catalog")
            .arg(&aiosqlite_receipt)
            .arg("--emit")
            .arg(&promotion)
            .output()
            .expect("spawn witness consensus"),
    );

    let status = Command::new(cargo())
        .current_dir(repo.join("implementations").join("rust"))
        .arg("run")
        .arg("-p")
        .arg("provekit-cli")
        .arg("--")
        .arg("witness")
        .arg("status")
        .arg("--project")
        .arg(temp.path())
        .arg("--concept")
        .arg("concept:sql-query")
        .arg("--require-fixture")
        .arg(FIXTURE_STATE_CID)
        .arg("--json")
        .output()
        .expect("spawn witness status");
    assert_success("witness status", &status);

    let verify = Command::new(cargo())
        .current_dir(repo.join("implementations").join("rust"))
        .arg("run")
        .arg("-p")
        .arg("provekit-cli")
        .arg("--")
        .arg("verify")
        .arg(temp.path())
        .arg("--require-empirically-witnessed")
        .arg("concept:sql-query")
        .arg("--require-fixture")
        .arg(FIXTURE_STATE_CID)
        .arg("--consensus-policy")
        .arg(&policy)
        .arg("--json")
        .output()
        .expect("spawn verify tier query");
    assert_success("verify --require-empirically-witnessed", &verify);

    let stdout = String::from_utf8(verify.stdout).expect("verify stdout utf8");
    let report: Value = serde_json::from_str(&stdout).expect("verify JSON report");
    assert_eq!(report["ok"], true);
    assert_eq!(report["verdict"], "accepted");
    assert_eq!(report["requirement"]["concept"], "concept:sql-query");
    assert_eq!(
        report["requirement"]["fixture_state_cid"],
        FIXTURE_STATE_CID
    );
    assert!(report["requirement"]["policy_cid"]
        .as_str()
        .unwrap_or_default()
        .starts_with("blake3-512:"));
    assert_eq!(
        report["promotion"]["consensus_vector"]["unique_fixtures"],
        1
    );
    assert!(
        report["promotion"]["witnesses_consulted"]
            .as_u64()
            .unwrap_or(0)
            >= 8,
        "promotion must cite at least the policy's witness floor"
    );
    assert!(
        report["promotion"]["consensus_vector"]["total_sample_count"]
            .as_u64()
            .unwrap_or(0)
            >= 8,
        "policy consumes the vector's sample-depth axis"
    );
}

fn write_consensus_policy(path: &Path) {
    fs::write(
        path,
        r#"{
  "kind": "consensus-policy",
  "schemaVersion": "1",
  "name": "test-cross-language-sql-query",
  "thresholds": [
    {"axis": "min-witnesses-floor", "predicate": "n>=8"},
    {"axis": "environment-diversity", "predicate": "unique_fixtures>=1"},
    {"axis": "sample-depth", "predicate": "total_sample_count>=8"}
  ],
  "allow_failures": false
}
"#,
    )
    .expect("write consensus policy");
}

fn run_migrate(
    repo: &Path,
    library_to: &str,
    out_dir: &Path,
    receipt: &Path,
) -> std::process::Output {
    let source_dir = repo
        .join("examples")
        .join("migrate-demo")
        .join("users-better-sqlite3");
    let fixture = source_dir.join("fixture.sqlite");
    Command::new(cargo())
        .current_dir(repo.join("implementations").join("rust"))
        .arg("run")
        .arg("-p")
        .arg("provekit-cli")
        .arg("--")
        .arg("bind")
        .arg("--library-from")
        .arg("typescript-better-sqlite3")
        .arg("--library-to")
        .arg(library_to)
        .arg("--source-dir")
        .arg(source_dir)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--receipt")
        .arg(receipt)
        .arg("--witness-fixture")
        .arg(fixture)
        .arg("--write")
        .output()
        .expect("spawn cargo run migrate bind")
}

fn sql_query_witness_count(paths: &[&Path]) -> usize {
    let mut cids = std::collections::BTreeSet::new();
    for path in paths {
        let raw = fs::read_to_string(path).expect("receipt written");
        let receipt: MigrateReceiptEnvelope = serde_json::from_str(&raw).expect("parse receipt");
        receipt.validate().expect("receipt validates");
        cids.extend(
            receipt
                .witnesses
                .iter()
                .filter(|witness| witness.fixture_state_cid == FIXTURE_STATE_CID)
                .filter(|witness| witness.witness_for == sql_query_concept_cid())
                .map(|witness| witness.cid.clone()),
        );
    }
    cids.len()
}

fn sql_query_concept_cid() -> &'static str {
    "blake3-512:dd0429a4d4276c076f5dde08a993a046afa15dd36433b1d89c4bc18831f63733788abef283ffb78b2b9f88c607593741367a35ebcbd36a88132c06d6ff233ed1"
}

fn assert_success(label: &str, output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}
