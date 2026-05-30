// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::Value as Json;

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

fn node_bin() -> String {
    std::env::var("NODE").unwrap_or_else(|_| "node".to_string())
}

fn install_node_manifest(root: &Path, surface: &str, script: &Path, library_tag: &str) {
    install_node_manifest_with_metadata(root, surface, script, library_tag, None, &[]);
}

fn append_realize_registration(root: &Path, name: &str, surface: &str) {
    let provekit_dir = root.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("create .provekit dir");
    let config = provekit_dir.join("config.toml");
    let mut text = fs::read_to_string(&config).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "\n[[plugins]]\nname = \"{name}\"\nkind = \"realize\"\nsurface = \"{surface}\"\n"
    ));
    fs::write(config, text).expect("write realize config registration");
}

fn install_node_manifest_with_metadata(
    root: &Path,
    surface: &str,
    script: &Path,
    library_tag: &str,
    family: Option<&str>,
    provides_concepts: &[&str],
) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let script = script
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut manifest_text = format!(
        "name = \"typescript-realize-{library_tag}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"{}\", \"{}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
        node_bin().replace('\\', "\\\\").replace('"', "\\\""),
        script,
    );
    append_manifest_metadata(&mut manifest_text, family, provides_concepts);
    fs::write(manifest, manifest_text).expect("write manifest");
    append_realize_registration(root, &format!("typescript-realize-{library_tag}"), surface);
}

fn append_manifest_metadata(
    manifest_text: &mut String,
    family: Option<&str>,
    provides_concepts: &[&str],
) {
    if let Some(family) = family {
        manifest_text.push_str(&format!("family = \"{family}\"\n"));
    }
    if !provides_concepts.is_empty() {
        let concepts = provides_concepts
            .iter()
            .map(|concept| format!("\"{concept}\""))
            .collect::<Vec<_>>()
            .join(", ");
        manifest_text.push_str(&format!("provides_concepts = [{concepts}]\n"));
    }
}

fn install_python_script_manifest_with_metadata(
    root: &Path,
    surface: &str,
    script: &Path,
    library_tag: &str,
    family: Option<&str>,
    provides_concepts: &[&str],
) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let script = script
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut manifest_text = format!(
        "name = \"typescript-realize-{library_tag}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"python3\", \"{}\"]\n\
         working_dir = \".\"\n",
        script,
    );
    append_manifest_metadata(&mut manifest_text, family, provides_concepts);
    fs::write(manifest, manifest_text).expect("write manifest");
    append_realize_registration(root, &format!("python-realize-{library_tag}"), surface);
}

fn install_binary_manifest(
    root: &Path,
    surface: &str,
    binary: &Path,
    manifest_name: &str,
    library_tag: &str,
) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let binary = binary
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let manifest_text = format!(
        "name = \"{manifest_name}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"{binary}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
    );
    fs::write(manifest, manifest_text).expect("write manifest");
    append_realize_registration(root, manifest_name, surface);
}

fn install_python_module_manifest(
    root: &Path,
    surface: &str,
    module: &str,
    pythonpath: &[PathBuf],
    manifest_name: &str,
    library_tag: &str,
) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let pythonpath = pythonpath
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let manifest_text = format!(
        "name = \"{manifest_name}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"env\", \"PYTHONPATH={pythonpath}\", \"python3\", \"-m\", \"{module}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
    );
    fs::write(manifest, manifest_text).expect("write manifest");
    append_realize_registration(root, manifest_name, surface);
}

fn write_typescript_project_fixture(workspace: &Path) -> PathBuf {
    let repo = repo_root();
    install_node_manifest(
        workspace,
        "typescript-better-sqlite3",
        &repo
            .join("implementations")
            .join("typescript")
            .join("provekit-realize-typescript-better-sqlite3")
            .join("src")
            .join("main.js"),
        "better-sqlite3",
    );
    fs::write(workspace.join("package.json"), "{\"type\":\"module\"}\n")
        .expect("write package marker");
    let src_dir = workspace.join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    src_dir
}

fn write_python_requests_project_fixture(workspace: &Path) -> Option<PathBuf> {
    let repo = repo_root();
    let package_src = repo
        .join("implementations")
        .join("python")
        .join("provekit-realize-python-requests")
        .join("src");
    let core_src = repo
        .join("implementations")
        .join("python")
        .join("provekit-realize-python-core")
        .join("src");
    let shim_src = repo.join("examples").join("provekit-shim-python-requests");
    if !package_src.is_dir() || !core_src.is_dir() || !shim_src.is_dir() {
        return None;
    }
    install_python_module_manifest(
        workspace,
        "python-requests",
        "provekit_realize_python_requests",
        &[core_src, package_src, shim_src],
        "python-realize-requests",
        "requests",
    );
    fs::write(
        workspace.join("pyproject.toml"),
        "[project]\nname = \"materialize-python-example\"\nversion = \"0.0.0\"\n",
    )
    .expect("write python project marker");
    let src_dir = workspace.join("src");
    fs::create_dir_all(&src_dir).expect("create python src dir");
    Some(src_dir)
}

fn write_rust_reqwest_project_fixture(workspace: &Path) -> Option<PathBuf> {
    let repo = repo_root();
    let binary = repo
        .join("implementations")
        .join("rust")
        .join("target")
        .join("debug")
        .join("provekit-realize-rust");
    if !binary.exists() {
        return None;
    }
    install_binary_manifest(workspace, "rust", &binary, "rust-realize", "reqwest");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"materialize-rust-example\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write rust project marker");
    let src_dir = workspace.join("src");
    fs::create_dir_all(&src_dir).expect("create rust src dir");
    Some(src_dir)
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

fn rewrite_python_realize_manifest(manifest: &Path) {
    let core_src = repo_root()
        .join("implementations")
        .join("python")
        .join("provekit-realize-python-core")
        .join("src");
    let pythonpath = core_src
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let text = fs::read_to_string(manifest)
        .unwrap_or_else(|_| panic!("read checked-in manifest {}", manifest.display()));
    let rewritten = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("command = ") {
                format!(
                    "command = [\"env\", \"PYTHONPATH={pythonpath}\", \"python3\", \"-m\", \"provekit_realize_python_core\", \"--rpc\"]"
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(manifest, format!("{rewritten}\n"))
        .unwrap_or_else(|_| panic!("write manifest {}", manifest.display()));
}

fn concept_carrier_lines(indent: &str) -> String {
    format!(
        "{indent}// provekit-concept: {}\n{indent}// provekit-concept-payload-cid: {}\n",
        concept_payload_json(),
        concept_payload_cid()
    )
}

fn concept_payload_json() -> &'static str {
    "{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"concept:sql-query-all\",\"function\":\"selectRows\",\"params\":[\"sql\",\"args\"],\"param_types\":[\"string\",\"unknown[]\"],\"return_type\":\"unknown[]\",\"named_term_tree\":{\"conceptName\":\"concept:sql-query-all\",\"args\":[{\"sort\":\"Sql\",\"source\":\"sql\"},{\"sort\":\"SqlArgs\",\"source\":\"args\"}]}}"
}

fn concept_payload_cid() -> String {
    payload_cid(concept_payload_json())
}

fn payload_cid(payload: &str) -> String {
    let json: Json = serde_json::from_str(payload).expect("payload json parses");
    let canonical = canonical_value_from_json(&json);
    blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes())
}

fn flat_member(mut env: Json) -> (String, Vec<u8>) {
    if let Json::Object(map) = &mut env {
        map.remove("cid");
        map.remove("producerSignature");
    }
    let canonical = encode_jcs(canonical_value_from_json(&env).as_ref());
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
}

fn int_const(value: i64) -> Json {
    serde_json::json!({
        "kind": "const",
        "value": value,
        "sort": {"kind": "primitive", "name": "Int"},
    })
}

fn publish_materialize_contract_fixture(project: &Path, boundary_fn: &str) -> String {
    let proof_dir = project.join(".provekit");
    fs::create_dir_all(&proof_dir).expect("create .provekit");
    let mut members = BTreeMap::new();

    let target_env = serde_json::json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": format!("{boundary_fn}_vendor_contract"),
                "pre": {
                    "kind": "forall",
                    "name": "x",
                    "sort": {"kind": "primitive", "name": "Int"},
                    "body": {
                        "kind": "atomic",
                        "name": ">",
                        "args": [
                            {"kind": "var", "name": "x"},
                            int_const(0),
                        ],
                    },
                },
            },
        },
    });
    let (target_cid, target_bytes) = flat_member(target_env);
    members.insert(target_cid.clone(), target_bytes);

    let source_env = serde_json::json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "materialized_boundary_consumer",
                "post": {
                    "kind": "atomic",
                    "name": "=",
                    "args": [
                        {"kind": "var", "name": "out"},
                        {"kind": "ctor", "name": boundary_fn, "args": [int_const(0)]},
                    ],
                },
            },
        },
    });
    let (source_cid, source_bytes) = flat_member(source_env);
    members.insert(source_cid, source_bytes);

    let signer_seed: Ed25519Seed = [0x42; 32];
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@test/materialize-contract-fixture".to_string(),
        version: "1.0.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: "2026-05-27T00:00:00.000Z".to_string(),
    });
    fs::write(proof_dir.join(format!("{}.proof", proof.cid)), &proof.bytes).expect("write proof");
    target_cid
}

fn write_contract_materialize_source(src_dir: &Path, contract_cid: &str) -> PathBuf {
    let source_path = src_dir.join("lib.rs");
    let payload = serde_json::json!({
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_name": "concept:must-be-positive",
        "function": "must_be_positive",
        "params": ["x"],
        "param_types": ["i64"],
        "return_type": "i64",
        "contract": {
            "concept_site_cid": format!("blake3-512:{}", "1".repeat(128)),
            "object_fcm_cid": format!("blake3-512:{}", "2".repeat(128)),
            "local_contract_cid": contract_cid,
            "origin": "vendor-fixture",
            "discharge_verdict": "accepted",
            "witnesses": [],
        },
    });
    let payload = serde_json::to_string(&payload).expect("payload serializes");
    fs::write(
        &source_path,
        format!(
            "{}pub fn must_be_positive(x: i64) -> i64 {{\n    0\n}}\n",
            carrier_lines("//", "", &payload)
        ),
    )
    .expect("write contract materialize source");
    source_path
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_verify_json_with_code(project: &Path) -> (Json, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("verify")
        .arg("--project")
        .arg(project)
        .arg("--json")
        .output()
        .expect("spawn provekit verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("verify JSON parse failed: {error}\nstdout: {stdout}"));
    (receipt, out.status.code().unwrap_or(-1))
}

fn http_payload_json(function: &str, param_type: &str, return_type: &str) -> String {
    format!(
        "{{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"concept:http-request\",\"function\":\"{function}\",\"params\":[\"url\"],\"param_types\":[\"{param_type}\"],\"return_type\":\"{return_type}\"}}"
    )
}

fn carrier_lines(comment_prefix: &str, indent: &str, payload: &str) -> String {
    format!(
        "{indent}{comment_prefix} provekit-concept: {payload}\n{indent}{comment_prefix} provekit-concept-payload-cid: {}\n",
        payload_cid(payload)
    )
}

fn canonical_value_from_json(value: &Json) -> Arc<CanonicalValue> {
    match value {
        Json::Null => CanonicalValue::null(),
        Json::Bool(value) => CanonicalValue::boolean(*value),
        Json::Number(value) => {
            CanonicalValue::integer(value.as_i64().expect("test JSON uses integers only"))
        }
        Json::String(value) => CanonicalValue::string(value),
        Json::Array(values) => {
            CanonicalValue::array(values.iter().map(canonical_value_from_json).collect())
        }
        Json::Object(entries) => CanonicalValue::object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), canonical_value_from_json(value))),
        ),
    }
}

fn block_comment_concept_carrier_lines(indent: &str) -> String {
    format!(
        "{indent}/* provekit-concept: {} */\n{indent}/* provekit-concept-payload-cid: {} */\n",
        concept_payload_json(),
        concept_payload_cid()
    )
}

fn write_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("queries.ts");
    fs::write(
        &source_path,
        format!(
            "// header stays\n{}// footer stays\n",
            concept_carrier_lines("")
        ),
    )
    .expect("write source");
    source_path
}

fn write_indented_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("nested.ts");
    fs::write(
        &source_path,
        format!(
            "export function wrapper() {{\n{}  return true;\n}}\n",
            concept_carrier_lines("  ")
        ),
    )
    .expect("write indented source");
    source_path
}

fn write_block_comment_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("block.ts");
    fs::write(
        &source_path,
        format!(
            "// header stays\n{}// footer stays\n",
            block_comment_concept_carrier_lines("")
        ),
    )
    .expect("write block comment source");
    source_path
}

fn write_malformed_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("bad.ts");
    fs::write(
        &source_path,
        "// provekit-concept: {not json}\n// provekit-concept-payload-cid: blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )
    .expect("write malformed source");
    source_path
}

fn write_malformed_dependency_source(src_dir: &Path) -> PathBuf {
    let dependency_dir = src_dir.join("node_modules").join("bad-package");
    fs::create_dir_all(&dependency_dir).expect("create dependency dir");
    let source_path = dependency_dir.join("index.js");
    fs::write(
        &source_path,
        "// provekit-concept: {not json from dependency}\n",
    )
    .expect("write malformed dependency source");
    source_path
}

fn write_mismatched_cid_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("mismatch.ts");
    fs::write(
        &source_path,
        format!(
            "// provekit-concept: {}\n// provekit-concept-payload-cid: blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            concept_payload_json()
        ),
    )
    .expect("write mismatched CID source");
    source_path
}

fn write_no_carrier_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("plain.ts");
    fs::write(&source_path, "export const untouched = 42;\n").expect("write plain source");
    source_path
}

fn write_python_http_request_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("client.py");
    let payload = http_payload_json("fetch_status", "str", "int");
    fs::write(
        &source_path,
        format!(
            "# python materialize example\n{}# end\n",
            carrier_lines("#", "", &payload)
        ),
    )
    .expect("write python HTTP source");
    source_path
}

fn write_python_identity_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("identity.py");
    let payload = "{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"identity\",\"function\":\"identity_value\",\"params\":[\"x\"],\"param_types\":[\"int\"],\"return_type\":\"int\"}";
    fs::write(
        &source_path,
        format!(
            "{}def placeholder():\n    pass\n",
            carrier_lines("#", "", payload)
        ),
    )
    .expect("write python identity source");
    source_path
}

fn write_materialize_check_rpc_kit(path: &Path) {
    fs::write(
        path,
        r#"import json
import pathlib
import sys

for line in sys.stdin:
    request = json.loads(line)
    msg_id = request.get("id")
    method = request.get("method")
    if method == "provekit.plugin.invoke":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "source": "def fetch_status(url):\n    return 200\n",
                "is_stub": False,
                "extension": "py",
                "imports": [],
                "emitted_artifact_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            }
        }), flush=True)
    elif method == "provekit.plugin.assemble":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "files": [{
                    "path": "client.py",
                    "content": "def fetch_status(url):\n    return 200\n"
                }],
                "compile_classpath": ["kit-owned-classpath"]
            }
        }), flush=True)
    elif method == "provekit.plugin.check":
        params = request.get("params") or {}
        out_dir = pathlib.Path(params["out_dir"])
        (out_dir / "checked-by-materialize-kit.txt").write_text(
            json.dumps(params, sort_keys=True),
            encoding="utf-8",
        )
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "ok": True,
                "command": "fake materialize kit check",
                "classpath": params.get("compile_classpath", [])
            }
        }), flush=True)
    elif method == "provekit.plugin.shutdown":
        print(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": None}), flush=True)
        break
    else:
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {"code": -32601, "message": "METHOD_NOT_FOUND: " + str(method)}
        }), flush=True)
"#,
    )
    .expect("write fake materialize check kit");
}

fn write_materialize_no_assemble_rpc_kit(path: &Path) {
    fs::write(
        path,
        r#"import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    msg_id = request.get("id")
    method = request.get("method")
    if method == "provekit.plugin.invoke":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "source": "def fetch_status(url):\n    return 200\n",
                "is_stub": False,
                "extension": "py",
                "imports": [],
                "emitted_artifact_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
            }
        }), flush=True)
    elif method == "provekit.plugin.shutdown":
        print(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": None}), flush=True)
        break
    else:
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {"code": -32601, "message": "METHOD_NOT_FOUND: " + str(method)}
        }), flush=True)
"#,
    )
    .expect("write fake no-assemble materialize kit");
}

fn write_rust_http_request_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("lib.rs");
    let payload = http_payload_json("fetch_status", "&str", "i64");
    fs::write(
        &source_path,
        format!(
            "// rust materialize example\n{}// end\n",
            carrier_lines("//", "", &payload)
        ),
    )
    .expect("write rust HTTP source");
    source_path
}

#[test]
fn materialize_dry_run_replaces_concept_citation_with_realized_library_source() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("// file: queries.ts"),
        "stdout should name the file: {stdout}"
    );
    assert!(stdout.contains("// header stays"));
    assert!(
        stdout.contains("db.prepare(sql).all(args)"),
        "stdout should contain better-sqlite3 materialization:\n{stdout}"
    );
    assert!(stdout.contains("// footer stays"));
    assert!(
        fs::read_to_string(&source_path)
            .expect("read original")
            .contains("provekit-concept:"),
        "dry run must not rewrite source files"
    );
}

#[test]
fn materialize_write_rewrites_source_file_in_place_and_reports_summary() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--write")
        .output()
        .expect("spawn provekit materialize --write");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize --write should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("materialized 1 exact + 0 lossy + 0 refused across 1 file(s)"),
        "write mode should report replacement summary: {stdout}"
    );
    let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
    assert!(rewritten.contains("// header stays"));
    assert!(
        rewritten.contains("db.prepare(sql).all(args)"),
        "rewritten file should contain better-sqlite3 materialization:\n{rewritten}"
    );
    assert!(rewritten.contains("// footer stays"));
    assert!(
        !rewritten.contains("provekit-concept:"),
        "write mode should remove concept citation carrier comments:\n{rewritten}"
    );
    assert!(
        !rewritten.contains("provekit-concept-payload-cid:"),
        "write mode should remove payload CID carrier comments:\n{rewritten}"
    );
}

#[test]
fn materialize_write_emits_contract_bridge_that_verify_refuses_on_violation() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping materialize contract bridge e2e");
        return;
    }

    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname = \"materialize-contract-bridge\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write cargo marker");

    let vendor_contract_cid =
        publish_materialize_contract_fixture(workspace.path(), "must_be_positive");
    let source_path = write_contract_materialize_source(&src_dir, &vendor_contract_cid);

    let fake_realize = workspace.path().join("fake_realize_contract.py");
    fs::write(
        &fake_realize,
        r#"import json, sys
for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps({
        "jsonrpc": "2.0",
        "id": request.get("id"),
        "result": {
            "source": "fn must_be_positive(x: i64) -> i64 { x }",
            "is_stub": False,
            "extension": "rs",
            "emitted_artifact_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }
    }), flush=True)
"#,
    )
    .expect("write fake realize contract script");
    install_python_script_manifest_with_metadata(
        workspace.path(),
        "rust-vendor",
        &fake_realize,
        "vendor",
        None,
        &["concept:must-be-positive"],
    );

    let (before, before_code) = run_verify_json_with_code(workspace.path());
    assert_eq!(
        before_code, 1,
        "pre-materialize verify should reject the empty proof as non-success: {before}"
    );
    assert_eq!(
        before["totalClaims"], 0,
        "without the materialize bridge the boundary call should not enumerate: {before}"
    );
    assert_eq!(
        before["ok"], false,
        "zero-claim verification must not be reported as ok: {before}"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("rust")
        .arg("--library")
        .arg("vendor")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--write")
        .output()
        .expect("spawn provekit materialize --write");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize --write should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
    assert!(
        rewritten.contains("pub fn must_be_positive(x: i64) -> i64"),
        "materialize should preserve the boundary signature:\n{rewritten}"
    );

    let pool = provekit_verifier::load_all_proofs::run(workspace.path());
    assert!(
        pool.load_errors.is_empty(),
        "materialize bridge proof should load cleanly: {:?}",
        pool.load_errors
    );
    let bridge = pool
        .bridges_by_symbol
        .get("must_be_positive")
        .unwrap_or_else(|| {
            panic!(
                "materialize must write a bridge for the boundary; indexed symbols: {:?}",
                pool.bridges_by_symbol.keys().collect::<Vec<_>>()
            )
        });
    let body = provekit_verifier::types::memento_body(bridge).expect("bridge body");
    assert_eq!(
        body.get("sourceContractCid").and_then(Json::as_str),
        Some(vendor_contract_cid.as_str())
    );
    assert_eq!(
        body.get("targetContractCid").and_then(Json::as_str),
        Some(vendor_contract_cid.as_str())
    );
    assert_eq!(
        body.pointer("/target/cid").and_then(Json::as_str),
        Some(vendor_contract_cid.as_str())
    );
    assert!(
        pool.mementos.contains_key(&vendor_contract_cid),
        "bridge target must resolve to the vendor contract memento"
    );

    let (after, after_code) = run_verify_json_with_code(workspace.path());
    assert_eq!(
        after_code, 1,
        "violating materialized boundary must be refused\nreceipt: {after}"
    );
    assert_eq!(after["totalClaims"], 1, "receipt: {after}");
    let claim = &after["claims"].as_array().expect("claims")[0];
    assert_eq!(claim["status"], "unsatisfied", "claim: {claim}");
    assert_eq!(claim["pass"], false, "claim: {claim}");
}

#[test]
fn materialize_out_dir_writes_materialized_copy_and_leaves_source_unchanged() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_concept_source(&src_dir);
    let out_dir = workspace.path().join("materialized");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("spawn provekit materialize --out-dir");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize --out-dir should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("materialized 1 exact + 0 lossy + 0 refused across 1 file(s)"),
        "out-dir mode should report replacement summary: {stdout}"
    );
    let copied = fs::read_to_string(out_dir.join("queries.ts")).expect("read materialized copy");
    assert!(copied.contains("db.prepare(sql).all(args)"));
    assert!(!copied.contains("provekit-concept:"));
    let original = fs::read_to_string(&source_path).expect("read original source");
    assert!(
        original.contains("provekit-concept:"),
        "out-dir mode must not rewrite source file: {original}"
    );
}

#[test]
fn materialize_preserves_carrier_indentation_when_replacing_source() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_indented_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--write")
        .output()
        .expect("spawn provekit materialize --write");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize --write should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
    assert!(
        rewritten.contains("\n  function selectRows"),
        "replacement should start at the carrier's indentation level:\n{rewritten}"
    );
    assert!(
        rewritten.contains("\n    return db.prepare(sql).all(args);"),
        "replacement body indentation should be offset from the carrier indentation:\n{rewritten}"
    );
    assert!(
        rewritten.contains("\n  }\n  return true;"),
        "replacement closing brace should preserve carrier indentation and following code:\n{rewritten}"
    );
}

#[test]
fn materialize_accepts_single_line_block_comment_carriers() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_block_comment_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--write")
        .output()
        .expect("spawn provekit materialize --write");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize should accept block-comment carriers\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
    assert!(rewritten.contains("db.prepare(sql).all(args)"));
    assert!(!rewritten.contains("provekit-concept:"));
    assert!(!rewritten.contains("*/"));
}

#[test]
fn materialize_malformed_carrier_error_names_source_file() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    write_malformed_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "malformed carrier should fail\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("bad.ts"),
        "error should identify the source file with the malformed carrier:\n{stderr}"
    );
    assert!(
        stderr.contains("parse provekit-concept payload JSON"),
        "error should preserve the JSON parse detail:\n{stderr}"
    );
}

#[test]
fn materialize_ignores_dependency_directories_when_scanning_sources() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    write_concept_source(&src_dir);
    write_malformed_dependency_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize should ignore malformed carriers under dependency directories\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("// file: queries.ts"));
    assert!(stdout.contains("db.prepare(sql).all(args)"));
    assert!(
        !stdout.contains("node_modules"),
        "dependency files should not appear in materialize output:\n{stdout}"
    );
}

#[test]
fn materialize_rejects_payload_cid_mismatch() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    write_mismatched_cid_concept_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "CID mismatch should fail\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("mismatch.ts"),
        "CID mismatch error should name the source file:\n{stderr}"
    );
    assert!(
        stderr.contains("provekit-concept-payload-cid mismatch"),
        "CID mismatch error should explain the mismatch:\n{stderr}"
    );
}

#[test]
fn materialize_no_carriers_reports_zero_without_printing_dry_run_source() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = write_typescript_project_fixture(workspace.path());
    let source_path = write_no_carrier_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "no-carrier materialize should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout, "",
        "dry-run no-carrier mode should not print source"
    );
    assert!(
        stderr.contains("found 0 concept citation(s)"),
        "no-carrier mode should explain why no files were printed:\n{stderr}"
    );
    assert_eq!(
        fs::read_to_string(source_path).expect("read plain source"),
        "export const untouched = 42;\n"
    );
}

#[test]
fn materialize_without_target_or_registered_manifest_refuses_project_marker_inference() {
    let workspace = tempfile::tempdir().expect("tempdir");
    fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname = \"marker-only\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write rust project marker");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    let _source_path = write_no_carrier_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("reqwest")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "marker-only materialize must not infer a target without a registered realize manifest\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("could not infer target language for library `reqwest`"),
        "error should require explicit target or manifest-backed dispatch:\n{stderr}"
    );
}

#[test]
fn materialize_python_requests_example_uses_python_library_shim() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let Some(src_dir) = write_python_requests_project_fixture(workspace.path()) else {
        eprintln!("skipping Python materialize example: provekit-realize-python-requests binary is unavailable");
        return;
    };
    write_python_http_request_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", repo_root())
        .arg("materialize")
        .arg("--library")
        .arg("python-requests")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize for Python requests");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Python requests materialize example should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("// file: client.py"));
    assert!(
        stdout.contains("requests.get(url)"),
        "Python requests example should route through the requests shim:\n{stdout}"
    );
    assert!(!stdout.contains("provekit-concept:"));
}

#[test]
fn materialize_python_uses_checked_in_python_double_realize_registration() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("python-double");
    fs::create_dir_all(&project).expect("mkdir project");
    copy_dir_recursive(
        &repo_root()
            .join("examples")
            .join("python-double")
            .join(".provekit"),
        &project.join(".provekit"),
    );
    rewrite_python_realize_manifest(
        &project
            .join(".provekit")
            .join("realize")
            .join("python")
            .join("manifest.toml"),
    );
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"checked-in-python-materialize\"\nversion = \"0.0.0\"\n",
    )
    .expect("write pyproject");
    let src_dir = project.join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_python_identity_source(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("python")
        .arg("--library")
        .arg("python")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(&project)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for checked-in Python fixture");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "checked-in Python fixture registration must drive materialize\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assembled by python kit via RPC"),
        "Python materialize route must assemble through the Python kit\nstderr:\n{stderr}"
    );
    let emitted =
        fs::read_to_string(out_dir.join("identity.py")).expect("read materialized Python");
    assert!(
        emitted.contains("def identity_value(x):") && emitted.contains("return x"),
        "materialized Python should contain identity body from checked-in registration:\n{emitted}"
    );
}

#[test]
fn materialize_rust_reqwest_example_uses_rust_library_shim() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let Some(src_dir) = write_rust_reqwest_project_fixture(workspace.path()) else {
        eprintln!("skipping Rust materialize example: provekit-realize-rust binary is unavailable; build with `cargo build -p provekit-realize-rust-core`");
        return;
    };
    write_rust_http_request_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", repo_root())
        .arg("materialize")
        .arg("--library")
        .arg("rust-reqwest")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize for Rust reqwest");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Rust reqwest materialize example should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("// file: lib.rs"));
    assert!(
        stdout.contains("reqwest::get(url)"),
        "Rust reqwest example should route through the Rust reqwest shim:\n{stdout}"
    );
    assert!(!stdout.contains("provekit-concept:"));
}

#[test]
fn materialize_explicit_target_strips_redundant_language_prefix_from_library() {
    // N1 regression: --target python --library python-requests previously produced
    // a duplicated python-python-requests realization surface, which no
    // manifest matches.
    // After the fix, resolve_library_surface strips the "python-" prefix and
    // resolves the python-requests surface, matching the installed plugin.
    let workspace = tempfile::tempdir().expect("tempdir");
    let Some(src_dir) = write_python_requests_project_fixture(workspace.path()) else {
        eprintln!(
            "skipping N1 prefix-strip test: provekit-realize-python-requests binary is unavailable"
        );
        return;
    };
    write_python_http_request_source(&src_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", repo_root())
        .arg("materialize")
        .arg("--target")
        .arg("python")
        .arg("--library")
        .arg("python-requests")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize with explicit target and prefixed library");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "explicit --target python --library python-requests should succeed after prefix strip\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("requests.get(url)"),
        "result should route through the python-requests shim: {stdout}"
    );
    assert!(!stdout.contains("provekit-concept:"));
}

// --- compile-check tests (#1376) ---

/// --compile-check without --out-dir must be rejected by clap (requires = "out_dir").
/// Exit code 2 is EXIT_USER_ERROR (clap writes to stderr and exits 2 for usage errors).
#[test]
fn compile_check_without_out_dir_is_user_error() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize --compile-check without --out-dir");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "--compile-check without --out-dir must exit 2 (user error)\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--out-dir"),
        "clap error should mention --out-dir as missing required argument\nstderr:\n{stderr}"
    );
}

/// End-to-end: materialize python with --out-dir + --compile-check.
/// The python realize kit owns the native py_compile check over the emitted file.
#[test]
fn compile_check_passes_for_valid_python_materialized_output() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let Some(src_dir) = write_python_requests_project_fixture(workspace.path()) else {
        eprintln!(
            "skipping compile-check python test: provekit-realize-python-requests binary is unavailable"
        );
        return;
    };
    write_python_http_request_source(&src_dir);
    let out_dir = workspace.path().join("compiled-out");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", repo_root())
        .arg("materialize")
        .arg("--target")
        .arg("python")
        .arg("--library")
        .arg("python-requests")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize --compile-check for python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "--compile-check over valid python output should exit 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("compile-check: python -m py_compile passed"),
        "stderr should confirm the python kit check passed\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assembled by python kit via RPC"),
        "Python materialize must use the configured Python kit for assembly, not legacy concat fallback\nstderr:\n{stderr}"
    );
    let emitted = fs::read_to_string(out_dir.join("client.py")).expect("read emitted python");
    assert!(
        emitted.contains("requests.get(url)"),
        "emitted python should contain requests body: {emitted}"
    );
}

#[test]
fn materialize_compile_check_dispatches_to_realize_kit() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        workspace.path().join("pyproject.toml"),
        "[project]\nname = \"materialize-check-rpc\"\nversion = \"0.0.0\"\n",
    )
    .expect("write project marker");
    write_python_http_request_source(&src_dir);

    let fake_kit = workspace.path().join("fake_materialize_check_kit.py");
    write_materialize_check_rpc_kit(&fake_kit);
    install_python_script_manifest_with_metadata(
        workspace.path(),
        "python-rpc-check",
        &fake_kit,
        "rpc-check",
        None,
        &["concept:http-request"],
    );

    let out_dir = workspace.path().join("materialized");
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("python")
        .arg("--library")
        .arg("rpc-check")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize --compile-check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "materialize compile-check should succeed via kit RPC\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let marker = out_dir.join("checked-by-materialize-kit.txt");
    assert!(
        marker.exists(),
        "compile-check must be executed by the selected realize kit over RPC; marker missing\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let marker_body = fs::read_to_string(&marker).expect("read kit marker");
    assert!(
        marker_body.contains("kit-owned-classpath"),
        "compile-check RPC must receive kit-owned assemble metadata: {marker_body}"
    );
}

#[test]
fn materialize_out_dir_refuses_selected_kit_without_assemble_rpc() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        workspace.path().join("pyproject.toml"),
        "[project]\nname = \"materialize-no-assemble\"\nversion = \"0.0.0\"\n",
    )
    .expect("write project marker");
    write_python_http_request_source(&src_dir);

    let fake_kit = workspace.path().join("fake_materialize_no_assemble_kit.py");
    write_materialize_no_assemble_rpc_kit(&fake_kit);
    install_python_script_manifest_with_metadata(
        workspace.path(),
        "python-no-assemble",
        &fake_kit,
        "no-assemble",
        None,
        &["concept:http-request"],
    );

    let out_dir = workspace.path().join("materialized");
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("python")
        .arg("--library")
        .arg("no-assemble")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("spawn provekit materialize with no-assemble kit");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "materialize must fail closed when the selected kit cannot assemble target source\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assemble RPC"),
        "failure should name the missing kit assembly boundary\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !out_dir.join("client.py").exists(),
        "CLI must not write target source through legacy fallback when kit assembly is missing"
    );
}
