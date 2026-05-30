// SPDX-License-Identifier: Apache-2.0
//
// GO MATERIALIZE/REALIZE gauntlet: a contract is materialized into a Go
// surface by the Go SHIM that supplies the native Go sugar
// (provekit-realize-go-core). This is the emit direction's Go peer of
// provekit-realize-python-core, exercised through the substrate's
// language-neutral realize dispatch (kit_dispatch::dispatch_realize, PEP
// 1.7.0 `provekit.plugin.invoke`).
//
// HONEST claim (and its boundary): this drives the realize DISPATCH directly
// (the same code path `provekit materialize` uses to invoke a target kit), NOT
// the full `provekit materialize` carrier-rewrite pipeline. The latter needs a
// Go `@sugar`/`@boundary` authoring surface (carrier annotations in source),
// which is DEFERRED follow-up -- see this file's sibling report. What is proven
// here: the substrate dispatches a contract's concept to the Go shim, the shim
// supplies REAL Go sugar, and that sugar `go build`s. Supra omnia, rectum.
//
// The concept is `identity` -- a real cross-language concept (also in Python's
// canonical-bodies), realized in Go as `return x`. Requires `go` on PATH.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_cli::kit_dispatch::{dependency_proofs_via_rpc, dispatch_realize, RealizeRequest};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::Value as Json;

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
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-go-realize-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

/// Build the Go realize shim binary; panic on failure (a real regression).
fn build_go_realize() -> PathBuf {
    let module = repo_root()
        .join("implementations")
        .join("go")
        .join("provekit-realize-go-core");
    let out = std::env::temp_dir().join(format!("provekit-realize-go-{}", std::process::id()));
    let built = Command::new("go")
        .current_dir(&module)
        .args([
            "build",
            "-o",
            out.to_str().expect("utf8"),
            "./cmd/provekit-realize-go",
        ])
        .output()
        .expect("spawn go build");
    assert!(
        built.status.success(),
        "go build provekit-realize-go failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        out.exists(),
        "go build produced no binary at {}",
        out.display()
    );
    out
}

/// Register the Go realize shim as the `go` realize surface in `root`.
fn install_go_realize_manifest(root: &Path, bin: &Path) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join("go")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest dir");
    let text = format!(
        "name = \"go-realize\"\nlibrary_tag = \"go\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        bin.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    );
    fs::write(manifest, text).expect("write manifest");
}

fn install_go_realize_registration(root: &Path, bin: &Path) {
    install_go_realize_manifest(root, bin);
    fs::create_dir_all(root.join(".provekit")).expect("mkdir .provekit");
    fs::write(
        root.join(".provekit").join("config.toml"),
        r#"[[plugins]]
name = "go-realize"
kind = "realize"
surface = "go"

"#,
    )
    .expect("write realize config");
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

fn rewrite_manifest_command(manifest: &Path, command: &Path) {
    let text = fs::read_to_string(manifest)
        .unwrap_or_else(|_| panic!("read checked-in manifest {}", manifest.display()));
    let escaped = command
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let rewritten = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("command = ") {
                format!("command = [\"{escaped}\", \"--rpc\"]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(manifest, format!("{rewritten}\n"))
        .unwrap_or_else(|_| panic!("write manifest {}", manifest.display()));
}

fn write_go_dependency_with_proof(project: &Path, proof_name: &str) -> PathBuf {
    let dep = project.join("dep");
    let proof = dep.join("META-INF").join("provekit").join(proof_name);
    fs::create_dir_all(proof.parent().unwrap()).expect("mkdir dependency proof dir");
    fs::write(
        dep.join("go.mod"),
        "module example.com/proofdep\n\ngo 1.22\n",
    )
    .expect("write dep go.mod");
    fs::write(&proof, "proof bytes").expect("write dependency proof");
    fs::write(
        project.join("go.mod"),
        "module example.com/app\n\ngo 1.22\n\nrequire example.com/proofdep v0.0.0\nreplace example.com/proofdep => ./dep\n",
    )
    .expect("write project go.mod");
    proof
}

fn identity_payload_json(function: &str) -> String {
    format!(
        "{{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"identity\",\"function\":\"{function}\",\"params\":[\"x\"],\"param_types\":[\"int\"],\"return_type\":\"int\"}}"
    )
}

fn proof_backed_payload_json(function: &str) -> String {
    format!(
        "{{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"concept:go-proof-backed\",\"function\":\"{function}\",\"params\":[\"x\"],\"param_types\":[\"int\"],\"return_type\":\"int\"}}"
    )
}

fn payload_cid(payload: &str) -> String {
    let json: Json = serde_json::from_str(payload).expect("payload json parses");
    let canonical = canonical_value_from_json(&json);
    blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes())
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

fn write_go_identity_carrier(src_dir: &Path) -> PathBuf {
    let payload = identity_payload_json("Id");
    let source = src_dir.join("id.go");
    fs::write(
        &source,
        format!(
            "package sample\n\n// provekit-concept: {payload}\n// provekit-concept-payload-cid: {}\n",
            payload_cid(&payload)
        ),
    )
    .expect("write go carrier source");
    source
}

fn write_go_proof_backed_carrier(src_dir: &Path) -> PathBuf {
    let payload = proof_backed_payload_json("AddFortyOne");
    let source = src_dir.join("add_forty_one.go");
    fs::write(
        &source,
        format!(
            "package sample\n\n// provekit-concept: {payload}\n// provekit-concept-payload-cid: {}\n",
            payload_cid(&payload)
        ),
    )
    .expect("write go proof-backed carrier source");
    source
}

fn write_external_go_dependency_with_proof_backed_body(project: &Path) -> PathBuf {
    write_external_go_dependency_with_body(
        project,
        "concept:go-proof-backed",
        "value",
        "return value + 41",
    )
}

fn write_external_go_dependency_with_identity_body(project: &Path) -> PathBuf {
    write_external_go_dependency_with_body(project, "identity", "x", "return x")
}

fn write_external_go_dependency_with_body(
    project: &Path,
    concept_name: &str,
    param_name: &str,
    body_text: &str,
) -> PathBuf {
    let dep = unique_dir("go-proof-backed-dep");
    let proof_dir = dep.join("META-INF").join("provekit");
    fs::create_dir_all(&proof_dir).expect("mkdir dependency proof dir");
    fs::write(
        dep.join("go.mod"),
        "module example.com/proofdep\n\ngo 1.22\n",
    )
    .expect("write dep go.mod");

    let member = serde_json::json!({
        "body": {
            "kind": "library-sugar-binding-entry",
            "concept_name": concept_name,
            "source_function_name": "ProofBacked",
            "target_language": "go",
            "target_library_tag": "go",
            "param_names": [param_name],
            "param_types": ["int"],
            "return_type": "int",
            "body_source": {
                "body_text": body_text
            },
            "loss_record_contribution": {
                "form": "literal",
                "value": {"entries": []}
            }
        }
    });
    let member_bytes = serde_json::to_vec(&member).expect("member json");
    let mut members = BTreeMap::new();
    members.insert(format!("blake3-512:{}", "b".repeat(128)), member_bytes);
    let signer_seed: Ed25519Seed = [0x42; 32];
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@test/go-proof-backed".to_string(),
        version: "0.0.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: "2026-05-29T00:00:00.000Z".to_string(),
    });
    let proof_path = proof_dir.join(format!("{}.proof", proof.cid));
    fs::write(&proof_path, proof.bytes).expect("write proof-backed Go proof");

    fs::write(
        project.join("go.mod"),
        format!(
            "module example.com/app\n\ngo 1.22\n\nrequire example.com/proofdep v0.0.0\nreplace example.com/proofdep => {}\n",
            dep.display()
        ),
    )
    .expect("write project go.mod");
    proof_path
}

fn identity_request() -> RealizeRequest {
    RealizeRequest {
        function: "Id".to_string(),
        params: vec!["x".to_string()],
        param_types: vec!["int".to_string()],
        return_type: "int".to_string(),
        concept_name: "identity".to_string(),
        named_term_tree: None,
        term_shape: None,
        operand_bindings: Vec::new(),
        source_function_name: None,
        mode: None,
        modes: Vec::new(),
        contract: None,
        sugar_cids: Vec::new(),
        sugar_plugins: Vec::new(),
        proc_macro_invocations: Vec::new(),
        family: None,
        library_version: None,
        param_sort_cids: Vec::new(),
        return_sort_cid: String::new(),
        target_library_tag: String::new(),
        visibility: String::new(),
        generic_params: String::new(),
        original_param_types: Vec::new(),
        parametric_sort_expansions: Vec::new(),
        function_return_types: std::collections::BTreeMap::new(),
        doc_lines: Vec::new(),
    }
}

#[test]
fn go_materialize_refuses_unconfigured_realize_manifest() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go manifest registration test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    install_go_realize_manifest(workspace.path(), &bin);
    write_external_go_dependency_with_identity_body(workspace.path());
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_go_identity_carrier(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_go_materialized\n\ngo 1.22\n",
    )
    .expect("write output go.mod");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("go")
        .arg("--library")
        .arg("go")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for unconfigured Go");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "Go materialize must require a project [[plugins]] realize registration, not just a loose manifest\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no realize plugin registration")
            || stdout.contains("no realize plugin registration"),
        "failure should explain the missing config registration\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn go_materialize_uses_checked_in_go_double_realize_registration() {
    if !go_available() {
        eprintln!("go not on PATH: skipping checked-in Go materialize registration test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(
        &repo_root()
            .join("examples")
            .join("go-double")
            .join(".provekit"),
        &workspace.path().join(".provekit"),
    );
    rewrite_manifest_command(
        &workspace
            .path()
            .join(".provekit")
            .join("realize")
            .join("go")
            .join("manifest.toml"),
        &bin,
    );
    write_external_go_dependency_with_identity_body(workspace.path());
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_go_identity_carrier(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_go_materialized\n\ngo 1.22\n",
    )
    .expect("write output go.mod");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("go")
        .arg("--library")
        .arg("go")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for checked-in Go registration");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "checked-in go-double realize registration must drive materialize\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assembled by go kit via RPC"),
        "checked-in Go materialize route must assemble through the Go kit\nstderr:\n{stderr}"
    );
    let emitted = fs::read_to_string(out_dir.join("id.go")).expect("read materialized Go");
    assert!(
        emitted.contains("func Id(x int) int") && emitted.contains("return x"),
        "materialized Go should contain identity body from checked-in registration:\n{emitted}"
    );
}

#[test]
fn go_materialize_uses_body_template_from_go_module_proof() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go proof-backed materialize CLI test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    install_go_realize_registration(workspace.path(), &bin);
    let proof_path = write_external_go_dependency_with_proof_backed_body(workspace.path());
    assert!(
        !proof_path.starts_with(workspace.path()),
        "fixture must keep the shim .proof outside the CLI project root"
    );

    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_go_proof_backed_carrier(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_go_materialized\n\ngo 1.22\n",
    )
    .expect("write output go.mod");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("go")
        .arg("--library")
        .arg("go")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for proof-backed Go");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Go materialize must load body templates through the Go kit's module proof resolver\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let emitted = fs::read_to_string(out_dir.join("add_forty_one.go"))
        .expect("read materialized proof-backed Go");
    assert!(
        emitted.contains("func AddFortyOne(x int) int"),
        "materialized Go should contain realized function signature:\n{emitted}"
    );
    assert!(
        emitted.contains("return x + 41"),
        "materialized Go body must come from the dependency .proof, not the inline identity table:\n{emitted}"
    );
}

#[test]
fn go_dependency_proofs_are_resolved_by_configured_go_kit() {
    if !go_available() {
        eprintln!("go not on PATH: skipping Go dependency proof resolver test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    install_go_realize_registration(workspace.path(), &bin);
    let proof_name = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.proof";
    let expected = write_go_dependency_with_proof(workspace.path(), proof_name);

    let proofs = dependency_proofs_via_rpc(workspace.path())
        .expect("CLI must ask the configured Go kit over RPC for dependency proofs");

    assert_eq!(
        proofs.len(),
        1,
        "dependency proof resolution must be kit-owned and config/manifest-driven"
    );
    let expected_bytes = fs::read(&expected).expect("read expected dependency proof");
    assert_eq!(
        blake3_512_of(&proofs[0].bytes),
        blake3_512_of(&expected_bytes),
        "resolved proof must preserve content-addressed proof bytes"
    );
    assert!(
        proofs[0].label != expected.display().to_string(),
        "the Go kit must not hand the CLI a Go module-internal proof path"
    );
}

/// Full CLI path: `provekit materialize` reads Go carrier comments, dispatches
/// the concept to the Go realize kit over RPC, writes materialized Go under
/// --out-dir, and asks the same Go kit to run its native check.
#[test]
fn go_materialize_cli_rewrites_carrier_and_asks_go_kit_to_compile_check() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go materialize CLI test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    install_go_realize_registration(workspace.path(), &bin);
    write_external_go_dependency_with_identity_body(workspace.path());
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_go_identity_carrier(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_go_materialized\n\ngo 1.22\n",
    )
    .expect("write output go.mod");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--target")
        .arg("go")
        .arg("--library")
        .arg("go")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for Go");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Go materialize CLI path should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("compile-check: go test ./... passed"),
        "Go materialize must ask the Go kit to run its native check\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assembled by go kit via RPC"),
        "Go materialize must use the configured Go kit for assembly, not legacy concat fallback\nstderr:\n{stderr}"
    );

    let emitted = fs::read_to_string(out_dir.join("id.go")).expect("read materialized Go");
    assert!(
        emitted.contains("func Id(x int) int"),
        "materialized Go should contain realized function signature:\n{emitted}"
    );
    assert!(
        emitted.contains("return x"),
        "materialized Go should contain identity body:\n{emitted}"
    );
    assert!(
        !emitted.contains("provekit-concept:"),
        "materialized Go must remove the carrier comments:\n{emitted}"
    );
}

#[test]
fn go_materialize_infers_target_from_registered_go_manifest() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go manifest-inference materialize test");
        return;
    }
    let bin = build_go_realize();
    let workspace = tempfile::tempdir().expect("tempdir");
    install_go_realize_registration(workspace.path(), &bin);
    write_external_go_dependency_with_identity_body(workspace.path());
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("mkdir src");
    write_go_identity_carrier(&src_dir);

    let out_dir = workspace.path().join("materialized");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_go_materialized\n\ngo 1.22\n",
    )
    .expect("write output go.mod");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("materialize")
        .arg("--library")
        .arg("go")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .output()
        .expect("spawn provekit materialize for inferred Go target");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Go materialize should infer target from the registered Go manifest, without --target\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("assembled by go kit via RPC"),
        "inferred Go materialize must still assemble through the Go kit\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("compile-check: go test ./... passed"),
        "inferred Go materialize must still ask the Go kit to run its native check\nstderr:\n{stderr}"
    );

    let emitted = fs::read_to_string(out_dir.join("id.go")).expect("read materialized Go");
    assert!(
        emitted.contains("func Id(x int) int") && emitted.contains("return x"),
        "materialized Go should contain the Go kit's realized identity body:\n{emitted}"
    );
}

/// The shim supplies REAL Go sugar (`func Id(x int) int { return x }`),
/// dispatched through the substrate realize protocol, and that Go compiles.
#[test]
fn go_realize_shim_materializes_compilable_go_for_identity() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go realize materialize test");
        return;
    }
    let bin = build_go_realize();
    let workspace = unique_dir("ws");
    install_go_realize_registration(&workspace, &bin);
    write_external_go_dependency_with_identity_body(&workspace);

    let realized = dispatch_realize(&workspace, "go", None, &identity_request())
        .expect("go realize shim must materialize the identity concept");

    assert!(
        !realized.is_stub,
        "realize must supply real sugar for a supported concept, not a stub"
    );
    assert_eq!(realized.extension, "go", "realized extension must be go");
    eprintln!("GO_REALIZE_SOURCE=\n{}", realized.source);

    // The supplied sugar must be the correct Go realization of `identity`.
    assert!(
        realized.source.contains("func Id(x int) int"),
        "sugar must declare the requested signature; got:\n{}",
        realized.source
    );
    assert!(
        realized.source.contains("return x"),
        "identity sugar body must be `return x`; got:\n{}",
        realized.source
    );

    // The decisive bar: the supplied Go sugar COMPILES.
    let proj = unique_dir("compile");
    fs::write(proj.join("go.mod"), "module realized.test\n\ngo 1.22\n").expect("write go.mod");
    fs::write(
        proj.join("id.go"),
        format!("package sample\n\n{}\n", realized.source),
    )
    .expect("write id.go");
    let build = Command::new("go")
        .current_dir(&proj)
        .args(["build", "./..."])
        .output()
        .expect("spawn go build of realized sugar");
    assert!(
        build.status.success(),
        "materialized Go sugar must compile\n  stdout: {}\n  stderr: {}\n  source:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr),
        realized.source
    );
    eprintln!("GO_REALIZE_COMPILE_EXIT=0");

    let _ = fs::remove_dir_all(&workspace);
    let _ = fs::remove_dir_all(&proj);
}

/// Discrimination: an UNSUPPORTED concept is refused loudly (the shim does not
/// silently stub). Proves the realize coverage is honest, not vacuous.
#[test]
fn go_realize_shim_refuses_unsupported_concept() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go realize refusal test");
        return;
    }
    let bin = build_go_realize();
    let workspace = unique_dir("refuse");
    install_go_realize_manifest(&workspace, &bin);
    write_external_go_dependency_with_identity_body(&workspace);

    let mut req = identity_request();
    req.concept_name = "concept:unsupported-go-thing".to_string();

    let result = dispatch_realize(&workspace, "go", None, &req);
    assert!(
        result.is_err(),
        "unsupported concept must be refused, not silently stubbed; got {result:?}"
    );

    let _ = fs::remove_dir_all(&workspace);
}
