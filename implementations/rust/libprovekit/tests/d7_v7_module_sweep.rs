// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use libprovekit::canonical::{json_cid, serializable_jcs};
use libprovekit::proofir_bridge::{CatalogIndex, ResolvedNode, ResolvedTerm};
use libprovekit::proofir_resolve;
use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::Term;
use serde_json::{json, Value as JsonValue};

struct MethodCase {
    method: &'static str,
    fixture_file: &'static str,
    receipt_file: &'static str,
    original_needle: &'static str,
    realize_function: &'static str,
    params: &'static [&'static str],
    param_types: &'static [&'static str],
    return_type: &'static str,
}

const CASES: &[MethodCase] = &[
    MethodCase {
        method: "null",
        fixture_file: "d7_v7_value_null.json",
        receipt_file: "value_null_source_round_trip_receipt.json",
        original_needle: "pub fn null(",
        realize_function: "null",
        params: &[],
        param_types: &[],
        return_type: "Arc < Value >",
    },
    MethodCase {
        method: "boolean",
        fixture_file: "d7_v7_value_boolean.json",
        receipt_file: "value_boolean_source_round_trip_receipt.json",
        original_needle: "pub fn boolean(",
        realize_function: "boolean",
        params: &["b"],
        param_types: &["bool"],
        return_type: "Arc < Value >",
    },
    MethodCase {
        method: "integer",
        fixture_file: "d7_v7_value_integer.json",
        receipt_file: "value_integer_source_round_trip_receipt.json",
        original_needle: "pub fn integer(",
        realize_function: "integer",
        params: &["n"],
        param_types: &["i64"],
        return_type: "Arc < Value >",
    },
    MethodCase {
        method: "string",
        fixture_file: "d7_v7_value_string.json",
        receipt_file: "value_string_source_round_trip_receipt.json",
        original_needle: "pub fn string<",
        realize_function: "string<S: Into<String>>",
        params: &["s"],
        param_types: &["S"],
        return_type: "Arc < Value >",
    },
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn fixture_path(repo_root: &Path, case: &MethodCase) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("libprovekit")
        .join("tests")
        .join("fixtures")
        .join("proofir")
        .join(case.fixture_file)
}

fn source_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("provekit-canonicalizer")
        .join("src")
        .join("value.rs")
}

fn receipt_path(repo_root: &Path, case: &MethodCase) -> PathBuf {
    repo_root
        .join("bootstrap")
        .join("D7-v7")
        .join(case.receipt_file)
}

fn scratch_path(repo_root: &Path, case: &MethodCase, name: &str) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("target")
        .join("d7-v7")
        .join(case.method)
        .join(name)
}

fn resolved_sort(name: &str) -> JsonValue {
    json!({
        "args": [],
        "kind": "ctor",
        "name": name,
    })
}

fn fixture_op_cid(fixture: &JsonValue, name: &str) -> String {
    fixture["proofir_catalog_ops"]
        .as_array()
        .expect("proofir_catalog_ops array")
        .iter()
        .find(|op| op["name"] == name)
        .and_then(|op| op["op_cid"].as_str())
        .unwrap_or_else(|| panic!("fixture has op cid for {name}"))
        .to_string()
}

fn op_name_for_cid(fixture: &JsonValue, cid: &str) -> Option<String> {
    fixture["proofir_catalog_ops"]
        .as_array()?
        .iter()
        .find(|op| op["op_cid"] == cid)
        .and_then(|op| op["name"].as_str())
        .map(str::to_string)
}

fn value_constructor_catalog(fixture: &JsonValue) -> CatalogIndex {
    let mut catalog = CatalogIndex::new();
    catalog.insert_op(
        "return",
        fixture_op_cid(fixture, "return"),
        Some(vec![resolved_sort("Expr")]),
        Some(resolved_sort("Stmt")),
    );
    catalog.insert_op(
        "call:new",
        fixture_op_cid(fixture, "call:new"),
        Some(vec![
            resolved_sort("FnContract"),
            resolved_sort("ListOfExpr"),
        ]),
        Some(resolved_sort("Expr")),
    );
    catalog
}

fn root_concept_name(fixture: &JsonValue, resolved: &ResolvedTerm) -> String {
    match &resolved.node {
        ResolvedNode::OpApplication {
            op_definition_cid, ..
        } => op_name_for_cid(fixture, op_definition_cid)
            .unwrap_or_else(|| op_definition_cid.to_string()),
        ResolvedNode::Literal { .. } => "literal".to_string(),
    }
}

fn term_summary(fixture: &JsonValue, resolved: &ResolvedTerm) -> String {
    fn inner(fixture: &JsonValue, term: &ResolvedTerm) -> String {
        match &term.node {
            ResolvedNode::Literal { value } => format!("literal({value})"),
            ResolvedNode::OpApplication {
                op_definition_cid,
                args,
            } => {
                let name = op_name_for_cid(fixture, op_definition_cid)
                    .unwrap_or_else(|| op_definition_cid.to_string());
                let args = args
                    .iter()
                    .map(|arg| inner(fixture, arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({args})")
            }
        }
    }

    format!("{} -> sort {}", inner(fixture, resolved), resolved.sort)
}

/// Extract the source visibility (`pub`, `pub(crate)`, ... or `""`) preceding
/// the `fn` keyword in a function source slice, so the realizer reproduces it
/// on emit instead of defaulting to a private `fn`.
fn visibility_of_fn_slice(slice: &str) -> String {
    let fn_index = slice.find("fn ").expect("slice must contain `fn `");
    let prefix = slice[..fn_index].trim();
    if let Some(rest) = prefix.strip_prefix("pub") {
        let rest = rest.trim();
        if rest.starts_with('(') {
            format!("pub{rest}")
        } else {
            "pub".to_string()
        }
    } else {
        String::new()
    }
}

fn extract_method_slice(source: &str, needle: &str) -> String {
    let method = source.find(needle).expect("find target Value method");
    let start = source[..method]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let open_brace = source[method..]
        .find('{')
        .map(|index| method + index)
        .expect("find method body");

    let mut depth = 0usize;
    for (offset, ch) in source[open_brace..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let mut end = open_brace + offset + ch.len_utf8();
                    if source[end..].starts_with('\n') {
                        end += 1;
                    }
                    return source[start..end].to_string();
                }
            }
            _ => {}
        }
    }

    panic!("unterminated target Value method body");
}

fn rustfmt_config(repo_root: &Path) -> Option<PathBuf> {
    ["rustfmt.toml", ".rustfmt.toml"]
        .iter()
        .map(|name| repo_root.join(name))
        .find(|path| path.is_file())
}

fn rustfmt_command_text(config: Option<&Path>, input_label: &str) -> String {
    let mut parts = vec!["rustfmt".to_string(), "--edition 2021".to_string()];
    if let Some(config) = config {
        parts.push(format!("--config-path {}", config.display()));
    }
    parts.push("--emit stdout".to_string());
    parts.push(format!("<stdin:{input_label}>"));
    parts.join(" ")
}

fn rustfmt_source(repo_root: &Path, input_label: &str, source: &str) -> (String, String) {
    let config = rustfmt_config(repo_root);
    let mut command = Command::new("rustfmt");
    command.arg("--edition").arg("2021");
    if let Some(config) = &config {
        command.arg("--config-path").arg(config);
    }
    let mut child = command
        .arg("--emit")
        .arg("stdout")
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rustfmt");
    child
        .stdin
        .as_mut()
        .expect("rustfmt stdin")
        .write_all(source.as_bytes())
        .expect("write rustfmt stdin");
    let output = child.wait_with_output().expect("run rustfmt");

    if !output.status.success() {
        panic!(
            "rustfmt failed for {input_label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    (
        String::from_utf8(output.stdout).expect("rustfmt stdout utf8"),
        rustfmt_command_text(config.as_deref(), input_label),
    )
}

fn unified_diff(repo_root: &Path, case: &MethodCase, original: &str, regenerated: &str) -> String {
    let original_path = scratch_path(repo_root, case, "original.rustfmt.rs");
    let regenerated_path = scratch_path(repo_root, case, "regenerated.rustfmt.rs");
    std::fs::create_dir_all(original_path.parent().expect("diff scratch parent"))
        .expect("create diff scratch dir");
    std::fs::write(&original_path, original).expect("write original rustfmt text");
    std::fs::write(&regenerated_path, regenerated).expect("write regenerated rustfmt text");

    let output = Command::new("diff")
        .arg("--label")
        .arg("original_rustfmt")
        .arg("--label")
        .arg("regenerated_rustfmt")
        .arg("-u")
        .arg(&original_path)
        .arg(&regenerated_path)
        .output()
        .expect("run diff -u");

    match output.status.code() {
        Some(0) => String::new(),
        Some(1) => String::from_utf8(output.stdout).expect("diff stdout utf8"),
        _ => panic!("diff failed: {}", String::from_utf8_lossy(&output.stderr)),
    }
}

fn diff_hunks(diff: &str) -> Vec<String> {
    let mut hunks = Vec::new();
    let mut current = Vec::new();

    for line in diff.lines() {
        if line.starts_with("@@") {
            if !current.is_empty() {
                hunks.push(current.join("\n"));
                current.clear();
            }
            current.push(line.to_string());
        } else if !current.is_empty() {
            current.push(line.to_string());
        }
    }

    if !current.is_empty() {
        hunks.push(current.join("\n"));
    }

    hunks
}

fn classify_diff(diff: &str, realization_is_stub: bool, concept_name: &str) -> Vec<JsonValue> {
    diff_hunks(diff)
        .into_iter()
        .map(|hunk| {
            if realization_is_stub && hunk.contains("provekit-bind canonical:") {
                json!({
                    "hunk": hunk,
                    "class": "stub-body",
                    "d7_debt_ticket": "#964",
                    "explanation": format!(
                        "provekit-realize-rust-core emitted its stub body because the existing D7 resolved body lowering does not consume this Value module surface; the realized fallback concept is `{concept_name}`."
                    ),
                })
            } else {
                json!({
                    "hunk": hunk,
                    "class": "structural-difference",
                    "d7_debt_ticket": "new-sub-issue-required",
                    "explanation": "post-rustfmt text differs in AST shape rather than byte-only spelling.",
                })
            }
        })
        .collect()
}

fn dominant_diff_class(byte_identical: bool, classification: &[JsonValue]) -> String {
    if byte_identical {
        "byte-identical".to_string()
    } else {
        classification
            .iter()
            .find_map(|item| item["class"].as_str())
            .unwrap_or("unclassified")
            .to_string()
    }
}

fn stub_concept_name(source: &str, fallback: &str) -> String {
    let marker = "provekit-bind canonical: ";
    source
        .find(marker)
        .and_then(|start| {
            let value_start = start + marker.len();
            source[value_start..]
                .find('"')
                .map(|end| source[value_start..value_start + end].to_string())
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn run_case(repo_root: &Path, case: &MethodCase) -> JsonValue {
    let fixture_text =
        std::fs::read_to_string(fixture_path(repo_root, case)).expect("read D7-v7 fixture");
    let fixture: JsonValue = serde_json::from_str(&fixture_text).expect("parse D7-v7 fixture");
    let fixture_cid = json_cid(&fixture).expect("fixture CID");

    let term: Term =
        serde_json::from_value(fixture["proofir_term"].clone()).expect("decode ProofIR term");
    let catalog = value_constructor_catalog(&fixture);
    let resolved = proofir_resolve(&term, &catalog).expect("resolve D7-v7 ProofIR term");
    let resolved_jcs = serializable_jcs(&resolved).expect("ResolvedTerm JCS");
    let resolved_summary = term_summary(&fixture, &resolved);

    let params: Vec<String> = case
        .params
        .iter()
        .map(|param| (*param).to_string())
        .collect();
    let param_types: Vec<String> = case
        .param_types
        .iter()
        .map(|ty| (*ty).to_string())
        .collect();
    let concept_name = root_concept_name(&fixture, &resolved);

    let source_text =
        std::fs::read_to_string(source_path(repo_root)).expect("read canonicalizer value.rs");
    let original_slice = extract_method_slice(&source_text, case.original_needle);
    let original_slice_cid = blake3_512_of(original_slice.as_bytes());
    let source_visibility = visibility_of_fn_slice(&original_slice);
    let (original_rustfmt, original_rustfmt_command) = rustfmt_source(
        repo_root,
        &format!("value_{}_original.rs", case.method),
        &original_slice,
    );

    let realization = provekit_realize_rust_core::emit_from_resolved_with_visibility(
        &resolved_jcs,
        case.realize_function,
        &params,
        &param_types,
        case.return_type,
        &source_visibility,
    );
    let (regenerated_rustfmt, regenerated_rustfmt_command) = rustfmt_source(
        repo_root,
        &format!("value_{}_regenerated.rs", case.method),
        &realization.source,
    );
    let byte_identical = regenerated_rustfmt.as_bytes() == original_rustfmt.as_bytes();
    let diff = unified_diff(repo_root, case, &original_rustfmt, &regenerated_rustfmt);
    let diff_concept_name = if realization.is_stub {
        stub_concept_name(&realization.source, &concept_name)
    } else {
        concept_name
    };
    let classification = classify_diff(&diff, realization.is_stub, &diff_concept_name);
    let verdict = if byte_identical {
        "BYTE_IDENTICAL"
    } else {
        "CHARACTERIZED_DIFF"
    };
    let dominant_diff_class = dominant_diff_class(byte_identical, &classification);
    let regenerated_source_cid = blake3_512_of(regenerated_rustfmt.as_bytes());
    let original_slice_post_rustfmt_cid = blake3_512_of(original_rustfmt.as_bytes());

    if !byte_identical {
        assert!(
            !classification.is_empty(),
            "non-identical source must have a classified diff hunk for {}",
            case.method
        );
    }

    json!({
        "version": "1",
        "target": {
            "crate": "provekit-canonicalizer",
            "function": format!("impl Value::{}", case.method),
            "source_path": "implementations/rust/provekit-canonicalizer/src/value.rs",
        },
        "lift": {
            "tool": "provekit-walk",
            "command": format!(
                "cd implementations/rust && target/debug/provekit-walk-emit term provekit-canonicalizer/src/value.rs {} /private/tmp/d7-v7-walk/{}.raw.json",
                case.method, case.method
            ),
            "fixture_path": format!(
                "implementations/rust/libprovekit/tests/fixtures/proofir/{}",
                case.fixture_file
            ),
            "fixture_cid": fixture_cid,
            "outcome": fixture["handling"].clone(),
            "term_surface": fixture["term_surface"].clone(),
            "loss_record": fixture["loss_record"].clone(),
        },
        "pipeline": {
            "step_1_fixture_cid": fixture_cid,
            "step_2_resolve": format!("summary={resolved_summary}; jcs={resolved_jcs}"),
            "step_3_4_realize_command": format!(
                "provekit_realize_rust_core::emit_from_resolved(<resolved-term-jcs>, {:?}, {:?}, {:?}, {:?})",
                case.realize_function, params, param_types, case.return_type
            ),
            "step_3_4_realize_input": {
                "resolved_term_json": resolved_jcs,
                "function": case.realize_function,
                "params": params,
                "param_types": param_types,
                "return_type": case.return_type,
            },
            "step_3_4_regenerated_source": realization.source,
            "step_5_original_slice_cid": original_slice_cid,
            "step_6_rustfmt_command": format!("{regenerated_rustfmt_command}\n{original_rustfmt_command}"),
            "step_7_byte_identical_post_rustfmt": byte_identical,
            "step_8_unified_diff": diff,
            "step_9_diff_classification": classification,
            "step_10_dominant_diff_class": dominant_diff_class.clone(),
            "step_11_expected_diff_shape": "BYTE_IDENTICAL required for every D7-v7 in-scope value.rs function",
            "step_12_empirical_root_cause": if byte_identical {
                "The D7-v6 resolved body lowering emits this in-scope Value constructor body byte-for-byte after rustfmt.".to_string()
            } else {
                "The existing D7-v6 resolved body lowering does not consume this in-scope Value module surface; record the gap and retire it after v7.".to_string()
            },
            "regenerated_source_cid": regenerated_source_cid,
            "original_slice_post_rustfmt_cid": original_slice_post_rustfmt_cid,
        },
        "verdict": verdict,
        "dominant_diff_class": dominant_diff_class,
        "next_action": if byte_identical {
            "No per-function D7 debt remains for this in-scope Value constructor.".to_string()
        } else {
            "Retire the recorded D7 debt class in a follow-up before claiming module terminus.".to_string()
        },
    })
}

#[test]
fn value_module_sweep_receipts_match_pipeline() {
    let repo_root = repo_root();
    for case in CASES {
        let receipt = run_case(&repo_root, case);
        let verdict = receipt["verdict"].as_str().expect("receipt verdict");
        assert_eq!(
            verdict, "BYTE_IDENTICAL",
            "D7-v7 in-scope function {} must be BYTE_IDENTICAL",
            case.method
        );

        if let Ok(out_dir) = std::env::var("D7_V7_RECEIPT_OUT_DIR") {
            let path = PathBuf::from(out_dir).join(case.receipt_file);
            std::fs::create_dir_all(path.parent().expect("receipt parent"))
                .expect("create generated D7-v7 receipt dir");
            std::fs::write(
                &path,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&receipt).expect("pretty receipt")
                ),
            )
            .expect("write generated D7-v7 receipt");
            println!(
                "d7_v7_{} verdict={} fixture_cid={} generated_receipt={}",
                case.method,
                verdict,
                receipt["pipeline"]["step_1_fixture_cid"]
                    .as_str()
                    .expect("fixture cid"),
                path.display()
            );
            continue;
        }

        let path = receipt_path(&repo_root, case);
        let committed_text = std::fs::read_to_string(&path).expect("read committed D7-v7 receipt");
        let committed: JsonValue =
            serde_json::from_str(&committed_text).expect("parse committed D7-v7 receipt");
        assert_eq!(
            committed, receipt,
            "committed D7-v7 receipt drifted for {}",
            case.method
        );
        println!(
            "d7_v7_{} verdict={} fixture_cid={} receipt={}",
            case.method,
            verdict,
            receipt["pipeline"]["step_1_fixture_cid"]
                .as_str()
                .expect("fixture cid"),
            path.display()
        );
    }
}
