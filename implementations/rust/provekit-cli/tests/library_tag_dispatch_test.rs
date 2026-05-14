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
        mode: None,
        contract: None,
        sugar_cids: Vec::new(),
        sugar_plugins: Vec::new(),
    }
}

#[test]
fn dispatch_realize_routes_python_library_tags_to_distinct_kits() {
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
