use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
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

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn tsx_cli() -> Option<PathBuf> {
    let path = repo_root()
        .join("node_modules")
        .join("tsx")
        .join("dist")
        .join("cli.mjs");
    path.exists().then_some(path)
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

fn canonical_cid(j: &Json) -> String {
    blake3_512_of(json_to_canonical_jcs(j).as_bytes())
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

fn typescript_shim_template() -> Json {
    json!({
        "kind": "block",
        "stmts": [{
            "kind": "return",
            "expr": {
                "kind": "method_call",
                "receiver": {"kind": "ident", "name": "client"},
                "method": "execute",
                "args": [
                    {"kind": "param_ref", "index": 1},
                    {"kind": "param_ref", "index": 2}
                ]
            }
        }]
    })
}

fn write_typescript_shim_proof(project: &Path, predicate: &str) -> String {
    let package_root = project
        .join("node_modules")
        .join("@provekit")
        .join("shim-fetch-lib");
    fs::create_dir_all(&package_root).expect("mkdir shim package");

    let template = typescript_shim_template();
    let template_cid = canonical_cid(&template);
    let target_contract = json!({
        "evidence": {
            "kind": "contract",
            "body": target_contract_body("fetchUrl", predicate),
        }
    });
    let (target_cid, target_bytes) = canonical_member(&target_contract);
    let binding = json!({
        "body": {
            "kind": "library-sugar-binding-entry",
            "target_language": "typescript",
            "target_library_tag": "fetch-lib",
            "concept_name": "concept:http-request",
            "family": "concept:family:http",
            "source_function_name": "fetchUrl",
            "param_names": ["url", "headers"],
            "param_types": ["string", "Headers"],
            "return_type": "Response",
            "body_source": {
                "file": "src/shim.ts",
                "span": {"start_line": 1, "start_col": 0, "end_line": 3, "end_col": 1},
                "source_cid": "blake3-512:".to_string() + &"1".repeat(128),
                "body_text": "return client.execute(url, headers);",
                "ast_template": template,
                "template_cid": template_cid,
                "param_names": ["url", "headers"]
            },
            "contract_cid": target_cid
        }
    });
    let (binding_cid, binding_bytes) = canonical_member(&binding);

    let mut members = BTreeMap::new();
    members.insert(target_cid, target_bytes);
    members.insert(binding_cid, binding_bytes);

    let signer_seed: Ed25519Seed = [0x74; 32];
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@provekit/shim-fetch-lib".to_string(),
        version: "0.1.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: "2026-05-31T00:00:00.000Z".into(),
    });
    fs::write(
        package_root.join(format!("{}.proof", proof.cid)),
        &proof.bytes,
    )
    .expect("write shim proof");
    fs::write(
        package_root.join("package.json"),
        format!(
            "{{\"name\":\"@provekit/shim-fetch-lib\",\"version\":\"0.1.0\",\"provekit\":{{\"proofHash\":\"{}\"}}}}\n",
            proof.cid
        ),
    )
    .expect("write shim package.json");
    proof.cid
}

fn stage_typescript_project(predicate: &str, user_body: &str) -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("src").join("user.ts"),
        format!("export function send(uri: string, h: Headers): Response {{\n  {user_body}\n}}\n"),
    )
    .expect("write user.ts");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("typescript-source"))
        .expect("mkdir typescript-source manifest dir");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "typescript-source"
surface = "typescript-source"
layer = "library-bindings"
"#,
    )
    .expect("write config.toml");

    let tsx = tsx_cli().expect("tsx CLI must exist; run pnpm install at repo root");
    let ts_source_bin = repo_root()
        .join("implementations")
        .join("typescript")
        .join("src")
        .join("lift")
        .join("typescript-source")
        .join("bin.ts");
    fs::write(
        provekit
            .join("lift")
            .join("typescript-source")
            .join("manifest.toml"),
        format!(
            "name = \"typescript-source\"\ncommand = [\"node\", \"{}\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            tsx.display(),
            ts_source_bin.display()
        ),
    )
    .expect("write typescript-source manifest");

    let target_proof_cid = write_typescript_shim_proof(&project, predicate);
    (temp, target_proof_cid)
}

fn run_recognize(project: &Path, write: bool) -> Json {
    let mut cmd = Command::new(provekit_bin());
    cmd.arg("recognize")
        .arg("--target")
        .arg("typescript")
        .arg("--project")
        .arg(project)
        .arg("--source")
        .arg("src/user.ts")
        .arg("--json");
    if write {
        cmd.arg("--write");
    }
    let output = cmd.output().expect("spawn provekit recognize");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "recognize failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    serde_json::from_str(&stdout).expect("recognize JSON receipt parses")
}

fn run_prove_json(project: &Path) -> (bool, Json, String, String) {
    let output = Command::new(provekit_bin())
        .arg("prove")
        .arg(project)
        .arg("--json")
        .output()
        .expect("spawn provekit prove");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let report: Json = serde_json::from_str(&stdout).unwrap_or_else(|_| {
        panic!("prove JSON report parses\nstdout:\n{stdout}\nstderr:\n{stderr}")
    });
    (output.status.success(), report, stdout, stderr)
}

fn assert_recognize_receipt_pinned(receipt: &Json, expected_target_proof_cid: &str) {
    let tags = receipt["tags"].as_array().expect("tags array");
    assert_eq!(tags.len(), 1, "TypeScript recognizer receipt:\n{receipt:#}");
    let tag = &tags[0];
    assert_eq!(tag["function_name"], "send");
    assert_eq!(tag["concept_name"], "concept:http-request");
    assert_eq!(tag["library_tag"], "fetch-lib");
    assert!(
        tag["contract_cid"].as_str().is_some_and(|s| !s.is_empty()),
        "tag must target the shim contract, not sibling fallback:\n{tag:#}"
    );
    assert_eq!(
        tag["target_proof_cid"].as_str(),
        Some(expected_target_proof_cid),
        "tag must carry the TypeScript-kit resolved proof bundle CID:\n{tag:#}"
    );
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
            "recognized TypeScript callsite discharged vacuously\nrow:\n{row:#}\nreport:\n{report:#}"
        );
    }
}

#[test]
fn typescript_recognize_write_self_resolves_package_proofs_and_proves() {
    if !node_available() || tsx_cli().is_none() {
        eprintln!("skipping TypeScript recognizer parity test: node/tsx unavailable");
        return;
    }
    if !z3_available() {
        eprintln!("skipping TypeScript recognizer parity test: z3 unavailable");
        return;
    }

    let (temp, target_proof_cid) = stage_typescript_project(">=", "return client.execute(uri, h);");
    let project = temp.path().join("project");

    let receipt = run_recognize(&project, true);
    assert_recognize_receipt_pinned(&receipt, &target_proof_cid);
    let bridge_proof = receipt["bridge_proof"]
        .as_str()
        .expect("recognize --write must mint a bridge proof");
    assert!(Path::new(bridge_proof).is_file(), "{bridge_proof}");

    let (ok, report, prove_stdout, prove_stderr) = run_prove_json(&project);
    assert!(
        ok,
        "prove should consume the TypeScript recognize bridge proof\nstdout:\n{prove_stdout}\nstderr:\n{prove_stderr}"
    );
    assert_eq!(report["totalCallsites"].as_u64(), Some(1), "{report:#}");
    assert_eq!(report["discharged"].as_u64(), Some(1), "{report:#}");
    assert_eq!(report["violations"].as_u64(), Some(0), "{report:#}");
    assert_no_vacuous_rows(&report);
}

#[test]
fn typescript_recognize_returns_no_tags_for_non_matching_source() {
    if !node_available() || tsx_cli().is_none() {
        eprintln!("skipping TypeScript recognizer non-match test: node/tsx unavailable");
        return;
    }

    let (temp, _target_proof_cid) = stage_typescript_project(">=", "return other.execute(uri, h);");
    let project = temp.path().join("project");

    let receipt = run_recognize(&project, false);
    assert_eq!(
        receipt["tags"].as_array().map(Vec::len),
        Some(0),
        "non-alpha-equivalent TypeScript source must not tag:\n{receipt:#}"
    );
}
