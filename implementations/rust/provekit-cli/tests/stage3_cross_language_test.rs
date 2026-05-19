// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
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

fn run_migrate(
    repo: &Path,
    library_to: &str,
    out_dir: &Path,
    receipt: &Path,
) -> std::process::Output {
    let target = tempfile::tempdir().expect("fresh target dir");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let source_dir = repo
        .join("examples")
        .join("migrate-demo")
        .join("users-better-sqlite3");
    let fixture = source_dir.join("fixture.sqlite");
    let mut cmd = Command::new(cargo);
    cmd.current_dir(repo.join("implementations").join("rust"))
        .env("CARGO_TARGET_DIR", target.path())
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
        .arg("--write");
    cmd.output().expect("spawn cargo run migrate bind")
}

#[test]
fn bind_migrates_better_sqlite3_to_python_with_cross_language_witness_pairs() {
    let repo = repo_root();
    let temp = tempfile::tempdir().expect("temp output");

    let sqlite_out = temp.path().join("users-python-sqlite3");
    let sqlite_receipt_path = sqlite_out.join("migrate-receipt.proof");
    let sqlite_output = run_migrate(&repo, "python-sqlite3", &sqlite_out, &sqlite_receipt_path);
    assert_success("python-sqlite3", &sqlite_output);
    let sqlite_receipt = read_receipt(&sqlite_receipt_path);
    let sqlite_json = read_receipt_json(&sqlite_receipt_path);
    assert_aggregate(
        "python-sqlite3",
        &sqlite_output,
        &sqlite_receipt,
        ExpectedAggregate {
            rewritten: 4,
            widened: 1,
            halted: 0,
            refused: 0,
            lossy: 1,
        },
    );
    assert_eq!(
        sqlite_receipt.promotion_decisions.len(),
        1,
        "sqlite3 migration emits one PromotionDecisionMemento for recordEvent (async contagion via lossy insert-and-get-id callsite)"
    );
    assert_python_source_parses(&sqlite_out.join("src").join("users.py"));
    assert_language_transition_claim(&sqlite_json, "getUserById", "get_user_by_id");
    assert_concept_binding_claim(&sqlite_receipt);
    assert_witness_pairs(&sqlite_json);

    let aiosqlite_out = temp.path().join("users-python-aiosqlite");
    let aiosqlite_receipt_path = aiosqlite_out.join("migrate-receipt.proof");
    let aiosqlite_output = run_migrate(
        &repo,
        "python-aiosqlite",
        &aiosqlite_out,
        &aiosqlite_receipt_path,
    );
    assert_success("python-aiosqlite", &aiosqlite_output);
    let aiosqlite_receipt = read_receipt(&aiosqlite_receipt_path);
    let aiosqlite_json = read_receipt_json(&aiosqlite_receipt_path);
    assert_aggregate(
        "python-aiosqlite",
        &aiosqlite_output,
        &aiosqlite_receipt,
        ExpectedAggregate {
            rewritten: 4,
            widened: 6,
            halted: 1,
            refused: 1,
            lossy: 1,
        },
    );
    assert_promotion_function_set(&aiosqlite_receipt);
    assert_python_source_parses(&aiosqlite_out.join("src").join("users.py"));
    assert_async_defs_match_promotions(
        &aiosqlite_out.join("src").join("users.py"),
        &aiosqlite_receipt,
    );
    assert_language_transition_claim(&aiosqlite_json, "getUserById", "get_user_by_id");
    assert_concept_binding_claim(&aiosqlite_receipt);
    assert_witness_pairs(&aiosqlite_json);
}

struct ExpectedAggregate {
    rewritten: usize,
    widened: usize,
    halted: usize,
    refused: usize,
    lossy: usize,
}

fn assert_success(label: &str, output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{label} migrate bind failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn assert_aggregate(
    label: &str,
    output: &std::process::Output,
    receipt: &MigrateReceiptEnvelope,
    expected: ExpectedAggregate,
) {
    assert_eq!(receipt.aggregate_summary.rewritten, expected.rewritten);
    assert_eq!(receipt.aggregate_summary.widened, expected.widened);
    assert_eq!(receipt.aggregate_summary.halted, expected.halted);
    assert_eq!(receipt.aggregate_summary.refused, expected.refused);
    assert_eq!(receipt.aggregate_summary.lossy, expected.lossy);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_lines = [
        format!("{} callsites rewritten", expected.rewritten),
        format!("{} functions widened to async", expected.widened),
        format!(
            "{} boundary handlers already async-capable",
            expected.halted
        ),
        format!(
            "{} refused exports because public API forbids promise return",
            expected.refused
        ),
        format!(
            "{} lossy callsites: kit-declared dimension divergence",
            expected.lossy
        ),
    ];
    for line in expected_lines {
        assert!(
            stdout.lines().any(|actual| actual == line),
            "{label} missing stdout line {line:?}\nstdout:\n{stdout}"
        );
    }
}

fn read_receipt(path: &Path) -> MigrateReceiptEnvelope {
    let raw = fs::read_to_string(path).expect("receipt written");
    let receipt: MigrateReceiptEnvelope = serde_json::from_str(&raw).expect("parse receipt");
    receipt.validate().expect("receipt validates");
    receipt
}

fn read_receipt_json(path: &Path) -> Value {
    let raw = fs::read_to_string(path).expect("receipt written");
    serde_json::from_str(&raw).expect("parse receipt JSON")
}

fn assert_python_source_parses(path: &Path) {
    let status = Command::new("python3")
        .arg("-c")
        .arg(format!(
            "import ast; ast.parse(open({:?}).read())",
            path.display().to_string()
        ))
        .status()
        .expect("spawn python ast parse");
    assert!(
        status.success(),
        "Python source must parse: {}",
        path.display()
    );
}

fn assert_promotion_function_set(receipt: &MigrateReceiptEnvelope) {
    let functions = receipt
        .promotion_decisions
        .iter()
        .map(|decision| {
            decision
                .header
                .decision_payload
                .get("function")
                .and_then(Value::as_str)
                .expect("promotion decision function")
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        functions,
        BTreeSet::from([
            "countUsers".to_string(),
            "getAllUsers".to_string(),
            "getUserById".to_string(),
            "recordEvent".to_string(),
            "renderDashboard".to_string(),
            "renderUsersPage".to_string(),
        ])
    );
}

fn assert_async_defs_match_promotions(path: &Path, receipt: &MigrateReceiptEnvelope) {
    let source = fs::read_to_string(path).expect("read Python output");
    let async_functions = source
        .lines()
        .filter_map(|line| line.strip_prefix("async def "))
        .filter_map(|rest| rest.split_once('(').map(|(name, _)| name.to_string()))
        .collect::<BTreeSet<_>>();
    let promoted = receipt
        .promotion_decisions
        .iter()
        .map(|decision| {
            let ts_name = decision
                .header
                .decision_payload
                .get("function")
                .and_then(Value::as_str)
                .expect("promotion function");
            snake_case(ts_name)
        })
        .collect::<BTreeSet<_>>();
    assert!(
        async_functions.is_superset(&promoted),
        "every promoted function must be rendered as async def"
    );
    assert!(
        async_functions.contains("handle_request"),
        "already async boundary must remain async in the aiosqlite target"
    );
    let allowed = promoted
        .iter()
        .cloned()
        .chain(["handle_request".to_string()])
        .collect::<BTreeSet<_>>();
    assert_eq!(async_functions, allowed);
}

fn assert_language_transition_claim(receipt: &Value, source_name: &str, target_name: &str) {
    let transitions = receipt
        .get("language_transitions")
        .and_then(Value::as_array)
        .expect("language_transitions array");
    assert!(
        transitions.iter().any(|transition| {
            transition
                .get("function_name_source")
                .and_then(Value::as_str)
                == Some(source_name)
                && transition
                    .get("function_language_source")
                    .and_then(Value::as_str)
                    == Some("typescript")
                && transition
                    .get("function_name_target")
                    .and_then(Value::as_str)
                    == Some(target_name)
                && transition
                    .get("function_language_target")
                    .and_then(Value::as_str)
                    == Some("python")
                && transition.get("naming_convention").and_then(Value::as_str)
                    == Some("camelCase -> snake_case")
                && transition
                    .get("signature_equivalence")
                    .and_then(Value::as_str)
                    == Some("structural")
        }),
        "missing source to target LanguageTransitionMemento for {source_name}"
    );
}

fn assert_concept_binding_claim(receipt: &MigrateReceiptEnvelope) {
    assert_eq!(receipt.concept_sites.len(), 4);
    for site in &receipt.concept_sites {
        assert!(
            site.concept_cid.starts_with("blake3-512:"),
            "concept CID must be explicit"
        );
        assert_ne!(
            site.source_binding_cid, site.target_binding_cid,
            "source and target bindings must differ while concept CID stays fixed"
        );
    }
}

fn assert_witness_pairs(receipt: &Value) {
    let witnesses = receipt
        .get("witnesses")
        .and_then(Value::as_array)
        .expect("witnesses array");
    let witness_by_cid = witnesses
        .iter()
        .map(|witness| {
            (
                witness
                    .get("cid")
                    .and_then(Value::as_str)
                    .expect("witness cid"),
                witness,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let pairs = receipt
        .get("cross_language_witness_pairs")
        .and_then(Value::as_array)
        .expect("cross_language_witness_pairs array");
    let concept_sites = receipt
        .get("concept_sites")
        .and_then(Value::as_array)
        .expect("concept_sites array");
    assert_eq!(pairs.len(), concept_sites.len());
    for pair in pairs {
        assert_eq!(
            pair.get("equivalence_outcome").and_then(Value::as_str),
            Some("pass")
        );
        let source = witness_by_cid[pair
            .get("source_witness_cid")
            .and_then(Value::as_str)
            .expect("source witness cid")];
        let target = witness_by_cid[pair
            .get("target_witness_cid")
            .and_then(Value::as_str)
            .expect("target witness cid")];
        assert_eq!(
            source
                .get("fixture_state_cid")
                .and_then(Value::as_str)
                .expect("source fixture cid"),
            FIXTURE_STATE_CID
        );
        assert_eq!(
            target
                .get("fixture_state_cid")
                .and_then(Value::as_str)
                .expect("target fixture cid"),
            FIXTURE_STATE_CID
        );
        assert_eq!(
            source.pointer("/measurements/row_schema"),
            target.pointer("/measurements/row_schema"),
            "row schema must be byte-equal across language witnesses"
        );
    }
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
