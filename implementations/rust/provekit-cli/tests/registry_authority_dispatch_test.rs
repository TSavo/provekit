// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use provekit_cli::kit_dispatch::{
    dispatch_bind_lift, dispatch_realize, drain_kit_dispatch_diagnostics,
    ensure_sealed_plugin_registry_for_project, federate_plugin_registries,
    reset_kit_dispatch_registry_cache_for_tests, RealizeRequest, DEFAULT_EXAM_MANIFEST_CID,
    EXAM_MANIFEST_MISMATCH_REASON,
};
use serial_test::serial;

const OTHER_EXAM_MANIFEST_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";

fn python_bin() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn write_realize_script(root: &Path, name: &str, source: &str) -> PathBuf {
    let script = root.join(format!("{name}.py"));
    fs::write(
        &script,
        format!(
            r#"import json
import sys

for line in sys.stdin:
    req = json.loads(line)
    print(json.dumps({{
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {{
            "source": {source:?},
            "is_stub": False,
            "extension": "py"
        }}
    }}), flush=True)
    break
"#
        ),
    )
    .expect("write realize script");
    script
}

fn write_lift_script(root: &Path, name: &str, fn_name: &str) -> PathBuf {
    let script = root.join(format!("{name}.py"));
    fs::write(
        &script,
        format!(
            r#"import json
import sys

for line in sys.stdin:
    req = json.loads(line)
    method = req.get("method")
    if method == "initialize":
        result = {{"name": "test-lift"}}
    elif method == "lift":
        result = {{
            "kind": "ir-document",
            "ir": [{{
                "kind": "bind-lift-entry",
                "file": "lib.rs",
                "fn_name": {fn_name:?},
                "fn_line": 1,
                "param_names": [],
                "param_types": [],
                "return_type": "i32",
                "term_shape": {{}},
                "term_shape_cid": "blake3-512:test"
            }}],
            "diagnostics": []
        }}
    else:
        result = {{}}
    print(json.dumps({{
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": result
    }}), flush=True)
    if method == "shutdown":
        break
"#
        ),
    )
    .expect("write lift script");
    script
}

fn install_manifest(
    root: &Path,
    kind: &str,
    surface: &str,
    script: &Path,
    library_tag: Option<&str>,
) -> PathBuf {
    let manifest = root
        .join(".provekit")
        .join(kind)
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest parent")).expect("create manifest dir");
    let tag = library_tag
        .map(|tag| format!("library_tag = \"{tag}\"\n"))
        .unwrap_or_default();
    let manifest_text = format!(
        "name = \"{surface}\"\n\
         {tag}\
         protocol_versions = [\"pep/1.7.0\"]\n\
         command = [\"{}\", \"{}\", \"--rpc\"]\n\
         working_dir = \".\"\n",
        python_bin().replace('\\', "\\\\").replace('"', "\\\""),
        script
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\""),
    );
    fs::write(&manifest, manifest_text).expect("write manifest");
    manifest
}

fn realize_request() -> RealizeRequest {
    RealizeRequest {
        function: "make_value".to_string(),
        params: Vec::new(),
        param_types: Vec::new(),
        return_type: "int".to_string(),
        concept_name: "concept:test".to_string(),
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
#[serial]
fn registered_realize_dispatch_uses_sealed_registry_after_manifest_removed() {
    reset_kit_dispatch_registry_cache_for_tests();
    let _ = drain_kit_dispatch_diagnostics();
    let workspace = tempfile::tempdir().expect("tempdir");
    let script = write_realize_script(
        workspace.path(),
        "realize_registered",
        "from sealed registry",
    );
    let manifest = install_manifest(
        workspace.path(),
        "realize",
        "python",
        &script,
        Some("urllib"),
    );

    let sealed =
        ensure_sealed_plugin_registry_for_project(workspace.path()).expect("seal plugin registry");
    assert!(sealed.path.exists());
    assert_eq!(sealed.memento.header.load_order.len(), 1);

    fs::remove_file(&manifest).expect("remove manifest after registry seal");
    let realized = dispatch_realize(
        workspace.path(),
        "python",
        Some("urllib"),
        &realize_request(),
    )
    .expect("dispatch through sealed registry");

    assert_eq!(realized.source, "from sealed registry");
    assert!(
        drain_kit_dispatch_diagnostics().is_empty(),
        "registry hit should not emit fallback diagnostics"
    );
}

#[test]
#[serial]
fn unregistered_realize_dispatch_falls_back_and_logs_deprecation() {
    reset_kit_dispatch_registry_cache_for_tests();
    let _ = drain_kit_dispatch_diagnostics();
    let workspace = tempfile::tempdir().expect("tempdir");

    ensure_sealed_plugin_registry_for_project(workspace.path()).expect("seal empty registry");
    let script = write_realize_script(workspace.path(), "realize_legacy", "from legacy fallback");
    install_manifest(
        workspace.path(),
        "realize",
        "python",
        &script,
        Some("urllib"),
    );

    let realized = dispatch_realize(
        workspace.path(),
        "python",
        Some("urllib"),
        &realize_request(),
    )
    .expect("dispatch through legacy fallback");
    let diagnostics = drain_kit_dispatch_diagnostics();

    assert_eq!(realized.source, "from legacy fallback");
    assert!(
        diagnostics.iter().any(|line| {
            line.contains("deprecated kit_dispatch filesystem fallback")
                && line.contains("kind=realize")
                && line.contains("surface=python")
        }),
        "missing fallback diagnostic: {diagnostics:?}"
    );
}

#[test]
#[serial]
fn registered_lift_dispatch_uses_sealed_registry_after_manifest_removed() {
    reset_kit_dispatch_registry_cache_for_tests();
    let _ = drain_kit_dispatch_diagnostics();
    let workspace = tempfile::tempdir().expect("tempdir");
    let script = write_lift_script(workspace.path(), "lift_registered", "lifted_by_registry");
    let manifest = install_manifest(workspace.path(), "lift", "rust-bind", &script, None);

    ensure_sealed_plugin_registry_for_project(workspace.path()).expect("seal lift registry");
    fs::remove_file(&manifest).expect("remove manifest after registry seal");
    let lifted =
        dispatch_bind_lift(workspace.path(), "rust").expect("dispatch lift through registry");

    assert_eq!(lifted.entries.len(), 1);
    assert_eq!(lifted.entries[0].fn_name, "lifted_by_registry");
    assert!(drain_kit_dispatch_diagnostics().is_empty());
}

#[test]
#[serial]
fn unregistered_lift_dispatch_falls_back_and_logs_deprecation() {
    reset_kit_dispatch_registry_cache_for_tests();
    let _ = drain_kit_dispatch_diagnostics();
    let workspace = tempfile::tempdir().expect("tempdir");

    ensure_sealed_plugin_registry_for_project(workspace.path()).expect("seal empty registry");
    let script = write_lift_script(workspace.path(), "lift_legacy", "lifted_by_fallback");
    install_manifest(workspace.path(), "lift", "rust-bind", &script, None);

    let lifted = dispatch_bind_lift(workspace.path(), "rust").expect("dispatch lift fallback");
    let diagnostics = drain_kit_dispatch_diagnostics();

    assert_eq!(lifted.entries.len(), 1);
    assert_eq!(lifted.entries[0].fn_name, "lifted_by_fallback");
    assert!(
        diagnostics.iter().any(|line| {
            line.contains("deprecated kit_dispatch filesystem fallback")
                && line.contains("kind=lift")
                && line.contains("surface=rust-bind")
        }),
        "missing fallback diagnostic: {diagnostics:?}"
    );
}

#[test]
#[serial]
fn sealed_registry_cid_is_deterministic_for_same_plugin_set() {
    reset_kit_dispatch_registry_cache_for_tests();
    let workspace = tempfile::tempdir().expect("tempdir");
    let script = write_realize_script(workspace.path(), "realize_stable", "stable");
    install_manifest(
        workspace.path(),
        "realize",
        "python",
        &script,
        Some("urllib"),
    );

    let first =
        ensure_sealed_plugin_registry_for_project(workspace.path()).expect("first registry seal");
    let first_bytes = fs::read(&first.path).expect("read first memento");

    reset_kit_dispatch_registry_cache_for_tests();
    let second =
        ensure_sealed_plugin_registry_for_project(workspace.path()).expect("second registry seal");
    let second_bytes = fs::read(&second.path).expect("read second memento");

    assert_eq!(first.memento.header.cid, second.memento.header.cid);
    assert_eq!(first.path, second.path);
    assert_eq!(first_bytes, second_bytes);
}

#[test]
#[serial]
fn sealed_registry_federation_refuses_exam_manifest_mismatch() {
    reset_kit_dispatch_registry_cache_for_tests();
    let local = tempfile::tempdir().expect("local tempdir");
    let remote = tempfile::tempdir().expect("remote tempdir");
    let local_script = write_realize_script(local.path(), "realize_same", "same");
    let remote_script = write_realize_script(remote.path(), "realize_same", "same");
    install_manifest(
        local.path(),
        "realize",
        "python",
        &local_script,
        Some("urllib"),
    );
    install_manifest(
        remote.path(),
        "realize",
        "python",
        &remote_script,
        Some("urllib"),
    );
    let remote_config = remote.path().join(".provekit").join("config.toml");
    fs::write(
        &remote_config,
        format!("exam_manifest_cid = \"{OTHER_EXAM_MANIFEST_CID}\"\n"),
    )
    .expect("write remote config");

    let local_registry =
        ensure_sealed_plugin_registry_for_project(local.path()).expect("local registry seal");
    let remote_registry =
        ensure_sealed_plugin_registry_for_project(remote.path()).expect("remote registry seal");
    let error = federate_plugin_registries(&local_registry.memento, &remote_registry.memento)
        .expect_err("different exam manifest CIDs must refuse");

    assert_ne!(
        local_registry.memento.header.cid,
        remote_registry.memento.header.cid
    );
    assert_eq!(error.refused_reason(), EXAM_MANIFEST_MISMATCH_REASON);
    assert_eq!(
        error.refusal_payload()["local_manifest_cid"].as_str(),
        Some(DEFAULT_EXAM_MANIFEST_CID)
    );
    assert_eq!(
        error.refusal_payload()["remote_manifest_cid"].as_str(),
        Some(OTHER_EXAM_MANIFEST_CID)
    );
}

#[test]
#[serial]
fn sealed_registry_same_cid_federates_without_exam_check() {
    reset_kit_dispatch_registry_cache_for_tests();
    let workspace = tempfile::tempdir().expect("tempdir");
    let script = write_realize_script(workspace.path(), "realize_federated", "same");
    install_manifest(
        workspace.path(),
        "realize",
        "python",
        &script,
        Some("urllib"),
    );

    let sealed =
        ensure_sealed_plugin_registry_for_project(workspace.path()).expect("registry seal");

    federate_plugin_registries(&sealed.memento, &sealed.memento)
        .expect("byte-equal registry CIDs federate");
}
