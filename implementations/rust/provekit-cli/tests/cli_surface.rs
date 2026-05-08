// SPDX-License-Identifier: Apache-2.0

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

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

fn write_executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(path)
            .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
    }
}

#[test]
fn bug_zoo_machinery_is_self_contained() {
    let root = repo_root();
    assert!(
        root.join("menagerie/bug-zoo/Cargo.toml").exists(),
        "Bug Zoo should live as a Menagerie destination under menagerie/bug-zoo/"
    );
    assert!(
        !root.join("bug-zoo/Cargo.toml").exists(),
        "Bug Zoo should no longer live at the repository root"
    );
    assert!(
        !root
            .join("implementations/rust/provekit-cli/src/cmd_zoo.rs")
            .exists(),
        "Bug Zoo should not be embedded as a provekit CLI command"
    );
    assert!(
        !root
            .join("implementations/rust/provekit-cli/tests/support/bug_zoo.rs")
            .exists(),
        "Bug Zoo harness code should live under menagerie/bug-zoo/, not provekit-cli tests"
    );
}

#[test]
fn provekit_cli_does_not_expose_zoo_subcommand() {
    let output = Command::new(provekit_bin())
        .arg("--help")
        .output()
        .expect("spawn provekit --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("zoo"),
        "`provekit zoo` must remain a repo harness, not a public CLI subcommand\nstdout:\n{stdout}"
    );
}

#[test]
fn prove_formula_catches_value_scope_escape() {
    if Command::new("z3").arg("--version").output().is_err() {
        eprintln!("skipping: z3 is not available on PATH");
        return;
    }

    let dir = tempfile::tempdir().expect("create tempdir");
    let exhibit_formula = dir.path().join("exhibit-value-scope.json");
    let fixed_formula = dir.path().join("fixed-value-scope.json");

    std::fs::write(
        &exhibit_formula,
        serde_json::to_vec_pretty(&json!({
            "kind": "implies",
            "operands": [
                {"kind": "atomic", "name": "eq", "args": [
                    {"kind": "var", "name": "value"},
                    {"kind": "const", "value": 42, "sort": {"kind": "primitive", "name": "Int"}}
                ]},
                {"kind": "atomic", "name": "gte", "args": [
                    {"kind": "var", "name": "value"},
                    {"kind": "const", "value": 43, "sort": {"kind": "primitive", "name": "Int"}}
                ]}
            ]
        }))
        .expect("serialize exhibit formula"),
    )
    .expect("write exhibit formula");
    std::fs::write(
        &fixed_formula,
        serde_json::to_vec_pretty(&json!({
            "kind": "implies",
            "operands": [
                {"kind": "atomic", "name": "eq", "args": [
                    {"kind": "var", "name": "value"},
                    {"kind": "const", "value": 43, "sort": {"kind": "primitive", "name": "Int"}}
                ]},
                {"kind": "atomic", "name": "gte", "args": [
                    {"kind": "var", "name": "value"},
                    {"kind": "const", "value": 43, "sort": {"kind": "primitive", "name": "Int"}}
                ]}
            ]
        }))
        .expect("serialize fixed formula"),
    )
    .expect("write fixed formula");

    let exhibit = Command::new(provekit_bin())
        .arg("prove")
        .arg("--formula")
        .arg(&exhibit_formula)
        .arg("--json")
        .arg("--quiet")
        .output()
        .expect("spawn provekit prove --formula exhibit");
    let exhibit_stdout = String::from_utf8_lossy(&exhibit.stdout);
    let exhibit_stderr = String::from_utf8_lossy(&exhibit.stderr);
    assert_eq!(
        exhibit.status.code(),
        Some(1),
        "42 should not discharge >= 43\nstdout:\n{exhibit_stdout}\nstderr:\n{exhibit_stderr}"
    );
    let exhibit_report: serde_json::Value =
        serde_json::from_str(&exhibit_stdout).expect("exhibit JSON parses");
    assert_eq!(exhibit_report["ok"], false);
    assert_eq!(exhibit_report["status"], "unsatisfied");

    let fixed = Command::new(provekit_bin())
        .arg("prove")
        .arg("--formula")
        .arg(&fixed_formula)
        .arg("--json")
        .arg("--quiet")
        .output()
        .expect("spawn provekit prove --formula fixed");
    let fixed_stdout = String::from_utf8_lossy(&fixed.stdout);
    let fixed_stderr = String::from_utf8_lossy(&fixed.stderr);
    assert!(
        fixed.status.success(),
        "43 should discharge >= 43\nstdout:\n{fixed_stdout}\nstderr:\n{fixed_stderr}"
    );
    let fixed_report: serde_json::Value =
        serde_json::from_str(&fixed_stdout).expect("fixed JSON parses");
    assert_eq!(fixed_report["ok"], true);
    assert_eq!(fixed_report["status"], "discharged");
}

#[test]
fn lift_identify_only_delegates_from_project_config() {
    let root = repo_root();
    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(root.join("menagerie/bridgeworks/checked-add-u8"))
        .arg("--identify-only")
        .arg("--json")
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("spawn provekit lift --identify-only");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift --identify-only failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("identify-only lift JSON parses");
    assert_eq!(report["kind"], "identity-document");
    let identities = report["identities"].as_array().expect("identities array");
    assert_eq!(identities.len(), 8);
    assert!(identities.iter().any(|identity| {
        identity["domain"] == "software" && identity["claim"] == "checked_add_u8.postcondition"
    }));
}

#[test]
fn lift_identify_only_rejects_non_identity_response() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let project = dir.path().join("project");
    let manifest_dir = project.join(".provekit/lift/bad-identify");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(
        project.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"bad-identify\"\n",
    )
    .expect("write config");
    let plugin = dir.path().join("bad-identify-plugin.sh");
    write_executable(
        &plugin,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"bad-identify","protocol_version":"provekit-lift/1","capabilities":{}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","ir":[],"diagnostics":[]}}'
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
    exit 0
  fi
done
"#,
    );
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"bad-identify\"\ncommand = [\"{}\"]\n",
            plugin.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .arg("--identify-only")
        .arg("--json")
        .arg("--quiet")
        .output()
        .expect("spawn provekit lift --identify-only");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "identify-only must reject a full ir-document response\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("identify-only") && stderr.contains("identity-document"),
        "stderr should explain the response-shape violation\nstderr:\n{stderr}"
    );
}

#[test]
fn mint_uses_lift_surface_from_project_config() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let project = dir.path().join("project");
    let manifest_dir = project.join(".provekit/lift/mint-lift");
    let out_dir = dir.path().join("out");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(
        project.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"mint-lift\"\n",
    )
    .expect("write config");
    let plugin = dir.path().join("mint-lift-plugin.sh");
    write_executable(
        &plugin,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"mint-lift","protocol_version":"provekit-lift/1","capabilities":{}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","ir":[{"kind":"contract","name":"demo.contract","outBinding":"out","post":{"kind":"atomic","name":"demo_true","args":[]}}],"diagnostics":[]}}'
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
    exit 0
  fi
done
"#,
    );
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"mint-lift\"\ncommand = [\"{}\"]\n",
            plugin.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(&project)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-attest")
        .arg("--json")
        .arg("--quiet")
        .output()
        .expect("spawn provekit mint");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "mint should compose through [authoring.lift], not require [authoring.must]\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("mint JSON parses");
    assert_eq!(report["surface"], "mint-lift");
    assert_eq!(report["lift"]["kind"], "ir-document");
    assert!(report["filenameCid"]
        .as_str()
        .unwrap_or_default()
        .starts_with("blake3-512:"));
}
