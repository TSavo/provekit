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

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_cli::kit_dispatch::{dispatch_realize, RealizeRequest};
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

fn identity_payload_json(function: &str) -> String {
    format!(
        "{{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"identity\",\"function\":\"{function}\",\"params\":[\"x\"],\"param_types\":[\"int\"],\"return_type\":\"int\"}}"
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
    install_go_realize_manifest(workspace.path(), &bin);
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
    install_go_realize_manifest(&workspace, &bin);

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

    let mut req = identity_request();
    req.concept_name = "concept:unsupported-go-thing".to_string();

    let result = dispatch_realize(&workspace, "go", None, &req);
    assert!(
        result.is_err(),
        "unsupported concept must be refused, not silently stubbed; got {result:?}"
    );

    let _ = fs::remove_dir_all(&workspace);
}
