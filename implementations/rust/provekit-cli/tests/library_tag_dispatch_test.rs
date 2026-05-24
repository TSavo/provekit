// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use provekit_cli::kit_dispatch::{dispatch_realize, RealizeRequest};

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

fn python_bin() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn python_blake3_available() -> bool {
    std::process::Command::new(python_bin())
        .arg("-c")
        .arg("import blake3")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn pythonpath_with_realize_srcs() -> std::ffi::OsString {
    let repo = repo_root();
    let paths = [
        repo.join("implementations")
            .join("python")
            .join("provekit-realize-python-core")
            .join("src"),
        repo.join("implementations")
            .join("python")
            .join("provekit-realize-python-requests")
            .join("src"),
    ];
    let mut all = paths.to_vec();
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        all.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(all).expect("join PYTHONPATH")
}

fn install_manifest(root: &Path, surface: &str, module: &str, library_tag: &str) {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let manifest_text = format!(
        "name = \"python-realize-{library_tag}\"\n\
         library_tag = \"{library_tag}\"\n\
         command = [\"{}\", \"-m\", \"{module}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
        python_bin().replace('\\', "\\\\").replace('"', "\\\""),
    );
    fs::write(manifest, manifest_text).expect("write manifest");
}

fn http_request_realize_request() -> RealizeRequest {
    RealizeRequest {
        function: "fetch_status".to_string(),
        params: vec!["url".to_string()],
        param_types: vec!["str".to_string()],
        return_type: "int".to_string(),
        concept_name: "concept:http-request".to_string(),
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
        body_templates: Vec::new(),
    }
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

fn sql_query_realize_request() -> RealizeRequest {
    RealizeRequest {
        function: "selectRows".to_string(),
        params: vec!["sql".to_string(), "args".to_string()],
        param_types: vec!["string".to_string(), "unknown[]".to_string()],
        return_type: "unknown[]".to_string(),
        concept_name: "concept:sql-query".to_string(),
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
        body_templates: Vec::new(),
    }
}

#[test]
fn dispatch_realize_routes_python_library_tags_to_distinct_kits() {
    if !python_blake3_available() {
        eprintln!("skipping python realize dispatch: python module `blake3` is unavailable");
        return;
    }
    let workspace = tempfile::tempdir().expect("tempdir");
    install_manifest(
        workspace.path(),
        "python",
        "provekit_realize_python_core",
        "urllib",
    );
    install_manifest(
        workspace.path(),
        "python-requests",
        "provekit_realize_python_requests",
        "requests",
    );

    std::env::set_var("PROVEKIT_REPO_ROOT", repo_root());
    std::env::set_var("PYTHONPATH", pythonpath_with_realize_srcs());

    let request = http_request_realize_request();
    let urllib = dispatch_realize(workspace.path(), "python", Some("urllib"), &request)
        .expect("dispatch urllib realize kit");
    let requests = dispatch_realize(workspace.path(), "python", Some("requests"), &request)
        .expect("dispatch requests realize kit");

    assert_ne!(urllib.source, requests.source);
    assert!(
        urllib.source.contains("urllib.request.urlopen"),
        "urllib output should use urllib.request.urlopen, got:\n{}",
        urllib.source
    );
    assert!(
        requests.source.contains("requests.get"),
        "requests output should use requests.get, got:\n{}",
        requests.source
    );
}

#[test]
fn dispatch_realize_routes_typescript_sql_library_tags_to_distinct_kits() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let repo = repo_root();
    install_node_manifest(
        workspace.path(),
        "typescript-better-sqlite3",
        &repo
            .join("implementations")
            .join("typescript")
            .join("provekit-realize-typescript-better-sqlite3")
            .join("src")
            .join("main.js"),
        "better-sqlite3",
    );
    install_node_manifest(
        workspace.path(),
        "typescript-pg",
        &repo
            .join("implementations")
            .join("typescript")
            .join("provekit-realize-typescript-pg")
            .join("src")
            .join("main.js"),
        "pg",
    );

    std::env::set_var("PROVEKIT_REPO_ROOT", repo);

    let request = sql_query_realize_request();
    let sqlite = dispatch_realize(
        workspace.path(),
        "typescript",
        Some("better-sqlite3"),
        &request,
    )
    .expect("dispatch better-sqlite3 realize kit");
    let pg = dispatch_realize(workspace.path(), "typescript", Some("pg"), &request)
        .expect("dispatch pg realize kit");

    assert_ne!(sqlite.source, pg.source);
    assert!(
        sqlite.source.contains("db.prepare(sql).all(args)"),
        "better-sqlite3 output should use db.prepare().all(), got:\n{}",
        sqlite.source
    );
    assert!(
        pg.source.contains("await pool.query(sql, args)"),
        "pg output should use awaited pool.query, got:\n{}",
        pg.source
    );
}
