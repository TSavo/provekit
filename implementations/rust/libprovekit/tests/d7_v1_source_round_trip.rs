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

const EXPECTED_FIXTURE_CID: &str = "blake3-512:bcb10be48ad632abc71c406355b6d11b0191a959b523aa755ee00ad7496afa2270ce28821af4abcd5949427026fb16d8d8b38af702b1810dec3bdff810ec8f32";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn fixture_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("libprovekit")
        .join("tests")
        .join("fixtures")
        .join("proofir")
        .join("d7_v0_value_null.json")
}

fn source_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("provekit-canonicalizer")
        .join("src")
        .join("value.rs")
}

fn receipt_path(repo_root: &Path, phase: &str) -> PathBuf {
    repo_root
        .join("bootstrap")
        .join(phase)
        .join("value_null_source_round_trip_receipt.json")
}

fn scratch_path(repo_root: &Path, name: &str) -> PathBuf {
    repo_root
        .join("implementations")
        .join("rust")
        .join("target")
        .join("d7-v1")
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

fn value_null_catalog(fixture: &JsonValue) -> CatalogIndex {
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

fn function_name_from_target(fixture: &JsonValue) -> String {
    fixture["target"]
        .as_str()
        .expect("target string")
        .rsplit("::")
        .next()
        .expect("method name")
        .to_string()
}

fn return_type_from_loss_record(fixture: &JsonValue) -> String {
    fixture["loss_record"]
        .as_array()
        .expect("loss_record array")
        .iter()
        .find(|loss| loss["loss"] == "return-type-user-defined")
        .and_then(|loss| loss["detail"].as_str())
        .expect("return-type-user-defined detail")
        .to_string()
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

fn extract_value_null_slice(source: &str) -> String {
    let needle = "pub fn null(";
    let method = source.find(needle).expect("find Value::null method");
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

    panic!("unterminated Value::null body");
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

fn unified_diff(repo_root: &Path, original: &str, regenerated: &str) -> String {
    let original_path = scratch_path(repo_root, "value_null_original.rustfmt.rs");
    let regenerated_path = scratch_path(repo_root, "value_null_regenerated.rustfmt.rs");
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
                    "explanation": format!(
                        "provekit-realize-rust-core emitted its stub body because no body template matched the extracted root concept `{concept_name}`; the current flat API cannot consume the nested resolved body tree."
                    ),
                })
            } else if hunk.contains("-    Arc::new(Value::Null)")
                && hunk.contains("+    new(Value::Null)")
            {
                json!({
                    "hunk": hunk,
                    "class": "name-difference",
                    "explanation": "provekit-realize-rust-core consumed the resolved body tree and emitted the constructor expression, but the resolved term lacks the receiver prefix recorded by #962 trait-path-truncated.",
                })
            } else {
                json!({
                    "hunk": hunk,
                    "class": "structural-difference",
                    "explanation": "post-rustfmt text differs in AST shape rather than spelling-only identifier changes.",
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

#[test]
fn value_null_source_round_trip_receipt_is_complete() {
    let repo_root = repo_root();
    let fixture_text =
        std::fs::read_to_string(fixture_path(&repo_root)).expect("read D7-v0 value null fixture");
    let fixture: JsonValue = serde_json::from_str(&fixture_text).expect("parse D7-v0 fixture");
    let fixture_cid = json_cid(&fixture).expect("fixture CID");
    assert_eq!(fixture_cid, EXPECTED_FIXTURE_CID);

    let term: Term =
        serde_json::from_value(fixture["proofir_term"].clone()).expect("decode ProofIR term");
    let catalog = value_null_catalog(&fixture);
    let resolved = proofir_resolve(&term, &catalog).expect("resolve D7-v0 ProofIR term");
    let resolved_jcs = serializable_jcs(&resolved).expect("ResolvedTerm JCS");
    let resolved_summary = term_summary(&fixture, &resolved);

    let function = function_name_from_target(&fixture);
    let params: Vec<String> = Vec::new();
    let param_types: Vec<String> = Vec::new();
    let return_type = return_type_from_loss_record(&fixture);
    let concept_name = root_concept_name(&fixture, &resolved);
    assert!(
        !fixture["loss_record"]
            .as_array()
            .expect("loss_record array")
            .iter()
            .any(|loss| {
                loss["loss"] == "trait-path-truncated" && loss["detail"] == "Arc :: new"
            }),
        "D7-v3 fixture must retire Arc::new trait-path-truncated loss"
    );

    let source_text =
        std::fs::read_to_string(source_path(&repo_root)).expect("read canonicalizer value.rs");
    let original_slice = extract_value_null_slice(&source_text);
    let original_slice_cid = blake3_512_of(original_slice.as_bytes());
    let (original_rustfmt, original_rustfmt_command) =
        rustfmt_source(&repo_root, "value_null_original.rs", &original_slice);

    let v3_realization = provekit_realize_rust_core::emit_from_resolved(
        &resolved_jcs,
        &function,
        &params,
        &param_types,
        &return_type,
    );
    let (v3_regenerated_rustfmt, v3_regenerated_rustfmt_command) = rustfmt_source(
        &repo_root,
        "value_null_regenerated_v3.rs",
        &v3_realization.source,
    );
    let v3_byte_identical = v3_regenerated_rustfmt.as_bytes() == original_rustfmt.as_bytes();
    let v3_diff = unified_diff(&repo_root, &original_rustfmt, &v3_regenerated_rustfmt);
    let v3_classification = classify_diff(&v3_diff, v3_realization.is_stub, &concept_name);
    let v3_verdict = if v3_byte_identical {
        "BYTE_IDENTICAL"
    } else {
        "CHARACTERIZED_DIFF"
    };
    let v3_dominant_diff_class = dominant_diff_class(v3_byte_identical, &v3_classification);
    let v3_regenerated_source_cid = blake3_512_of(v3_regenerated_rustfmt.as_bytes());
    let original_slice_post_rustfmt_cid = blake3_512_of(original_rustfmt.as_bytes());

    if !v3_byte_identical {
        assert!(
            !v3_classification.is_empty(),
            "non-identical source must have a classified diff hunk"
        );
    }

    assert_eq!(v3_verdict, "BYTE_IDENTICAL");
    assert_eq!(v3_dominant_diff_class, "byte-identical");
    assert_eq!(v3_diff, "");
    assert_eq!(v3_regenerated_source_cid, original_slice_post_rustfmt_cid);

    let v3_receipt = json!({
        "version": "1",
        "target": {
            "crate": "provekit-canonicalizer",
            "function": "impl Value::null",
            "source_path": "implementations/rust/provekit-canonicalizer/src/value.rs",
        },
        "pipeline": {
            "step_1_fixture_cid": EXPECTED_FIXTURE_CID,
            "step_2_resolve": format!("summary={resolved_summary}; jcs={resolved_jcs}"),
            "step_3_4_realize_command": format!(
                "provekit_realize_rust_core::emit_from_resolved(<resolved-term-jcs>, {function:?}, &[], &[], {return_type:?})"
            ),
            "step_3_4_realize_input": {
                "resolved_term_json": resolved_jcs,
                "function": function,
                "params": params,
                "param_types": param_types,
                "return_type": return_type,
            },
            "step_3_4_regenerated_source": v3_realization.source,
            "step_5_original_slice_cid": original_slice_cid,
            "step_6_rustfmt_command": format!("{v3_regenerated_rustfmt_command}\n{original_rustfmt_command}"),
            "step_7_byte_identical_post_rustfmt": v3_byte_identical,
            "step_8_unified_diff": v3_diff,
            "step_9_diff_classification": v3_classification,
            "step_10_dominant_diff_class": v3_dominant_diff_class.clone(),
            "step_11_expected_diff_shape": "post-rustfmt byte-identical",
            "step_12_empirical_root_cause": "#962 trait-path-truncated is retired for this call:new Value::null body because the resolved term now carries Arc::new.",
            "regenerated_source_cid": v3_regenerated_source_cid,
            "original_slice_post_rustfmt_cid": original_slice_post_rustfmt_cid,
        },
        "verdict": v3_verdict,
        "dominant_diff_class": v3_dominant_diff_class,
        "next_action": "D7 single-function terminus reached for Value::null; next chunks widen the empirical claim to module-level source round trips.",
    });

    if let Ok(out_dir) = std::env::var("D7_V3_RECEIPT_OUT_DIR") {
        let v3_receipt_path =
            PathBuf::from(out_dir).join("value_null_source_round_trip_receipt.json");
        std::fs::create_dir_all(v3_receipt_path.parent().expect("receipt parent"))
            .expect("create generated D7-v3 receipt dir");
        std::fs::write(
            &v3_receipt_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&v3_receipt).expect("pretty v3 receipt")
            ),
        )
        .expect("write generated D7-v3 receipt");

        let v3_parsed: JsonValue = serde_json::from_str(
            &std::fs::read_to_string(&v3_receipt_path).expect("read generated D7-v3 receipt"),
        )
        .expect("parse generated D7-v3 receipt");
        assert_eq!(v3_parsed["verdict"].as_str(), Some("BYTE_IDENTICAL"));
        assert_eq!(
            v3_parsed["dominant_diff_class"].as_str(),
            Some("byte-identical")
        );
        println!("v3_receipt_path={}", v3_receipt_path.display());
        println!("v3_verdict={v3_verdict}");
        return;
    }

    let v3_receipt_path = receipt_path(&repo_root, "D7-v3");
    let v3_parsed: JsonValue = serde_json::from_str(
        &std::fs::read_to_string(&v3_receipt_path).expect("read committed D7-v3 receipt"),
    )
    .expect("parse committed D7-v3 receipt");
    assert_eq!(v3_parsed, v3_receipt, "committed D7-v3 receipt drifted");
    assert_eq!(v3_parsed["verdict"].as_str(), Some("BYTE_IDENTICAL"));
    assert_eq!(
        v3_parsed["dominant_diff_class"].as_str(),
        Some("byte-identical")
    );
    println!("v3_receipt_path={}", v3_receipt_path.display());
    println!("v3_verdict={v3_verdict}");
}
