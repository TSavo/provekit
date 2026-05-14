// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use provekit_ir_types::{MigrateReceiptEnvelope, WitnessMemento};
use serde_json::{json, Value};

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
    source_dir: &Path,
    out_dir: &Path,
    receipt: &Path,
    fixture: &Path,
) -> std::process::Output {
    let target = tempfile::tempdir().expect("fresh target dir");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
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
        .arg("typescript-pg")
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

fn build_fixture(path: &Path) {
    run_sqlite_script(path, FIXTURE_DDL);
    run_sqlite_script(
        path,
        r#"
INSERT INTO users (id, name, email) VALUES
  (1, 'Ada Lovelace', 'ada@example.test'),
  (2, 'Grace Hopper', 'grace@example.test'),
  (3, 'Katherine Johnson', 'katherine@example.test'),
  (4, 'Edsger Dijkstra', 'edsger@example.test'),
  (5, 'Barbara Liskov', 'barbara@example.test');

INSERT INTO events (user_id, kind) VALUES
  (1, 'login'),
  (2, 'view'),
  (1, 'logout');
"#,
    );
}

#[test]
fn migrate_receipt_emits_verifiable_row_shape_witnesses() {
    let repo = repo_root();
    let source_dir = repo
        .join("examples")
        .join("migrate-demo")
        .join("users-better-sqlite3");
    let temp = tempfile::tempdir().expect("temp output");
    let out_dir = temp.path().join("users-pg");
    let receipt_path = temp.path().join("migrate-receipt.proof");
    let fixture_path = temp.path().join("fixture.sqlite");
    build_fixture(&fixture_path);

    let output = run_migrate(&repo, &source_dir, &out_dir, &receipt_path, &fixture_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "migrate bind failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let receipt_raw = fs::read_to_string(&receipt_path).expect("receipt written");
    let receipt: MigrateReceiptEnvelope =
        serde_json::from_str(&receipt_raw).expect("parse migrate receipt");
    receipt.validate().expect("receipt validates");

    assert!(
        receipt.witnesses.len() >= 4,
        "expected one witness per concept site, got {}",
        receipt.witnesses.len()
    );
    assert!(
        receipt
            .witnesses
            .iter()
            .all(|witness| witness.outcome == "pass"),
        "all witnesses must pass"
    );

    let get_user_witness = receipt
        .witnesses
        .iter()
        .find(|witness| row_schema_has_user_columns(&witness.measurements))
        .expect("getUserById row-shape witness present");
    assert_eq!(
        get_user_witness
            .measurements
            .pointer("/row_schema/columns")
            .expect("columns present"),
        &json!([
            {"name": "id", "declared_type": "INTEGER", "observed_typeof": "integer"},
            {"name": "name", "declared_type": "TEXT", "observed_typeof": "text"},
            {"name": "email", "declared_type": "TEXT", "observed_typeof": "text"}
        ])
    );

    for witness in &receipt.witnesses {
        assert_eq!(
            witness.recompute_cid().expect("recompute witness cid"),
            witness.cid,
            "witness cid must match self-CID"
        );
        let redischarged = redischarge_measurements(&fixture_path, witness);
        assert_eq!(
            redischarged, witness.measurements,
            "re-discharge must be byte-equal for subject {}",
            witness.subject
        );
    }
}

fn row_schema_has_user_columns(measurements: &Value) -> bool {
    measurements
        .pointer("/row_schema/columns")
        .and_then(Value::as_array)
        .map(|columns| {
            *columns
                == [
                    json!({"name": "id", "declared_type": "INTEGER", "observed_typeof": "integer"}),
                    json!({"name": "name", "declared_type": "TEXT", "observed_typeof": "text"}),
                    json!({"name": "email", "declared_type": "TEXT", "observed_typeof": "text"}),
                ]
        })
        .unwrap_or(false)
}

fn redischarge_measurements(fixture: &Path, witness: &WitnessMemento) -> Value {
    let query = witness
        .measurements
        .get("query")
        .expect("query metadata present");
    let sql = query
        .get("sql")
        .and_then(Value::as_str)
        .expect("query sql present");
    let sample_args = query
        .get("sample_args")
        .and_then(Value::as_array)
        .expect("query sample args present");
    observe_sql(fixture, sql, sample_args)
}

fn observe_sql(fixture: &Path, sql: &str, sample_args: &[Value]) -> Value {
    let expanded_sql = substitute_sql_args(sql, sample_args);
    let names = select_column_names(sql);
    if names.is_empty() {
        run_sqlite(fixture, &format!("EXPLAIN {expanded_sql}"));
        return json!({
            "query": {"sql": sql, "sample_args": sample_args},
            "row_schema": {"columns": []},
            "sample_row": {}
        });
    }

    let rows = run_sqlite_json(fixture, &expanded_sql);
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(Value::as_object)
        .expect("sample row present");
    let declared_types = declared_types(fixture, sql, &names);
    let mut columns = Vec::new();
    let mut sample_row = serde_json::Map::new();
    for (idx, name) in names.iter().enumerate() {
        let value = row.get(name).expect("read sqlite column value").clone();
        columns.push(json!({
            "name": name,
            "declared_type": declared_types[idx],
            "observed_typeof": sqlite_json_typeof(&value)
        }));
        sample_row.insert(name.clone(), value);
    }

    json!({
        "query": {"sql": sql, "sample_args": sample_args},
        "row_schema": {"columns": columns},
        "sample_row": Value::Object(sample_row)
    })
}

fn declared_types(fixture: &Path, sql: &str, names: &[String]) -> Vec<String> {
    let Some(table) = table_name(sql) else {
        return names.iter().map(|_| "UNKNOWN".to_string()).collect();
    };
    let rows = run_sqlite_json(fixture, &format!("PRAGMA table_info({table})"));
    let rows = rows.as_array().expect("table_info rows");
    let mut by_name = std::collections::BTreeMap::new();
    for row in rows {
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .expect("table_info name");
        let declared_type = row
            .get("type")
            .and_then(Value::as_str)
            .expect("table_info type");
        by_name.insert(name.to_string(), declared_type.to_string());
    }
    names
        .iter()
        .map(|name| {
            by_name
                .get(name)
                .cloned()
                .unwrap_or_else(|| expression_declared_type(name))
        })
        .collect()
}

fn run_sqlite_script(path: &Path, script: &str) {
    let mut child = Command::new("sqlite3")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sqlite3");
    child
        .stdin
        .as_mut()
        .expect("sqlite3 stdin")
        .write_all(script.as_bytes())
        .expect("write sqlite3 script");
    let output = child.wait_with_output().expect("wait sqlite3");
    assert!(
        output.status.success(),
        "sqlite3 script failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_sqlite(fixture: &Path, sql: &str) -> String {
    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg(fixture)
        .arg(sql)
        .output()
        .expect("spawn sqlite3 readonly");
    assert!(
        output.status.success(),
        "sqlite3 failed for {sql}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("sqlite3 output UTF-8")
}

fn run_sqlite_json(fixture: &Path, sql: &str) -> Value {
    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg("-json")
        .arg(fixture)
        .arg(sql)
        .output()
        .expect("spawn sqlite3 readonly json");
    assert!(
        output.status.success(),
        "sqlite3 json failed for {sql}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("sqlite3 json output UTF-8");
    if text.trim().is_empty() {
        json!([])
    } else {
        serde_json::from_str(&text).expect("parse sqlite3 json")
    }
}

fn select_column_names(sql: &str) -> Vec<String> {
    let lower = sql.to_ascii_lowercase();
    let Some(select_pos) = lower.find("select ") else {
        return Vec::new();
    };
    let Some(from_pos) = lower.find(" from ") else {
        return Vec::new();
    };
    let projection = &sql[select_pos + "select ".len()..from_pos];
    split_top_level_commas(projection)
        .into_iter()
        .map(column_name_for_projection)
        .collect()
}

fn column_name_for_projection(projection: &str) -> String {
    let tokens = projection.split_whitespace().collect::<Vec<_>>();
    for idx in 0..tokens.len() {
        if tokens[idx].eq_ignore_ascii_case("as") && idx + 1 < tokens.len() {
            return clean_identifier(tokens[idx + 1]);
        }
    }
    clean_identifier(tokens.last().copied().unwrap_or(projection))
}

fn split_top_level_commas(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in args.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(current_quote) = quote {
            if ch == current_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            continue;
        }
        if ch == '(' || ch == '[' || ch == '{' {
            depth += 1;
            continue;
        }
        if ch == ')' || ch == ']' || ch == '}' {
            depth = depth.saturating_sub(1);
            continue;
        }
        if ch == ',' && depth == 0 {
            out.push(args[start..idx].trim());
            start = idx + 1;
        }
    }
    out.push(args[start..].trim());
    out
}

fn clean_identifier(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| ch == '"' || ch == '`' || ch == '[' || ch == ']')
        .to_string()
}

fn table_name(sql: &str) -> Option<String> {
    let lower = sql.to_ascii_lowercase();
    let from_pos = lower.find(" from ")?;
    let after_from = &sql[from_pos + " from ".len()..];
    after_from.split_whitespace().next().map(|table| {
        table
            .trim_matches(|ch: char| ch == '"' || ch == '`')
            .to_string()
    })
}

fn expression_declared_type(name: &str) -> String {
    if name.eq_ignore_ascii_case("count") || name.eq_ignore_ascii_case("id") {
        "INTEGER".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

fn substitute_sql_args(sql: &str, sample_args: &[Value]) -> String {
    let mut out = String::new();
    let mut arg_idx = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for ch in sql.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            out.push(ch);
            escaped = true;
            continue;
        }
        if let Some(current_quote) = quote {
            out.push(ch);
            if ch == current_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            out.push(ch);
            quote = Some(ch);
            continue;
        }
        if ch == '?' {
            out.push_str(&sql_literal(
                sample_args.get(arg_idx).expect("sample arg exists"),
            ));
            arg_idx += 1;
        } else {
            out.push(ch);
        }
    }
    assert_eq!(arg_idx, sample_args.len(), "all sample args used");
    out
}

fn sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(value) => {
            if *value {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("'{}'", value.replace('\'', "''")),
        Value::Array(_) | Value::Object(_) => panic!("sample args must be scalar"),
    }
}

fn sqlite_json_typeof(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "integer",
        Value::Number(value) => {
            if value.as_i64().is_some() || value.as_u64().is_some() {
                "integer"
            } else {
                "real"
            }
        }
        Value::String(_) => "text",
        Value::Array(_) | Value::Object(_) => "blob",
    }
}

const FIXTURE_DDL: &str = r#"
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL
);

CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL,
  kind TEXT NOT NULL
);
"#;
