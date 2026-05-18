// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{IrFormula, MigrateReceiptEnvelope};
use serde_json::json;

const INSERT_AND_GET_ID_CONCEPT_CID: &str = "blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca";

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

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

fn write_source_project(root: &Path, source: &str) -> PathBuf {
    let source_dir = root.join("users-better-sqlite3");
    let src_dir = source_dir.join("src");
    fs::create_dir_all(&src_dir).expect("create source dir");
    fs::write(src_dir.join("users.ts"), source).expect("write users.ts");
    source_dir
}

fn run_migrate(
    source_dir: &Path,
    library_to: &str,
    receipt: &Path,
    focus: Option<&str>,
) -> std::process::Output {
    let repo = repo_root();
    let out_dir = receipt
        .parent()
        .expect("receipt has parent")
        .join(format!("out-{library_to}"));
    let mut cmd = Command::new(provekit_bin());
    cmd.current_dir(&repo)
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
        .arg(receipt);
    if let Some(focus) = focus {
        cmd.arg("--focus").arg(focus);
    }
    cmd.output().expect("spawn provekit bind migrate")
}

fn assert_success(label: &str, output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn read_receipt(path: &Path) -> MigrateReceiptEnvelope {
    let raw = fs::read_to_string(path).expect("receipt written");
    let receipt: MigrateReceiptEnvelope = serde_json::from_str(&raw).expect("parse receipt");
    receipt.validate().expect("receipt validates");
    receipt
}

fn receipt_for(source: &str, library_to: &str, focus: Option<&str>) -> MigrateReceiptEnvelope {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_dir = write_source_project(temp.path(), source);
    let receipt_path = temp.path().join("migrate-receipt.proof");
    let output = run_migrate(&source_dir, library_to, &receipt_path, focus);
    assert_success("migrate", &output);
    read_receipt(&receipt_path)
}

fn receipt_cid_for(source: &str, library_to: &str, focus: Option<&str>) -> String {
    receipt_for(source, library_to, focus).root_cid
}

fn insert_source() -> String {
    [
        r#"import Database from "better-sqlite3";"#,
        "",
        r#"const db = new Database("users.sqlite");"#,
        "",
        "export function recordEvent(userId: number, kind: string): number {",
        "  const result = db",
        r#"    .prepare("INSERT INTO events (user_id, kind) VALUES (?, ?)")"#,
        "    .run(userId, kind);",
        "  return Number(result.lastInsertRowid);",
        "}",
        "",
    ]
    .join("\n")
}

fn two_insert_source() -> String {
    [
        r#"import Database from "better-sqlite3";"#,
        "",
        r#"const db = new Database("users.sqlite");"#,
        "",
        "export function recordEvent(userId: number, kind: string): number {",
        "  const result = db",
        r#"    .prepare("INSERT INTO events (user_id, kind) VALUES (?, ?) /* first */")"#,
        "    .run(userId, kind);",
        "  return Number(result.lastInsertRowid);",
        "}",
        "",
        "export function recordEvent(userId: number, kind: string): number {",
        "  const result = db",
        r#"    .prepare("INSERT INTO events (user_id, kind) VALUES (?, ?) /* second */")"#,
        "    .run(userId, kind);",
        "  return Number(result.lastInsertRowid);",
        "}",
        "",
    ]
    .join("\n")
}

fn query_only_source() -> String {
    [
        r#"import Database from "better-sqlite3";"#,
        "",
        r#"const db = new Database("users.sqlite");"#,
        "",
        "export function countUsers(): number {",
        r#"  const row = db.prepare("SELECT count(*) AS count FROM users").get() as { count: number };"#,
        "  return row.count;",
        "}",
        "",
    ]
    .join("\n")
}

fn callsite_cid(source: &str, concept: &str, function: &str, sql_needle: &str) -> String {
    let offset = source.find(sql_needle).expect("SQL callsite exists");
    let line = source[..offset].chars().filter(|ch| *ch == '\n').count() + 1;
    cid_for_json(&json!({
        "concept": concept,
        "function": function,
        "kind": "migration-sql-callsite",
        "line": line
    }))
}

fn cid_for_json(value: &serde_json::Value) -> String {
    blake3_512_of(encode_jcs(&json_to_value(value)).as_bytes())
}

fn json_to_value(j: &serde_json::Value) -> Arc<Value> {
    match j {
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(i64::try_from(u).unwrap_or(i64::MAX))
            } else {
                Value::string(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Array(arr) => Value::array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => {
            let kv: Vec<(String, Arc<Value>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::object(kv)
        }
    }
}

#[test]
fn fixture_1_documents_sql_migrate_narrowing_gap_with_supported_divergence() {
    let source = insert_source();
    let receipt = receipt_for(&source, "typescript-pg", None);

    assert_eq!(receipt.loss_records.len(), 1);
    assert_eq!(receipt.loss_records[0].loss_dimension, "RowIdMechanism");
    assert!(receipt.root_cid.starts_with("blake3-512:"));
}

#[test]
fn fixture_2_row_id_mechanism_divergence_has_a18_shape() {
    let source = insert_source();
    let receipt = receipt_for(&source, "typescript-pg", None);
    let expected_callsite = callsite_cid(
        &source,
        "concept:insert-and-get-id",
        "recordEvent",
        ".prepare(\"INSERT INTO events",
    );
    let loss = receipt
        .loss_records
        .iter()
        .find(|loss| loss.loss_dimension == "RowIdMechanism")
        .expect("RowIdMechanism loss record");

    assert!(receipt.aggregate_summary.lossy >= 1);
    assert!(matches!(
        loss.loss_dimensions.get("RowIdMechanism"),
        Some(IrFormula::DivergenceBetween { source, target })
            if matches!(source.as_ref(), IrFormula::Atomic { args, .. } if !args.is_empty())
                && matches!(target.as_ref(), IrFormula::Atomic { args, .. } if !args.is_empty())
    ));
    assert_eq!(loss.callsite_cid, expected_callsite);
}

#[test]
fn fixture_3_focus_scopes_point_query_loss_records() {
    let source = two_insert_source();
    let first_callsite = callsite_cid(
        &source,
        "concept:insert-and-get-id",
        "recordEvent",
        ".prepare(\"INSERT INTO events (user_id, kind) VALUES (?, ?) /* first */",
    );
    let second_callsite = callsite_cid(
        &source,
        "concept:insert-and-get-id",
        "recordEvent",
        ".prepare(\"INSERT INTO events (user_id, kind) VALUES (?, ?) /* second */",
    );

    let first = receipt_for(&source, "typescript-pg", Some(&first_callsite));
    let second = receipt_for(&source, "typescript-pg", Some(&second_callsite));

    assert_eq!(first.loss_records.len(), 1);
    assert_eq!(first.loss_records[0].callsite_cid, first_callsite);
    assert_eq!(second.loss_records.len(), 1);
    assert_eq!(second.loss_records[0].callsite_cid, second_callsite);
}

#[test]
fn fixture_4_exact_no_op_leg_emits_zero_loss_records() {
    let source = query_only_source();
    let receipt = receipt_for(&source, "python-sqlite3", None);

    assert_eq!(receipt.aggregate_summary.lossy, 0);
    assert_eq!(receipt.loss_records.len(), 0);
    assert_eq!(receipt.aggregate_summary.refused, 0);
}

#[test]
fn fixture_5_uncharacterizable_insert_routes_to_refusal_without_loss() {
    let source = insert_source();
    let receipt = receipt_for(&source, "python-sqlite3", None);

    assert!(receipt.aggregate_summary.refused >= 1);
    assert!(receipt
        .refusal_mementos
        .iter()
        .any(|refusal| refusal.reason.contains(INSERT_AND_GET_ID_CONCEPT_CID)));
    assert_eq!(receipt.aggregate_summary.lossy, 0);
}

#[test]
fn fixture_6_point_query_receipt_cids_are_byte_stable() {
    let fixture_2_source = insert_source();
    let fixture_3_source = two_insert_source();
    let fixture_3_focus = callsite_cid(
        &fixture_3_source,
        "concept:insert-and-get-id",
        "recordEvent",
        ".prepare(\"INSERT INTO events (user_id, kind) VALUES (?, ?) /* second */",
    );
    let fixture_4_source = query_only_source();

    assert_eq!(
        receipt_cid_for(&fixture_2_source, "typescript-pg", None),
        receipt_cid_for(&fixture_2_source, "typescript-pg", None)
    );
    assert_eq!(
        receipt_cid_for(&fixture_3_source, "typescript-pg", Some(&fixture_3_focus)),
        receipt_cid_for(&fixture_3_source, "typescript-pg", Some(&fixture_3_focus))
    );
    assert_eq!(
        receipt_cid_for(&fixture_4_source, "python-sqlite3", None),
        receipt_cid_for(&fixture_4_source, "python-sqlite3", None)
    );
}
