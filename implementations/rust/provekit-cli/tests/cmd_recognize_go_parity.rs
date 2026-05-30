use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{
    build_proof_envelope, cbor_decode, ed25519_pubkey_string, CborValue, Ed25519Seed,
    ProofEnvelopeInput,
};
use serde_json::{json, Value as Json};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|_| panic!("mkdir {}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|_| panic!("read {}", src.display())) {
        let entry = entry.expect("read dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().expect("entry file type").is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap_or_else(|_| {
                panic!("copy {} -> {}", src_path.display(), dst_path.display())
            });
        }
    }
}

fn write_direct_go_recognizer_manifest(project: &Path) {
    let manifest = project.join(".provekit/lift/go-bind/manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir go-bind manifest dir");
    let working_dir = repo_root()
        .join("implementations")
        .join("go")
        .join("provekit-lift-go");
    fs::write(
        manifest,
        format!(
            "name = \"go-bind-lift\"\ncommand = [\"go\", \"run\", \"./cmd/provekit-lift-go\", \"--rpc\"]\nworking_dir = \"{}\"\n",
            working_dir.display()
        ),
    )
    .expect("write go-bind manifest");
}

fn copy_go_recognizer_demo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    copy_dir_recursive(
        &repo_root().join("examples").join("recognize-demo-go"),
        &project,
    );
    let _ = fs::remove_dir_all(project.join(".provekit/recognize"));
    write_direct_go_recognizer_manifest(&project);
    temp
}

fn int_sort() -> Json {
    json!({"kind": "primitive", "name": "Int"})
}

fn var(name: &str) -> Json {
    json!({"kind": "var", "name": name})
}

fn json_to_canonical_jcs(j: &Json) -> String {
    fn to_cv(j: &Json) -> std::sync::Arc<provekit_canonicalizer::Value> {
        use provekit_canonicalizer::Value as CV;
        match j {
            Json::Null => CV::null(),
            Json::Bool(b) => CV::boolean(*b),
            Json::Number(n) => CV::integer(n.as_i64().unwrap_or(0)),
            Json::String(s) => CV::string(s.clone()),
            Json::Array(items) => CV::array(items.iter().map(to_cv).collect()),
            Json::Object(map) => CV::object(
                map.iter()
                    .map(|(k, v)| (k.clone(), to_cv(v)))
                    .collect::<Vec<_>>(),
            ),
        }
    }
    encode_jcs(&to_cv(j))
}

fn canonical_member(env: &Json) -> (String, Vec<u8>) {
    let canonical = json_to_canonical_jcs(env);
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
}

fn target_contract_body(source_function_name: &str, predicate: &str) -> Json {
    json!({
        "contractName": format!("recognize-target:{source_function_name}"),
        "formals": ["value"],
        "formalSorts": [int_sort()],
        "pre": {
            "kind": "atomic",
            "name": predicate,
            "args": [var("value"), var("value")]
        }
    })
}

fn proof_members(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let catalog = cbor_decode(bytes).expect("decode fixture proof cbor");
    let members = catalog
        .as_map()
        .and_then(|m| m.get("members"))
        .and_then(CborValue::as_map)
        .expect("fixture proof has members map");
    members
        .iter()
        .map(|(cid, value)| {
            (
                cid.clone(),
                value
                    .as_bstr()
                    .unwrap_or_else(|| panic!("member {cid} is bstr"))
                    .to_vec(),
            )
        })
        .collect()
}

fn write_go_shim_proof_with_target_contracts(project: &Path, predicate: &str) -> String {
    let shim_dir = project.join("internal-shim-stdlib-http");
    let proof_paths = fs::read_dir(&shim_dir)
        .unwrap_or_else(|_| panic!("read {}", shim_dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("proof"))
        .collect::<Vec<_>>();
    assert_eq!(
        proof_paths.len(),
        1,
        "fixture should start with exactly one local shim proof: {proof_paths:?}"
    );

    let original = fs::read(&proof_paths[0]).expect("read original shim proof");
    let mut members = BTreeMap::new();
    let mut rewritten_bindings = 0usize;
    for (cid, member_bytes) in proof_members(&original) {
        let mut member: Json = serde_json::from_slice(&member_bytes)
            .unwrap_or_else(|_| panic!("member {cid} is JSON"));
        let is_sugar_binding = member.pointer("/body/kind").and_then(Json::as_str)
            == Some("library-sugar-binding-entry");
        if !is_sugar_binding {
            members.insert(cid, member_bytes);
            continue;
        }

        let source_function_name = member
            .pointer("/body/source_function_name")
            .and_then(Json::as_str)
            .unwrap_or("recognized");
        let target_contract = json!({
            "evidence": {
                "kind": "contract",
                "body": target_contract_body(source_function_name, predicate),
            }
        });
        let (target_cid, target_bytes) = canonical_member(&target_contract);
        member["body"]["contract_cid"] = json!(target_cid.clone());
        let (binding_cid, binding_bytes) = canonical_member(&member);
        members.insert(target_cid, target_bytes);
        members.insert(binding_cid, binding_bytes);
        rewritten_bindings += 1;
    }
    assert_eq!(
        rewritten_bindings, 2,
        "fixture should contain two sugar bindings to pin"
    );

    for path in proof_paths {
        fs::remove_file(&path).unwrap_or_else(|_| panic!("remove {}", path.display()));
    }

    let signer_seed: Ed25519Seed = [0x67; 32];
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@test/go-recognize-nonvacuous-shim".to_string(),
        version: "0.1.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: "2026-05-30T00:00:00.000Z".into(),
    });
    let path = shim_dir.join(format!("{}.proof", proof.cid));
    fs::write(&path, &proof.bytes).unwrap_or_else(|_| panic!("write {}", path.display()));
    proof.cid
}

fn copy_go_recognizer_demo_with_target_contracts(predicate: &str) -> (tempfile::TempDir, String) {
    let temp = copy_go_recognizer_demo();
    let project = temp.path().join("project");
    let target_proof_cid = write_go_shim_proof_with_target_contracts(&project, predicate);
    (temp, target_proof_cid)
}

fn run_recognize_write(project: &Path) -> Json {
    let recognize = Command::new(provekit_bin())
        .arg("recognize")
        .arg("--target")
        .arg("go")
        .arg("--project")
        .arg(project)
        .arg("--source")
        .arg("pkg/ingest/ingest.go")
        .arg("--source")
        .arg("pkg/persist/persist.go")
        .arg("--write")
        .arg("--json")
        .output()
        .expect("spawn provekit recognize");

    let stdout = String::from_utf8_lossy(&recognize.stdout);
    let stderr = String::from_utf8_lossy(&recognize.stderr);
    assert!(
        recognize.status.success(),
        "recognize failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    serde_json::from_str(&stdout).expect("recognize JSON receipt parses")
}

fn assert_recognize_receipt_pinned(receipt: &Json, expected_target_proof_cid: &str) {
    let tags = receipt["tags"]
        .as_array()
        .expect("recognize receipt has tags array");
    assert_eq!(
        tags.len(),
        2,
        "Go recognizer must resolve from project config, self-resolve the demo-local shim proof, and tag both user callsites without CLI proof paths\nreceipt:\n{receipt:#}"
    );
    for tag in tags {
        assert!(
            tag["contract_cid"].as_str().is_some_and(|s| !s.is_empty()),
            "recognizer tag must target a shim contract cid, not sibling fallback\n{tag:#}"
        );
        assert_eq!(
            tag["target_proof_cid"].as_str(),
            Some(expected_target_proof_cid),
            "recognizer tag must carry the kit-resolved proof bundle cid\n{tag:#}"
        );
    }
    let bridge_proof = receipt["bridge_proof"]
        .as_str()
        .expect("recognize --write must mint a bridge proof");
    assert!(
        Path::new(bridge_proof).is_file(),
        "recognize bridge proof path should exist: {bridge_proof}"
    );
}

fn run_prove_json(project: &Path) -> (bool, Json, String, String) {
    let prove = Command::new(provekit_bin())
        .arg("prove")
        .arg(project)
        .arg("--json")
        .output()
        .expect("spawn provekit prove");
    let stdout = String::from_utf8_lossy(&prove.stdout).to_string();
    let stderr = String::from_utf8_lossy(&prove.stderr).to_string();
    let report: Json = serde_json::from_str(&stdout).unwrap_or_else(|_| {
        panic!("prove JSON report parses\nstdout:\n{stdout}\nstderr:\n{stderr}")
    });
    (prove.status.success(), report, stdout, stderr)
}

fn assert_no_vacuous_rows(report: &Json) {
    let rows = report["rows"].as_array().expect("prove report has rows");
    assert!(
        !rows.is_empty(),
        "prove report must contain rows\n{report:#}"
    );
    for row in rows {
        let reason = row["reason"].as_str().unwrap_or_default();
        assert!(
            !reason.contains("vacuous")
                && !reason.contains("no precondition")
                && !reason.contains("back-compat"),
            "recognized callsite discharged vacuously\nrow:\n{row:#}\nreport:\n{report:#}"
        );
    }
}

#[test]
fn go_recognize_write_self_resolves_project_proofs_and_proves() {
    if !go_available() {
        eprintln!("skipping Go recognizer parity test: go toolchain unavailable");
        return;
    }

    let (temp, target_proof_cid) = copy_go_recognizer_demo_with_target_contracts(">=");
    let project = temp.path().join("project");

    let receipt = run_recognize_write(&project);
    assert_recognize_receipt_pinned(&receipt, &target_proof_cid);

    let (ok, report, prove_stdout, prove_stderr) = run_prove_json(&project);
    assert!(
        ok,
        "prove should consume the recognize bridge proof\nstdout:\n{prove_stdout}\nstderr:\n{prove_stderr}"
    );
    assert_eq!(report["totalCallsites"].as_u64(), Some(2), "{report:#}");
    assert_eq!(report["discharged"].as_u64(), Some(2), "{report:#}");
    assert_eq!(report["violations"].as_u64(), Some(0), "{report:#}");
    assert_no_vacuous_rows(&report);
}

#[test]
fn go_recognize_write_rejects_unmet_target_obligation() {
    if !go_available() {
        eprintln!("skipping Go recognizer contradiction test: go toolchain unavailable");
        return;
    }

    let (temp, target_proof_cid) = copy_go_recognizer_demo_with_target_contracts(">");
    let project = temp.path().join("project");

    let receipt = run_recognize_write(&project);
    assert_recognize_receipt_pinned(&receipt, &target_proof_cid);

    let (ok, report, _stdout, _stderr) = run_prove_json(&project);
    assert!(
        !ok,
        "unmet recognized target precondition must fail\n{report:#}"
    );
    assert_eq!(report["totalCallsites"].as_u64(), Some(2), "{report:#}");
    assert_eq!(report["discharged"].as_u64(), Some(0), "{report:#}");
    assert!(
        report["violations"].as_u64().unwrap_or_default() > 0,
        "unmet recognized target precondition must report violations\n{report:#}"
    );
    assert_no_vacuous_rows(&report);
}
