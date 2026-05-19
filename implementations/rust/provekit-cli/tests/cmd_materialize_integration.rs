// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let manifest_text = format!(
        "name = \"typescript-realize-{library_tag}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"{}\", \"{}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
        node_bin().replace('\\', "\\\\").replace('"', "\\\""),
        script,
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

fn write_concept_source(src_dir: &Path) -> PathBuf {
    let source_path = src_dir.join("queries.ts");
    fs::write(
        &source_path,
        r#"// header stays
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"selectRows","params":["sql","args"],"param_types":["string","unknown[]"],"return_type":"unknown[]","named_term_tree":{"conceptName":"concept:sql-query","args":[{"sort":"Sql","source":"sql"},{"sort":"SqlArgs","source":"args"}]}}
// provekit-concept-payload-cid: blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
// footer stays
"#,
    )
    .expect("write source");
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
        stdout.contains("materialized 1 concept citation(s) across 1 file(s)"),
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
        stdout.contains("materialized 1 concept citation(s) across 1 file(s)"),
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
