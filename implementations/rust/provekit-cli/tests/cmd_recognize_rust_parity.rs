use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use libprovekit::core::emit_obligation::member_envelope_canonical;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
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
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn rust_workspace_root() -> PathBuf {
    repo_root().join("implementations").join("rust")
}

fn walk_rpc_bin() -> PathBuf {
    static WALK_RPC: OnceLock<PathBuf> = OnceLock::new();
    WALK_RPC
        .get_or_init(|| {
            let bin_dir = provekit_bin()
                .parent()
                .expect("provekit bin parent")
                .to_path_buf();
            let candidate = bin_dir.join("provekit-walk-rpc");
            if !candidate.exists() {
                let release_profile =
                    bin_dir.file_name().and_then(|name| name.to_str()) == Some("release");
                let mut args = vec!["build", "-p", "provekit-walk", "--bin", "provekit-walk-rpc"];
                if release_profile {
                    args.push("--release");
                }
                let output = Command::new("cargo")
                    .current_dir(rust_workspace_root())
                    .args(args)
                    .output()
                    .expect("spawn cargo build -p provekit-walk --bin provekit-walk-rpc");
                assert!(
                    output.status.success(),
                    "cargo build -p provekit-walk --bin provekit-walk-rpc failed\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            assert!(
                candidate.exists(),
                "walk RPC binary missing: {}",
                candidate.display()
            );
            candidate
        })
        .clone()
}

fn write_rust_recognizer_manifest(project: &Path, walk_rpc: &Path) {
    let manifest_dir = project.join(".provekit").join("lift").join("rust");
    std::fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    std::fs::write(
        project.join(".provekit").join("config.toml"),
        r#"[[plugins]]
name = "rust-recognize"
kind = "recognize"
surface = "rust"
"#,
    )
    .expect("write config");
    std::fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"rust-recognize\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            walk_rpc.display()
        ),
    )
    .expect("write manifest");
}

fn walk_rpc_once(walk_rpc: &Path, method: &str, params: Json) -> Json {
    let mut child = Command::new(walk_rpc)
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provekit-walk-rpc");
    {
        let stdin = child.stdin.as_mut().expect("walk-rpc stdin");
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        writeln!(stdin, "{request}").expect("write walk-rpc request");
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"})
        )
        .expect("write walk-rpc shutdown");
    }

    let stdout = child.stdout.take().expect("walk-rpc stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read walk-rpc response");
    let mut stderr_pipe = child.stderr.take().expect("walk-rpc stderr");
    let wait = child.wait().expect("wait walk-rpc");
    let mut stderr = String::new();
    stderr_pipe
        .read_to_string(&mut stderr)
        .expect("read walk-rpc stderr");
    assert!(
        wait.success(),
        "walk-rpc exited nonzero\nresponse:\n{line}\nstderr:\n{stderr}"
    );
    let response: Json = serde_json::from_str(line.trim()).expect("walk-rpc response JSON");
    assert!(
        response.get("error").is_none(),
        "walk-rpc returned error: {response:#}"
    );
    response["result"].clone()
}

fn json_to_canonical_jcs(value: &Json) -> String {
    fn to_canonical(value: &Json) -> std::sync::Arc<CanonicalValue> {
        match value {
            Json::Null => CanonicalValue::null(),
            Json::Bool(value) => CanonicalValue::boolean(*value),
            Json::Number(value) => CanonicalValue::integer(value.as_i64().unwrap_or(0)),
            Json::String(value) => CanonicalValue::string(value.clone()),
            Json::Array(values) => CanonicalValue::array(values.iter().map(to_canonical).collect()),
            Json::Object(entries) => CanonicalValue::object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), to_canonical(value)))
                    .collect::<Vec<_>>(),
            ),
        }
    }
    encode_jcs(&to_canonical(value))
}

fn sugar_binding_member(decl: &Json) -> (String, Vec<u8>) {
    let source_cid = decl
        .pointer("/body_source/source_cid")
        .and_then(Json::as_str)
        .expect("sugar source cid");
    let envelope = json!({
        "body": decl,
        "header": {
            "bodySourceCid": source_cid,
            "conceptName": decl["concept_name"],
            "kind": "library-sugar-binding-entry",
            "signatureShapeCid": decl["signature_shape_cid"],
            "targetLanguage": decl["target_language"],
            "targetLibraryTag": decl["target_library_tag"],
        },
        "schemaVersion": "1",
    });
    let canonical = json_to_canonical_jcs(&envelope);
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
}

fn lift_sugar_entry(walk_rpc: &Path, root: &Path) -> Json {
    let result = walk_rpc_once(
        walk_rpc,
        "lift",
        json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }),
    );
    result["ir"]
        .as_array()
        .expect("lift ir array")
        .iter()
        .find(|entry| entry["kind"] == "library-sugar-binding-entry")
        .expect("sugar binding entry")
        .clone()
}

fn write_imported_sugar_proof(project: &Path, mut sugar_entry: Json) -> String {
    let target_contract_body = json!({
        "contractName": "recognize-target:json_parse",
        "formals": ["value"],
        "formalSorts": [{"kind": "primitive", "name": "Int"}],
        "pre": {
            "kind": "atomic",
            "name": "=",
            "args": [
                {"kind": "var", "name": "value"},
                {"kind": "var", "name": "value"}
            ]
        }
    });
    let (target_cid, target_bytes) =
        member_envelope_canonical("contract", &target_contract_body).expect("target contract");
    sugar_entry["contract_cid"] = Json::String(target_cid);
    let (binding_cid, binding_bytes) = sugar_binding_member(&sugar_entry);

    let mut members = BTreeMap::new();
    members.insert(binding_cid, binding_bytes);
    members.insert(
        sugar_entry["contract_cid"].as_str().unwrap().to_string(),
        target_bytes,
    );
    let signer_seed: Ed25519Seed = [0x93; 32];
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@test/rust-recognize-shim".to_string(),
        version: "0.1.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: "2026-05-31T00:00:00.000Z".to_string(),
    });
    let imports_dir = project.join(".provekit").join("imports");
    std::fs::create_dir_all(&imports_dir).expect("create imports dir");
    std::fs::write(
        imports_dir.join(format!("{}.proof", proof.cid)),
        &proof.bytes,
    )
    .expect("write imported proof");
    proof.cid
}

fn bridge_proof_carries_target_proof_cid(path: &Path, target_proof_cid: &str) -> bool {
    let bytes = std::fs::read(path).expect("read bridge proof");
    let catalog = cbor_decode(&bytes).expect("decode bridge proof");
    let members = catalog
        .as_map()
        .and_then(|root| root.get("members"))
        .and_then(CborValue::as_map)
        .expect("bridge proof members");
    members.values().any(|member| {
        let Some(member_bytes) = member.as_bstr() else {
            return false;
        };
        let Ok(parsed) = serde_json::from_slice::<Json>(member_bytes) else {
            return false;
        };
        parsed.pointer("/evidence/kind").and_then(Json::as_str) == Some("bridge")
            && parsed
                .pointer("/evidence/body/targetProofCid")
                .and_then(Json::as_str)
                == Some(target_proof_cid)
    })
}

#[test]
fn rust_recognize_write_self_resolves_imported_sugar_proof() {
    let walk_rpc = walk_rpc_bin();
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let shim = temp.path().join("shim");
    std::fs::create_dir_all(project.join("src")).expect("create project src");
    std::fs::create_dir_all(shim.join("src")).expect("create shim src");

    std::fs::write(
        shim.join("src").join("lib.rs"),
        r##"
#[provekit::sugar(concept = "concept:json-parse", library = "provekit-shim-serde-json-rust")]
pub fn json_parse(s: &str) -> i64 {
    serde_json::from_str(s)
}
"##,
    )
    .expect("write shim source");
    std::fs::write(
        project.join("src").join("lib.rs"),
        r##"
pub fn json_parse(input: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(input)
}
"##,
    )
    .expect("write user source");
    write_rust_recognizer_manifest(&project, &walk_rpc);
    let sugar_entry = lift_sugar_entry(&walk_rpc, &shim);
    let target_proof_cid = write_imported_sugar_proof(&project, sugar_entry);

    let output = Command::new(provekit_bin())
        .arg("recognize")
        .arg("--project")
        .arg(&project)
        .arg("--source")
        .arg("src/lib.rs")
        .arg("--write")
        .arg("--json")
        .output()
        .expect("spawn provekit recognize");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "recognize failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Json = serde_json::from_str(&stdout).expect("recognize JSON receipt");
    let tags = receipt["tags"].as_array().expect("tags array");
    assert_eq!(
        tags.len(),
        1,
        "Rust recognize must find the imported proof template via kit RPC\n{receipt:#}"
    );
    let tag = &tags[0];
    assert_eq!(tag["concept_name"], "concept:json-parse");
    assert_eq!(tag["library_tag"], "provekit-shim-serde-json-rust");
    assert_eq!(tag["target_proof_cid"], target_proof_cid);
    assert!(
        tag["contract_cid"]
            .as_str()
            .is_some_and(|cid| !cid.is_empty()),
        "recognize tag must cite the shim contract cid\n{tag:#}"
    );
    let bridge_proof = receipt["bridge_proof"]
        .as_str()
        .expect("recognize --write bridge proof");
    assert!(
        bridge_proof_carries_target_proof_cid(Path::new(bridge_proof), &target_proof_cid),
        "bridge proof must preserve the kit-resolved targetProofCid\n{receipt:#}"
    );
}
