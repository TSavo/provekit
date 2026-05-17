// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::{fs, path::Path};

use provekit_cli::kit_dispatch::dispatch_exam_manifest;

const EXPECTED_EXAM_MANIFEST_CID: &str = "blake3-512:0e0dc132f3e8bf58da065d7fc237e85c225c5c87fbc690a19a42d594e9b1e46ed78e8f0f5a855fa1b75581745f588a4737adb17bc59e9a72b3bb9f6bcb665dd0";

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

#[test]
fn dispatch_exam_manifest_builtin_loads_fixture_path() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let fixture = repo_root()
        .join("implementations")
        .join("rust")
        .join("libprovekit")
        .join("tests")
        .join("fixtures")
        .join("exam_manifest")
        .join("v1.example.json");

    let manifest = dispatch_exam_manifest(
        workspace.path(),
        "default",
        fixture.to_str().expect("fixture path is utf-8"),
    )
    .expect("dispatch built-in exam manifest loader");

    manifest.validate().expect("manifest validates");
    assert_eq!(manifest.header.cid, EXPECTED_EXAM_MANIFEST_CID);
}

#[test]
fn dispatch_exam_manifest_invokes_project_plugin_manifest() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let fixture = exam_manifest_fixture();
    let script = workspace.path().join("exam_plugin.sh");
    fs::write(
        &script,
        "#!/bin/sh\n\
         while IFS= read -r line; do\n\
         case \"$line\" in\n\
         *provekit.plugin.invoke*)\n\
         printf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":'\n\
         tr -d '\\n' < \"$1\"\n\
         printf '}\\n'\n\
         exit 0\n\
         ;;\n\
         esac\n\
         done\n",
    )
    .expect("write plugin script");
    install_exam_manifest_plugin(workspace.path(), "test-plugin", &script, &fixture);

    let manifest = dispatch_exam_manifest(workspace.path(), "test-plugin", "ignored.json")
        .expect("dispatch project exam manifest plugin");

    manifest.validate().expect("manifest validates");
    assert_eq!(manifest.header.cid, EXPECTED_EXAM_MANIFEST_CID);
}

fn exam_manifest_fixture() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("rust")
        .join("libprovekit")
        .join("tests")
        .join("fixtures")
        .join("exam_manifest")
        .join("v1.example.json")
}

fn install_exam_manifest_plugin(root: &Path, name: &str, script: &Path, fixture: &Path) {
    let manifest = root
        .join(".provekit")
        .join("exam-manifest")
        .join(name)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create manifest dir");
    let script = shell_string(script);
    let fixture = shell_string(fixture);
    let manifest_text = format!(
        "name = \"{name}\"\n\
         version = \"1.0.0\"\n\
         protocol_versions = [\"pep/1.7.0\"]\n\
         command = [\"sh\", \"{script}\", \"{fixture}\"]\n\
         working_dir = \".\"\n\
         \n\
         [capabilities]\n\
         kind = \"exam-manifest\"\n\
         exam_manifest_schema_version = \"provekit-exam-manifest/v1\"\n",
    );
    fs::write(manifest, manifest_text).expect("write manifest");
}

fn shell_string(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
