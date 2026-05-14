// SPDX-License-Identifier: Apache-2.0
//
// Integration tests for `provekit bind`.
//
// Coverage:
//   - All 9 (rewrite x mode) configurations exit 0 against the cmd_bind fixture
//   - Annotation content (concept comment, substrate-origin, requires/ensures,
//     mode attributes for annotate mode)
//   - Canonical cross-language output (Rust -> Java, Rust -> Python, Rust -> Go)
//   - Invisible mode does not modify source on disk
//   - Site mementos + index.json + gaps.json written
//   - Annotation idempotence (second pass identical to first)
//   - Multi-target canonical emission smoke (Rust->Java, Rust->Python, Rust->Rust)
//   - Java output carries contract annotation syntax (contract-annotated demo)
//   - gaps.json records v0-capability-gap and v0-orp-delegation-gap entries

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_ir_types::{PromotionDecisionMemento, PromotionGate, PromotionResult};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cmd_bind")
}

/// Copy fixture tree to a tempdir (protects checked-in source from annotate writes).
fn copy_fixture_to_temp() -> PathBuf {
    let tmp = tempfile::tempdir().expect("tempdir").into_path();
    copy_dir(&fixture_root(), &tmp);
    tmp
}

fn copy_dir(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let dest = dst.join(&name);
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            let _ = fs::copy(&path, &dest);
        }
    }
}

fn copy_fixture_with_bind_lift_manifest() -> PathBuf {
    let root = copy_fixture_to_temp();
    let kit_path = root.join("bind-lift-kit.py");
    let shape_cid = format!("blake3-512:{}", "0".repeat(128));
    let script = format!(
        r#"#!/usr/bin/env python3
import json
import sys

SHAPE_CID = {shape_cid:?}

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {{}}
    elif method == "lift":
        result = {{
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{{
                "kind": "bind-lift-entry",
                "file": "src/account.rs",
                "fn_name": "deposit",
                "fn_line": 14,
                "attr_pre": "amount > 0",
                "attr_post": "out >= 0",
                "concept_annotation": "deposit-then-balance",
                "param_names": ["balance", "amount"],
                "param_types": ["i64", "i64"],
                "return_type": "i64",
                "term_shape": {{"kind": "body", "stmts": [{{"kind": "opaque"}}]}},
                "term_shape_cid": SHAPE_CID
            }}]
        }}
    elif method == "shutdown":
        result = {{}}
    else:
        print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "error": {{"message": "unknown method"}}}}), flush=True)
        continue
    print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "result": result}}), flush=True)
    if method == "shutdown":
        break
"#
    );
    fs::write(&kit_path, script).expect("write bind lift kit");
    let manifest_dir = root.join(".provekit").join("lift").join("rust");
    fs::create_dir_all(&manifest_dir).expect("create lift manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"test-bind-lift\"\ncommand = [\"python3\", \"{}\"]\n",
            kit_path.display()
        ),
    )
    .expect("write lift manifest");
    root
}

fn copy_fixture_with_three_evidence_manifest() -> PathBuf {
    let root = copy_fixture_to_temp();
    let kit_path = root.join("bind-lift-kit.py");
    let shape_cid = format!("blake3-512:{}", "1".repeat(128));
    let script = format!(
        r#"#!/usr/bin/env python3
import json
import sys

SHAPE_CID = {shape_cid:?}

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {{}}
    elif method == "lift":
        result = {{
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{{
                "kind": "bind-lift-entry",
                "file": "src/account.rs",
                "fn_name": "deposit",
                "fn_line": 14,
                "concept_annotation": "deposit-then-balance",
                "param_names": ["balance", "amount"],
                "param_types": ["i64", "i64"],
                "return_type": "i64",
                "term_shape": {{"kind": "body", "stmts": [{{"kind": "opaque"}}]}},
                "term_shape_cid": SHAPE_CID,
                "witnesses": [
                    {{
                        "role": "pre",
                        "predicate_text": "amount > 0",
                        "source_kind": "type-signature",
                        "line": 14,
                        "col": 20
                    }},
                    {{
                        "role": "post",
                        "predicate_text": "out >= balance",
                        "source_kind": "docstring",
                        "line": 13,
                        "col": 0
                    }},
                    {{
                        "role": "post",
                        "predicate_text": "out == balance + amount",
                        "source_kind": "test-assertion",
                        "line": 7,
                        "col": 8
                    }}
                ]
            }}]
        }}
    elif method == "shutdown":
        result = {{}}
    else:
        print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "error": {{"message": "unknown method"}}}}), flush=True)
        continue
    print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "result": result}}), flush=True)
    if method == "shutdown":
        break
"#
    );
    fs::write(&kit_path, script).expect("write bind lift kit");
    let manifest_dir = root.join(".provekit").join("lift").join("rust");
    fs::create_dir_all(&manifest_dir).expect("create lift manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"test-bind-lift-three-evidence\"\ncommand = [\"python3\", \"{}\"]\n",
            kit_path.display()
        ),
    )
    .expect("write lift manifest");
    root
}

fn copy_fixture_with_attrs_and_authoritative_witness_manifest() -> PathBuf {
    let root = copy_fixture_to_temp();
    let kit_path = root.join("bind-lift-kit.py");
    let shape_cid = format!("blake3-512:{}", "2".repeat(128));
    let script = format!(
        r#"#!/usr/bin/env python3
import json
import sys

SHAPE_CID = {shape_cid:?}

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {{}}
    elif method == "lift":
        result = {{
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{{
                "kind": "bind-lift-entry",
                "file": "src/account.rs",
                "fn_name": "deposit",
                "fn_line": 14,
                "attr_pre": "amount > 0",
                "attr_post": "out >= 0",
                "concept_annotation": "deposit-then-balance",
                "param_names": ["balance", "amount"],
                "param_types": ["i64", "i64"],
                "return_type": "i64",
                "term_shape": {{"kind": "body", "stmts": [{{"kind": "opaque"}}]}},
                "term_shape_cid": SHAPE_CID,
                "witnesses": [
                    {{
                        "role": "post",
                        "predicate_text": "out == balance + amount",
                        "source_kind": "test-assertion",
                        "line": 7,
                        "col": 8
                    }}
                ]
            }}]
        }}
    elif method == "shutdown":
        result = {{}}
    else:
        print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "error": {{"message": "unknown method"}}}}), flush=True)
        continue
    print(json.dumps({{"jsonrpc": "2.0", "id": request_id, "result": result}}), flush=True)
    if method == "shutdown":
        break
"#
    );
    fs::write(&kit_path, script).expect("write bind lift kit");
    let manifest_dir = root.join(".provekit").join("lift").join("rust");
    fs::create_dir_all(&manifest_dir).expect("create lift manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"test-bind-lift-authoritative-witness\"\ncommand = [\"python3\", \"{}\"]\n",
            kit_path.display()
        ),
    )
    .expect("write lift manifest");
    root
}

fn bind_cmd(
    root: &Path,
    out: &Path,
    rewrite: &str,
    mode: &str,
    target_lang: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(provekit_bin());
    cmd.arg("bind")
        .arg("--root")
        .arg(root)
        .arg("--lang")
        .arg("rust")
        .arg("--output")
        .arg(out)
        .arg("--rewrite")
        .arg(rewrite)
        .arg("--mode")
        .arg(mode)
        .arg("--quiet");
    if let Some(lang) = target_lang {
        cmd.arg("--target-language").arg(lang);
    }
    cmd.output().expect("spawn provekit bind")
}

fn read_json_dir(dir: &Path) -> Vec<serde_json::Value> {
    fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read_dir({}): {err}", dir.display()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .map(|entry| {
            let path = entry.path();
            serde_json::from_str(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display())),
            )
            .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
        })
        .collect()
}

// ============================================================================
// Test 1: --help lists bind command
// ============================================================================

#[test]
fn help_lists_bind_command() {
    let out = Command::new(provekit_bin())
        .arg("--help")
        .output()
        .expect("spawn provekit --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "provekit --help failed:\n{stdout}");
    assert!(
        stdout.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("bind ") || t == "bind"
        }),
        "provekit --help must list the bind subcommand\n{stdout}"
    );
}

// ============================================================================
// Tests 2-4: annotate x {monitor, emitter, witness}
// ============================================================================

#[test]
fn annotate_monitor_injects_concept_and_monitor_attr() {
    let tmp = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(
        result.status.success(),
        "annotate+monitor should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("// concept:"),
        "must inject concept comment"
    );
    assert!(
        rewritten.contains("// substrate-origin:"),
        "must inject substrate-origin"
    );
    assert!(
        rewritten.contains("#[cfg_attr(any(), requires("),
        "must inject requires"
    );
    assert!(
        rewritten.contains("#[cfg_attr(any(), ensures("),
        "must inject ensures"
    );
    assert!(
        rewritten.contains("provekit_monitor"),
        "monitor mode must inject provekit_monitor attribute"
    );
}

#[test]
fn witnesses_are_authoritative_over_legacy_attr_fields() {
    let tmp = copy_fixture_with_attrs_and_authoritative_witness_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(
        result.status.success(),
        "annotate+monitor should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("ensures(out == balance + amount)"),
        "non-empty witnesses[] must drive emitted postconditions:\n{rewritten}"
    );
    assert!(
        !rewritten.contains("ensures(out >= 0)"),
        "legacy attr_post must not override non-empty witnesses[]:\n{rewritten}"
    );
    assert!(
        !rewritten.contains("requires(amount > 0)"),
        "legacy attr_pre must not be promoted when witnesses[] is non-empty:\n{rewritten}"
    );
}

#[test]
fn annotate_emitter_injects_emitter_attribute() {
    let tmp = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "emitter", None);
    assert!(result.status.success(), "annotate+emitter should succeed");
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("provekit_emitter"),
        "emitter mode must inject provekit_emitter"
    );
}

#[test]
fn annotate_witness_injects_witness_attribute() {
    let tmp = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "witness", None);
    assert!(result.status.success(), "annotate+witness should succeed");
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("provekit_witness"),
        "witness mode must inject provekit_witness"
    );
}

// ============================================================================
// Tests 5-7: canonical x {monitor, emitter, witness}
// ============================================================================

#[test]
fn canonical_monitor_rust_target_creates_translated_dir() {
    let root = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("rust"));
    assert!(
        result.status.success(),
        "canonical+monitor+rust should succeed"
    );
    assert!(
        out.join("translated").join("rust").exists(),
        "translated/rust dir must be created"
    );
}

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn canonical_emitter_java_creates_java_output_with_contract() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "emitter", Some("java"));
    assert!(
        result.status.success(),
        "canonical+emitter+java should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let java_dir = out.join("translated").join("java");
    assert!(java_dir.exists(), "translated/java must exist");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "at least one .java file expected");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();
    assert!(
        java_src.contains("concept:"),
        "Java output must carry concept annotation"
    );
    // Java canonical+emitter must have emitter annotation.
    assert!(
        java_src.contains("provekit_emitter"),
        "Java canonical+emitter must carry emitter annotation\n{java_src}"
    );
    // Contract annotations for deposit function.
    assert!(
        java_src.contains("@requires:") || java_src.contains("/* @requires"),
        "Java output must carry contract annotation for deposit\n{java_src}"
    );
}

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn canonical_witness_python_creates_python_output() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "witness", Some("python"));
    assert!(
        result.status.success(),
        "canonical+witness+python should succeed"
    );
    let py_dir = out.join("translated").join("python");
    assert!(py_dir.exists(), "translated/python must exist");
    let py_files: Vec<_> = fs::read_dir(&py_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "at least one .py file expected");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(
        py_src.contains("# concept:"),
        "Python output must carry concept comment"
    );
}

// ============================================================================
// Tests 8-10: invisible x {monitor, emitter, witness} — must NOT write source
// ============================================================================

fn assert_source_unchanged(label: &str, mode: &str) {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let src_before = fs::read_to_string(root.join("src").join("account.rs")).unwrap();
    let result = bind_cmd(&root, &out, "invisible", mode, None);
    assert!(
        result.status.success(),
        "invisible+{mode} should succeed\nstderr: {}\n({label})",
        String::from_utf8_lossy(&result.stderr)
    );
    let src_after = fs::read_to_string(root.join("src").join("account.rs")).unwrap();
    assert_eq!(
        src_before, src_after,
        "invisible+{mode} must not modify source file ({label})"
    );
}

#[test]
fn invisible_monitor_does_not_modify_source() {
    assert_source_unchanged("invisible_monitor", "monitor");
}

#[test]
fn invisible_emitter_does_not_modify_source() {
    assert_source_unchanged("invisible_emitter", "emitter");
}

#[test]
fn invisible_witness_does_not_modify_source() {
    assert_source_unchanged("invisible_witness", "witness");
}

// ============================================================================
// Test 11: site mementos, index.json, gaps.json
// ============================================================================

#[test]
fn bind_writes_site_mementos_index_and_gaps() {
    let root = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(result.status.success(), "invisible+monitor should succeed");

    // index.json
    let index_path = out.join("index.json");
    assert!(index_path.exists(), "index.json must be written");
    let idx: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
    assert!(
        idx["total_bindings"].as_u64().unwrap_or(0) > 0,
        "index must record bindings"
    );
    assert!(idx["verdicts"].is_object(), "index must have verdicts");

    // gaps.json
    assert!(out.join("gaps.json").exists(), "gaps.json must be written");

    // sites/
    let sites_dir = out.join("sites");
    if sites_dir.exists() {
        let site_files: Vec<_> = fs::read_dir(&sites_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        if !site_files.is_empty() {
            let m: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(site_files[0].path()).unwrap()).unwrap();
            assert_eq!(
                m["schemaVersion"].as_str().unwrap_or(""),
                "1",
                "memento schemaVersion must be 1"
            );
            assert_eq!(
                m["kind"].as_str().unwrap_or(""),
                "concept-site",
                "memento kind must be concept-site"
            );
        }
    }
}

#[test]
fn bind_writes_evidence_and_compound_contracts() {
    let root = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(
        result.status.success(),
        "invisible+monitor should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let evidence_dir = out.join("evidence");
    assert!(evidence_dir.exists(), "evidence/ must be written");
    let evidence_docs = read_json_dir(&evidence_dir);
    assert!(
        evidence_docs.iter().any(|doc| {
            doc["kind"].as_str() == Some("evidence")
                && doc["source_kind"].as_str() == Some("annotation")
                && doc["extension_fields"]["role"].as_str() == Some("pre")
        }),
        "annotation precondition must be promoted to EvidenceMemento: {evidence_docs:?}"
    );

    let contracts_dir = out.join("contracts");
    assert!(contracts_dir.exists(), "contracts/ must be written");
    let contract_docs = read_json_dir(&contracts_dir);
    let compound_cids: BTreeSet<String> = contract_docs
        .iter()
        .filter(|doc| doc["kind"].as_str() == Some("compound-contract"))
        .filter_map(|doc| doc["cid"].as_str().map(str::to_string))
        .collect();
    assert!(
        !compound_cids.is_empty(),
        "bind must write CompoundContractMemento records: {contract_docs:?}"
    );
    assert!(
        contract_docs.iter().any(|doc| {
            doc["kind"].as_str() == Some("compound-contract")
                && doc["evidences"]
                    .as_array()
                    .map(|evidences| !evidences.is_empty())
                    .unwrap_or(false)
        }),
        "compound contracts must reference evidence CIDs: {contract_docs:?}"
    );

    let site_docs = read_json_dir(&out.join("sites"));
    assert!(
        site_docs.iter().any(|doc| {
            doc["kind"].as_str() == Some("concept-site")
                && doc["local_contract_cid"]
                    .as_str()
                    .map(|cid| compound_cids.contains(cid))
                    .unwrap_or(false)
        }),
        "ConceptSiteMemento.local_contract_cid must point at a compound contract CID"
    );
}

#[test]
fn bind_mints_promotion_decisions_for_admitted_evidence() {
    let root = copy_fixture_with_three_evidence_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(
        result.status.success(),
        "invisible+monitor should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let evidence_docs = read_json_dir(&out.join("evidence"));
    assert_eq!(
        evidence_docs.len(),
        3,
        "fixture should admit exactly three EvidenceMementos"
    );

    let promotion_dir = out.join("promotion-decisions");
    assert!(
        promotion_dir.exists(),
        "promotion-decisions/ must be written"
    );
    let promotion_docs = read_json_dir(&promotion_dir);
    assert_eq!(
        promotion_docs.len(),
        evidence_docs.len(),
        "bind must mint one PromotionDecisionMemento per admitted evidence field"
    );

    for doc in promotion_docs {
        let decision: PromotionDecisionMemento =
            serde_json::from_value(doc).expect("parse PromotionDecisionMemento");
        decision
            .validate()
            .expect("promotion decisions must carry non-empty evidence_cids");
        assert_eq!(
            decision.header.cid,
            decision
                .recompute_header_cid()
                .expect("recompute header cid"),
            "header.cid must match recomputed CID"
        );
        let serialized = serde_json::to_string(&decision).expect("serialize promotion decision");
        let round_trip: PromotionDecisionMemento =
            serde_json::from_str(&serialized).expect("deserialize promotion decision");
        assert_eq!(decision, round_trip, "promotion decision must round-trip");
        assert_eq!(decision.header.kind, "promotion-decision");
        assert_eq!(decision.header.schema_version, "1");
        assert_eq!(decision.header.result, PromotionResult::Admitted);
        assert_eq!(decision.header.gate, PromotionGate::Proof);
        assert_eq!(
            decision.header.decision_payload["result"].as_str(),
            Some("admitted")
        );
        assert_eq!(
            decision.header.evidence_cids.len(),
            1,
            "each per-field admission should cite the admitted evidence CID"
        );
    }
}

// ============================================================================
// Test 12: annotation idempotence
// ============================================================================

#[test]
fn annotate_is_idempotent() {
    let tmp = copy_fixture_to_temp();
    let out = tempfile::tempdir().expect("tempdir").into_path();

    // pass0: original source bytes, read before any bind run.
    let pass0 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // pass1: source after first annotate run.
    let r1 = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(r1.status.success(), "first annotate pass should succeed");
    let pass1 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // B2 regression gate: pass1 must contain the *content* of the lifted contract for
    // deposit, not merely structural annotations.  The cmd_bind annotator strips existing
    // `#[cfg_attr(any(), requires...)]` lines and re-emits them only when the parse-lift
    // extracted a non-empty predicate (B1 fix).  If B1's trim-ordering fix is reverted
    // the parser fails to extract the predicate and `requires(amount > 0)` is silently
    // dropped from pass1 — both this assertion AND the structural-idempotence check below
    // must fire against that regression.
    assert!(
        pass1.contains("substrate-origin: annotation-lift"),
        "pass1 must carry `substrate-origin: annotation-lift` for the deposit function \
         (B2 regression check — empty would mean the annotation-lift path did not fire)"
    );
    assert!(
        pass1.contains("requires(amount > 0)")
            || pass1.contains("requires (amount > 0)")
            || pass1.contains("requires(amount>0)"),
        "pass1 must contain the lifted `requires(amount > 0)` predicate for deposit \
         (B2 regression check — absence means B1 trim-ordering bug was reintroduced)\n\
         pass1:\n{pass1}"
    );

    // pass2: source after second annotate run on already-annotated file.
    let r2 = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(r2.status.success(), "second annotate pass should succeed");
    let pass2 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // Loss check: pass1 must contain every non-annotation line from pass0 unchanged.
    // Any fn body line from the original source must survive annotation injection.
    let is_substrate_line = |l: &str| -> bool {
        let t = l.trim_start();
        t.starts_with("// concept:")
            || t.starts_with("// substrate-origin:")
            || t.starts_with("// memento-cid:")
            || t.starts_with("// witness-inherited-from:")
            || t.starts_with("#[cfg_attr(any(), requires")
            || t.starts_with("#[cfg_attr(any(), ensures")
            || t.starts_with("#[cfg_attr(any(), provekit_monitor")
            || t.starts_with("#[cfg_attr(any(), provekit_emitter")
            || t.starts_with("#[cfg_attr(any(), provekit_witness")
    };
    let pass0_non_substrate: Vec<&str> = pass0.lines().filter(|l| !is_substrate_line(l)).collect();
    for original_line in &pass0_non_substrate {
        assert!(
            pass1.contains(original_line),
            "annotate pass1 lost a non-substrate line from pass0: {:?}\npass1:\n{pass1}",
            original_line
        );
    }

    // Structural idempotence check: pass1 == pass2 modulo lines that are allowed to
    // re-evaluate on each pass:
    //   - memento-cid: encodes source file hash; changes after first write
    //   - substrate-origin: may re-classify if injected attrs are now readable (v0 limitation)
    // Concept names, contract predicates (requires/ensures), function bodies, and all
    // user-written code MUST be byte-stable across passes.
    let strip_volatile = |s: &str| -> String {
        s.lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("// memento-cid:") && !t.starts_with("// substrate-origin:")
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    assert_eq!(
        strip_volatile(&pass1),
        strip_volatile(&pass2),
        "annotate must be structurally idempotent: concept names, contract predicates, \
         and function bodies must be stable across passes (memento-cid and substrate-origin may re-evaluate)"
    );
}

// ============================================================================
// Test 13: Java deposit carries contract annotations
// (the user-visible demo: "a unit test in Rust converted into contract
// annotations in Java that say this is required at this call site")
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn java_canonical_deposit_carries_contract_annotations() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("java"));
    assert!(result.status.success(), "canonical+java should succeed");
    let java_dir = out.join("translated").join("java");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .unwrap_or_else(|_| panic!("translated/java must exist"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "java output required for demo");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();

    // The deposit function has #[requires(amount > 0)] + #[ensures(out >= 0)].
    // ORP's Java emitter should render these as contract annotations.
    assert!(
        java_src.contains("@requires:") || java_src.contains("/* @requires"),
        "Java must carry @requires contract annotation\n{java_src}"
    );
    assert!(
        java_src.contains("@ensures:") || java_src.contains("/* @ensures"),
        "Java must carry @ensures contract annotation\n{java_src}"
    );
    // Concept annotation present.
    assert!(
        java_src.contains("concept:"),
        "Java must carry concept annotation\n{java_src}"
    );
    // Substrate origin present.
    assert!(
        java_src.contains("substrate-origin:"),
        "Java must carry substrate-origin\n{java_src}"
    );
}

// ============================================================================
// Test 14: Python canonical carries Python-idiomatic contract comments
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn python_canonical_carries_contract_comments() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("python"));
    assert!(result.status.success(), "canonical+python should succeed");
    let py_dir = out.join("translated").join("python");
    let py_files: Vec<_> = fs::read_dir(&py_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "python output required");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(
        py_src.contains("# concept:"),
        "Python must carry concept comment"
    );
    assert!(
        py_src.contains("# @requires:") || py_src.contains("@requires"),
        "Python must carry contract annotation\n{py_src}"
    );
}

// ============================================================================
// Test 15: Multi-target canonical emission smoke (Rust->Java, Rust->Python, Rust->Rust)
//
// Each leg exits 0 and produces concept-bearing output. In v0 the bind
// engine processes Rust source for all legs via the M+N concept hub (the hub
// collapses the NxM translation table: each leg is Rust->concept hub->target).
// True multi-leg round-trip (lift from Java/Python output back through the hub)
// is deferred to v1 when multi-lang lift_plugin dispatch is complete.
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn canonical_multi_target_emission_smoke() {
    let root = fixture_root();

    // Leg 1: Rust -> Java
    let out1 = tempfile::tempdir().expect("tempdir").into_path();
    let r1 = bind_cmd(&root, &out1, "canonical", "monitor", Some("java"));
    assert!(
        r1.status.success(),
        "trinity leg 1 (Rust->Java) must succeed"
    );
    let java_dir = out1.join("translated").join("java");
    assert!(java_dir.exists(), "java dir must exist after leg 1");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "java output required");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();
    assert!(
        java_src.contains("concept:"),
        "java output must carry concept"
    );

    // Leg 2: Rust -> Python (via the same hub)
    let out2 = tempfile::tempdir().expect("tempdir").into_path();
    let r2 = bind_cmd(&root, &out2, "canonical", "monitor", Some("python"));
    assert!(
        r2.status.success(),
        "trinity leg 2 (Rust->Python) must succeed"
    );
    let py_dir = out2.join("translated").join("python");
    assert!(py_dir.exists(), "python dir must exist after leg 2");
    let py_files: Vec<_> = fs::read_dir(&py_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "python output required");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(
        py_src.contains("concept:"),
        "python output must carry concept"
    );

    // Leg 3: Rust -> Rust (same-language canonical refactor)
    let out3 = tempfile::tempdir().expect("tempdir").into_path();
    let r3 = bind_cmd(&root, &out3, "canonical", "monitor", Some("rust"));
    assert!(
        r3.status.success(),
        "trinity leg 3 (Rust->Rust) must succeed"
    );
    let rs_dir = out3.join("translated").join("rust");
    assert!(rs_dir.exists(), "rust canonical dir must exist after leg 3");
    let rs_files: Vec<_> = fs::read_dir(&rs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(!rs_files.is_empty(), "rust canonical output required");
    let rs_src = fs::read_to_string(rs_files[0].path()).unwrap();
    assert!(
        rs_src.contains("concept:"),
        "rust canonical output must carry concept"
    );

    // Verify index.json from leg 1 records verdict breakdown.
    let idx: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out1.join("index.json")).unwrap()).unwrap();
    let total = idx["total_bindings"].as_u64().unwrap_or(0);
    assert!(total > 0, "trinity: index must record at least one binding");
    let exact = idx["verdicts"]["exact"].as_u64().unwrap_or(0);
    let lossy = idx["verdicts"]["loudly_bounded_lossy"]
        .as_u64()
        .unwrap_or(0);
    let refuse = idx["verdicts"]["refuse"].as_u64().unwrap_or(0);
    assert!(
        exact + lossy + refuse == total,
        "all bindings must have a verdict"
    );
}

// ============================================================================
// Test 16: gaps.json reflects real substrate state (post PR #779 + #783)
// ============================================================================

#[test]
fn gaps_doc_reflects_real_substrate_state() {
    // Pre-PR #783: bind emitted two unconditional `v0-capability-gap` entries
    // ("multi-lang lift_plugin dispatch deferred", "real ConceptAbstractionMemento
    // catalog lookup deferred") for every run. PR #779 made the dispatcher
    // exercise the actual per-language kits via kit_dispatch, so those gaps
    // became lies: they claimed a substrate limitation that no longer existed.
    //
    // Per Supra omnia rectum, gaps must reflect REAL substrate state. The
    // honest replacements are situation-specific:
    //   - `kit-plugin-unavailable` is emitted by the dispatcher when a kit
    //     isn't registered for the requested language.
    //   - `bind-stub-body-emitted` is emitted per concept by
    //     `apply_canonical_rewrite` when a body falls through to a language
    //     stub.
    //   - `below-threshold` is emitted per concept when site count falls
    //     under the threshold.
    //   - `kit-plugin-unavailable` for realize is emitted when the realize
    //     kit isn't registered.
    //
    // None of those require an unconditional v0-capability-gap. This test
    // enforces the absence of the stale lying gap kind.
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(result.status.success(), "bind should succeed");
    let gaps: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("gaps.json")).unwrap()).unwrap();
    let gap_arr = gaps["gaps"].as_array().expect("gaps must be array");
    let kinds: Vec<&str> = gap_arr.iter().filter_map(|g| g["kind"].as_str()).collect();
    assert!(
        !kinds.contains(&"v0-capability-gap"),
        "gaps.json must NOT record stale v0-capability-gap entries after PR #779 made dispatch real (PR #783 removed the lie)"
    );
    assert!(
        !kinds.contains(&"v0-orp-delegation-gap"),
        "gaps.json must NOT record v0-orp-delegation-gap after delegation is closed"
    );
}

// ============================================================================
// Test 17: Go canonical output
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn canonical_go_target_creates_go_output() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("go"));
    assert!(result.status.success(), "canonical+go should succeed");
    let go_dir = out.join("translated").join("go");
    assert!(go_dir.exists(), "translated/go must exist");
    let go_files: Vec<_> = fs::read_dir(&go_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "go").unwrap_or(false))
        .collect();
    assert!(!go_files.is_empty(), "at least one .go file expected");
    let go_src = fs::read_to_string(go_files[0].path()).unwrap();
    assert!(
        go_src.contains("concept:"),
        "Go output must carry concept annotation"
    );
}

// ============================================================================
// F5: Per-language syntax-checker regression tests (Test 18-26)
//
// Each test:
//   1. Invokes bind --rewrite canonical --target-language <lang>
//   2. Reads the emitted output file
//   3. Pipes it through the language's own syntax checker (or asserts structural
//      validity when a checker is not universally available).
//   4. Fails if the checker reports a syntax error.
//
// This is the load-bearing protocol. CI must not regress any of these without
// the checker explicitly reporting success.
// ============================================================================

/// Run `rustc --crate-type=lib --emit=metadata` on a file; return Ok(()) or Err(stderr).
fn rustc_check(path: &std::path::Path) -> Result<(), String> {
    let out_dir = tempfile::tempdir().expect("tempdir");
    let out = std::process::Command::new("rustc")
        .args(["--crate-type=lib", "--emit=metadata", "--edition=2021"])
        .arg("--out-dir")
        .arg(out_dir.path())
        .arg(path)
        .output()
        .expect("spawn rustc");
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        // Warnings are fine; only fail on errors.
        if stderr.lines().any(|l| l.starts_with("error")) {
            Err(stderr)
        } else {
            Ok(())
        }
    }
}

/// Run `python3 -m py_compile` on a file.
fn python_check(path: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("python3")
        .args(["-m", "py_compile"])
        .arg(path)
        .output()
        .expect("spawn python3");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Run `gofmt -e` on a file (parse check).
fn go_check(path: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("gofmt")
        .arg("-e")
        .arg(path)
        .output()
        .expect("spawn gofmt");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Run `ruby -c` on a file.
fn ruby_check(path: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("ruby")
        .arg("-c")
        .arg(path)
        .output()
        .expect("spawn ruby");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Run `php -l` on a file.
fn php_check(path: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("php")
        .arg("-l")
        .arg(path)
        .output()
        .expect("spawn php");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr))
    }
}

/// Run `javac` on a file.
fn javac_check(path: &std::path::Path) -> Result<(), String> {
    let out_dir = tempfile::tempdir().expect("tempdir");
    let out = std::process::Command::new("javac")
        .arg("-d")
        .arg(out_dir.path())
        .arg(path)
        .output()
        .expect("spawn javac");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Run `zig fmt --check` on a file.
fn zig_check(path: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("zig")
        .args(["fmt", "--check"])
        .arg(path)
        .output()
        .expect("spawn zig");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string()
            + &String::from_utf8_lossy(&out.stdout))
    }
}

/// Find the first file with the given extension under a directory.
fn first_file_with_ext(dir: &std::path::Path, ext: &str) -> Option<std::path::PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
        .map(|e| e.path())
}

// ============================================================================
// Test 18: Rust canonical output passes rustc
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_rust_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("rust"));
    assert!(result.status.success(), "bind rust must exit 0");
    let dir = out.join("translated").join("rust");
    let f = first_file_with_ext(&dir, "rs").expect("no .rs output");
    rustc_check(&f).unwrap_or_else(|e| panic!("Rust syntax error in emitted .rs file:\n{e}"));
}

// ============================================================================
// Test 19: Python canonical output passes python3 -m py_compile
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_python_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("python"));
    assert!(result.status.success(), "bind python must exit 0");
    let dir = out.join("translated").join("python");
    let f = first_file_with_ext(&dir, "py").expect("no .py output");
    python_check(&f).unwrap_or_else(|e| panic!("Python syntax error in emitted .py file:\n{e}"));
}

// ============================================================================
// Test 20: Go canonical output passes gofmt -e (exactly one `package main`)
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_go_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("go"));
    assert!(result.status.success(), "bind go must exit 0");
    let dir = out.join("translated").join("go");
    let f = first_file_with_ext(&dir, "go").expect("no .go output");
    let src = fs::read_to_string(&f).unwrap();
    let pkg_count = src
        .lines()
        .filter(|l| l.trim_start() == "package main")
        .count();
    assert_eq!(
        pkg_count, 1,
        "Go output must contain exactly one `package main` declaration, found {pkg_count}"
    );
    go_check(&f).unwrap_or_else(|e| panic!("Go syntax error in emitted .go file:\n{e}"));
}

// ============================================================================
// Test 21: Java canonical output passes javac
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_java_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("java"));
    assert!(result.status.success(), "bind java must exit 0");
    let dir = out.join("translated").join("java");
    let f = first_file_with_ext(&dir, "java").expect("no .java output");
    javac_check(&f).unwrap_or_else(|e| panic!("Java syntax error in emitted .java file:\n{e}"));
}

// ============================================================================
// Test 22: Zig canonical output passes zig fmt --check (no #[cfg_attr])
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_zig_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("zig"));
    assert!(result.status.success(), "bind zig must exit 0");
    let dir = out.join("translated").join("zig");
    let f = first_file_with_ext(&dir, "zig").expect("no .zig output");
    let src = fs::read_to_string(&f).unwrap();
    assert!(
        !src.contains("#[cfg_attr"),
        "Zig output must NOT contain Rust #[cfg_attr(...)] syntax\n{src}"
    );
    zig_check(&f).unwrap_or_else(|e| panic!("Zig syntax error in emitted .zig file:\n{e}"));
}

// ============================================================================
// Test 23: Ruby canonical output passes ruby -c (uses `#` not `//` for comments)
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_ruby_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("ruby"));
    assert!(result.status.success(), "bind ruby must exit 0");
    let dir = out.join("translated").join("ruby");
    let f = first_file_with_ext(&dir, "rb").expect("no .rb output");
    let src = fs::read_to_string(&f).unwrap();
    // The canonical rewrite header must use `#` not `//` for Ruby.
    assert!(
        src.contains("# canonical rewrite:"),
        "Ruby output must use `#` comment prefix, not `//`\n{src}"
    );
    ruby_check(&f).unwrap_or_else(|e| panic!("Ruby syntax error in emitted .rb file:\n{e}"));
}

// ============================================================================
// Test 24: PHP canonical output passes php -l (exactly one `<?php`)
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_php_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("php"));
    assert!(result.status.success(), "bind php must exit 0");
    let dir = out.join("translated").join("php");
    let f = first_file_with_ext(&dir, "php").expect("no .php output");
    let src = fs::read_to_string(&f).unwrap();
    let php_tag_count = src.matches("<?php").count();
    assert_eq!(
        php_tag_count, 1,
        "PHP output must contain exactly one `<?php` open tag, found {php_tag_count}"
    );
    php_check(&f).unwrap_or_else(|e| panic!("PHP syntax error in emitted .php file:\n{e}"));
}

// ============================================================================
// Test 25: TypeScript canonical output is structurally valid
//          (checks for export function, no duplicate declarations)
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_typescript_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("typescript"));
    assert!(result.status.success(), "bind typescript must exit 0");
    let dir = out.join("translated").join("typescript");
    let f = first_file_with_ext(&dir, "ts").expect("no .ts output");
    let src = fs::read_to_string(&f).unwrap();
    // Structural invariants: must contain export functions (not just a stub comment).
    assert!(
        src.contains("export function"),
        "TypeScript output must contain `export function`\n{src}"
    );
    // Must not contain duplicate `export function deposit` (would be a concat bug).
    let export_count = src.matches("export function deposit").count();
    assert_eq!(
        export_count, 1,
        "TypeScript output must have exactly one `export function deposit`, found {export_count}\n{src}"
    );
    // tsc syntax check (if tsc is available; structural check is always enforced above).
    let tsc_path = std::process::Command::new("which")
        .arg("tsc")
        .output()
        .ok()
        .filter(|o| o.status.success());
    if tsc_path.is_some() {
        let ts_dir = tempfile::tempdir().expect("tempdir");
        let ts_copy = ts_dir.path().join("account.ts");
        fs::copy(&f, &ts_copy).unwrap();
        fs::write(
            ts_dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"noEmit":true,"strict":true,"target":"ES2020","module":"commonjs"},"include":["*.ts"]}"#,
        ).unwrap();
        let tsc_out = std::process::Command::new("tsc")
            .arg("--noEmit")
            .current_dir(ts_dir.path())
            .output()
            .expect("spawn tsc");
        assert!(
            tsc_out.status.success(),
            "TypeScript syntax error:\n{}",
            String::from_utf8_lossy(&tsc_out.stdout)
        );
    }
}

// ============================================================================
// Test 26: C# canonical output is structurally valid
//          (checks for public static class, no duplicate declarations)
// ============================================================================

#[test]
#[ignore = "pr770: requires <lang> realize kit; without it cmd_bind emits an honest kit-plugin-unavailable gap. Re-enable when the per-language realize kit lands."]
fn f5_csharp_canonical_parses() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("csharp"));
    assert!(result.status.success(), "bind csharp must exit 0");
    let dir = out.join("translated").join("csharp");
    let f = first_file_with_ext(&dir, "cs").expect("no .cs output");
    let src = fs::read_to_string(&f).unwrap();
    // Structural invariants: must contain public static class wrappers.
    assert!(
        src.contains("public static class"),
        "C# output must contain `public static class`\n{src}"
    );
    // Class names must be unique (per-function class naming prevents duplicate-class errors).
    let deposit_class_count = src.matches("class DepositTransported").count();
    assert_eq!(
        deposit_class_count, 1,
        "C# must have exactly one DepositTransported class, found {deposit_class_count}\n{src}"
    );
    // dotnet syntax check (if dotnet is available and operational).
    // Note: `dotnet build` occasionally fails in tmp-dir contexts due to MSBuild
    // environment issues (NuGet cache, SDK discovery from arbitrary paths). The
    // structural assertions above are always enforced; the dotnet check is
    // best-effort and skipped if dotnet is absent OR if the SDK reports an
    // environment error unrelated to C# syntax.
    let dotnet_available = std::process::Command::new("dotnet")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some();
    if dotnet_available {
        let cs_dir = tempfile::tempdir().expect("tempdir");
        let cs_copy = cs_dir.path().join("account.cs");
        fs::copy(&f, &cs_copy).unwrap();
        fs::write(
            cs_dir.path().join("test.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Library</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>"#,
        ).unwrap();
        let dotnet_out = std::process::Command::new("dotnet")
            .args(["build", "--nologo", "-q"])
            .current_dir(cs_dir.path())
            .output()
            .expect("spawn dotnet");
        // Only assert on actual C# parse errors (lines starting with "error CS").
        // MSBuild environment errors (MSBUILD errors, path creation warnings) are
        // non-deterministic in tmp contexts and must not be treated as C# syntax failures.
        let combined = String::from_utf8_lossy(&dotnet_out.stdout).to_string()
            + &String::from_utf8_lossy(&dotnet_out.stderr);
        let has_cs_error = combined.lines().any(|l| {
            let t = l.trim();
            t.contains("error CS") || t.contains(": error CS")
        });
        assert!(
            !has_cs_error,
            "C# syntax error in emitted .cs file:\n{combined}"
        );
    }
}

// ============================================================================
// F6 (post #766/#767/#768): gaps.json records bind-stub-body-emitted PER
// CONCEPT for every concept whose body fell through to the language stub.
// Annotate-rewrite path does NOT emit stub bodies (in-place source rewrite),
// so this test exercises a cross-language canonical rewrite where the
// realizer reports is_stub for each binding.
// ============================================================================

#[test]
fn f6_gaps_record_stub_body_emitted_per_concept() {
    let root = copy_fixture_with_bind_lift_manifest();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    // Canonical Rust→Go forces the realize path for every binding, and none
    // of the Go templates exist in v1.0.0, so every concept must produce a
    // per-concept `bind-stub-body-emitted` gap entry.
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("go"));
    assert!(result.status.success(), "bind canonical → go must succeed");
    let gaps: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("gaps.json")).unwrap()).unwrap();
    let gap_arr = gaps["gaps"].as_array().expect("gaps must be array");
    let stub_entries: Vec<&serde_json::Value> = gap_arr
        .iter()
        .filter(|g| g["kind"].as_str() == Some("bind-stub-body-emitted"))
        .collect();
    assert!(
        !stub_entries.is_empty(),
        "gaps.json must record at least one per-concept bind-stub-body-emitted entry; \
         no Go body templates exist in v1.0.0 so every concept should emit one. \
         Got: {:?}",
        gap_arr
    );
    // Each entry must name a specific concept in its detail (not the old
    // single-record-with-count-of-bindings shape).
    for entry in &stub_entries {
        let detail = entry["detail"].as_str().unwrap_or("");
        assert!(
            detail.contains("concept '"),
            "bind-stub-body-emitted entry must name a specific concept; got: {detail}"
        );
    }
}
