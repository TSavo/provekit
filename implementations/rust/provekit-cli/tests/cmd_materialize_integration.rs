// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
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
    let binary = repo
        .join("implementations")
        .join("python")
        .join("target")
        .join("release")
        .join("provekit-realize-python-requests");
    if !binary.exists() {
        return None;
    }
    install_binary_manifest(
        workspace,
        "python-requests",
        &binary,
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

fn family_sql_payload_json() -> &'static str {
    "{\"artifact_kind\":\"provekit-concept-citation-comment-sugar\",\"concept_name\":\"concept:sql-query-all\",\"family\":\"concept:family:sql\",\"function\":\"selectRows\",\"params\":[\"sql\",\"args\"],\"param_types\":[\"string\",\"unknown[]\"],\"return_type\":\"unknown[]\",\"named_term_tree\":{\"conceptName\":\"concept:sql-query-all\",\"args\":[{\"sort\":\"Sql\",\"source\":\"sql\"},{\"sort\":\"SqlArgs\",\"source\":\"args\"}]}}"
}

fn concept_payload_cid() -> String {
    payload_cid(concept_payload_json())
}

fn payload_cid(payload: &str) -> String {
    let json: Json = serde_json::from_str(payload).expect("payload json parses");
    let canonical = canonical_value_from_json(&json);
    blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes())
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

fn write_rust_family_sql_carrier_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("lib.rs");
    fs::write(
        &source_path,
        format!(
            "// rust source\n{}// end\n",
            carrier_lines("//", "", family_sql_payload_json())
        ),
    )
    .expect("write rust family SQL carrier source");
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

#[test]
fn cross_language_discovery_honors_top_level_library_constraint() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let repo = repo_root();
    let src_dir = workspace.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    write_rust_family_sql_carrier_source(&src_dir);
    fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname = \"cross-language-source\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write source project marker");

    let fake_realize = workspace.path().join("fake_realize.py");
    fs::write(
        &fake_realize,
        r#"import json, sys
for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps({
        "jsonrpc": "2.0",
        "id": request.get("id"),
        "result": {
            "source": "const rows = db.prepare(sql).all(args);",
            "is_stub": False,
            "extension": "ts"
        }
    }), flush=True)
"#,
    )
    .expect("write fake realize script");

    install_python_script_manifest_with_metadata(
        workspace.path(),
        "typescript-better-sqlite3",
        &fake_realize,
        "better-sqlite3",
        Some("concept:family:sql"),
        &["concept:sql-query-all"],
    );
    install_python_script_manifest_with_metadata(
        workspace.path(),
        "typescript-pg",
        &fake_realize,
        "pg",
        Some("concept:family:sql"),
        &["concept:sql-query-all"],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", repo)
        .arg("materialize")
        .arg("--source-lang")
        .arg("rust")
        .arg("--target")
        .arg("typescript")
        .arg("--library")
        .arg("typescript-better-sqlite3")
        .arg("--source-dir")
        .arg(&src_dir)
        .arg("--project")
        .arg(workspace.path())
        .output()
        .expect("spawn provekit materialize cross-language discovery");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "top-level --library should disambiguate cross-language discovery\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("RESOLVE") && stderr.contains("manifest `better-sqlite3`"),
        "should resolve through better-sqlite3, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("AMBIGUOUS"),
        "--library should prevent ambiguous outcome, got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("1 resolve + 0 ambiguous"),
        "summary should count one resolved site, got stderr:\n{stderr}"
    );
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
/// `python3 -m py_compile` over the emitted file must pass → exit 0.
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
        stderr.contains("compile-check: python3 -m py_compile passed"),
        "stderr should confirm py_compile passed\nstderr:\n{stderr}"
    );
    let emitted = fs::read_to_string(out_dir.join("client.py")).expect("read emitted python");
    assert!(
        emitted.contains("requests.get(url)"),
        "emitted python should contain requests body: {emitted}"
    );
}

/// Negative test: confirms python3 -m py_compile actually rejects bad python
/// (so the compile-check gate would fire if materialize emitted bad output).
#[test]
fn compile_check_python_gate_fails_on_syntax_error() {
    let bad_dir = tempfile::tempdir().expect("tempdir");
    let bad_py = bad_dir.path().join("bad.py");
    fs::write(&bad_py, "def foo(\n    x = 42\n    return x\n").expect("write bad.py");

    let check = Command::new("python3")
        .arg("-m")
        .arg("py_compile")
        .arg(&bad_py)
        .output()
        .expect("spawn python3 -m py_compile");

    assert!(
        !check.status.success(),
        "python3 -m py_compile must reject syntactically broken python (gate must be live)"
    );
}
