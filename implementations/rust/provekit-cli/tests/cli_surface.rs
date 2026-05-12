// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use libprovekit::core::{address, Input, Path as CorePath, PathAlgebra, PathDocument};
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

/// Write `text` to `path` and mark it executable.
///
/// Uses explicit `sync_all` + drop before `set_permissions` to ensure the
/// kernel writer-fd is fully closed before the caller spawns the script.
/// This prevents ETXTBSY (os error 26) races on Linux where `exec` refuses
/// a file that still has an open writer fd.
fn write_executable(path: &Path, text: &str) {
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        f.write_all(text.as_bytes())
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        f.sync_all()
            .unwrap_or_else(|e| panic!("sync {}: {e}", path.display()));
        // f is dropped here — fd closed before chmod
    }
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

/// Spawn `cmd` and retry up to 5 times if the CLI subprocess reports
/// ETXTBSY ("Text file busy", os error 26) in stderr.
///
/// The root cause is a Linux kernel race: `exec` refuses a file that still
/// has an open writer fd anywhere on the system (e.g. the parallel test
/// runner's cargo worker just finished writing the plugin script).
/// `write_executable` closes + syncs before returning, but a belt-and-braces
/// retry catches any residual races.
fn output_retrying_etxtbsy(cmd: &mut Command) -> std::process::Output {
    const MAX_ATTEMPTS: u32 = 5;
    for attempt in 0..MAX_ATTEMPTS {
        let out = cmd.output().expect("spawn provekit");
        let stderr = String::from_utf8_lossy(&out.stderr);
        let is_etxtbsy = !out.status.success()
            && (stderr.contains("Text file busy") || stderr.contains("os error 26"));
        if !is_etxtbsy {
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * u64::from(attempt + 1)));
    }
    cmd.output().expect("spawn provekit (final attempt)")
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
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"bad-identify","protocol_version":"pep/1.7.0","capabilities":{}}}'
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

    let output = output_retrying_etxtbsy(
        Command::new(provekit_bin())
            .arg("lift")
            .arg(&project)
            .arg("--identify-only")
            .arg("--json")
            .arg("--quiet"),
    );

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
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"mint-lift","protocol_version":"pep/1.7.0","capabilities":{}}}'
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

    let output = output_retrying_etxtbsy(
        Command::new(provekit_bin())
            .arg("mint")
            .arg("--project")
            .arg(&project)
            .arg("--out")
            .arg(&out_dir)
            .arg("--no-attest")
            .arg("--json")
            .arg("--quiet"),
    );

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

#[test]
fn mint_uses_path_document_from_project_config() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let project = dir.path().join("project");
    let manifest_dir = project.join(".provekit/lift/path-lift");
    let path_dir = project.join(".provekit/paths");
    let out_dir = dir.path().join("path-out");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::create_dir_all(&path_dir).expect("create path dir");

    let plugin = dir.path().join("path-lift-plugin.sh");
    write_executable(
        &plugin,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"path-lift","protocol_version":"pep/1.7.0","capabilities":{}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","ir":[{"kind":"contract","name":"path.config.contract","outBinding":"out","post":{"kind":"atomic","name":"path_config_true","args":[]}}],"diagnostics":[]}}'
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
            "name = \"path-lift\"\ncommand = [\"{}\"]\n",
            plugin.display()
        ),
    )
    .expect("write manifest");

    let lift_input = Input::Spec(json!({
        "surface": "path-lift",
        "workspace_root": project.canonicalize().unwrap_or_else(|_| project.clone()),
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {
            "layer": "all",
            "identifyOnly": false
        }
    }));
    let mint_input = Input::Spec(json!({
        "projectRoot": project.display().to_string(),
        "surface": "path-lift",
        "outDir": out_dir.display().to_string(),
        "options": {
            "quiet": true
        }
    }));
    let lift_input_cid = address(&lift_input);
    let mint_input_cid = address(&mint_input);
    let path = CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-plugin:path-lift".to_string(),
                inputs: vec![lift_input_cid],
                depends_on: vec![],
            },
            PathAlgebra {
                name: "mint".to_string(),
                kit: "provekit-mint".to_string(),
                inputs: vec![mint_input_cid],
                depends_on: vec!["lift".to_string()],
            },
        ],
    };
    let document = PathDocument::from_path_and_inputs(path, vec![lift_input, mint_input])
        .expect("build path document");
    fs::write(
        path_dir.join("mint.json"),
        serde_json::to_string_pretty(&document).expect("serialize path document"),
    )
    .expect("write path document");
    fs::write(
        project.join(".provekit/config.toml"),
        "[paths.mint]\nfile = \".provekit/paths/mint.json\"\n",
    )
    .expect("write config");

    let output = output_retrying_etxtbsy(
        Command::new(provekit_bin())
            .arg("mint")
            .arg("--project")
            .arg(&project)
            .arg("--no-attest")
            .arg("--json")
            .arg("--quiet"),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "mint should load PathDocument from [paths.mint], not require [authoring.lift]\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("mint JSON parses");
    assert_eq!(report["surface"], "path-lift");
    assert_eq!(
        report["proofFile"]
            .as_str()
            .map(|value| value.contains("path-out")),
        Some(true)
    );
    assert_eq!(report["lift"]["kind"], "ir-document");
}

#[test]
fn lift_python_emits_contracts_and_callsite_implications() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("test_parser.py"),
        r#"
def parse_int(raw):
    return int(raw)

def test_parse_value_scope():
    actual = parse_int("42")
    assert actual == 42

def test_direct_parse():
    assert parse_int("42") == 42

def test_two_callsites():
    assert parse_int("42") == parse_int("042")
"#,
    )
    .expect("write python fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "python"
"#,
    )
    .expect("write config");
    let manifest_dir = project.path().join(".provekit/lift/python");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let python_src = root.join("implementations/python/provekit-lift-py-tests/src");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"python-lift\"\ncommand = [\"env\", \"PYTHONPATH={}\", \"python3\", \"-m\", \"provekit_lift_py_tests.lsp\"]\nworking_dir = \".\"\n",
            python_src.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(project.path())
        .arg("--json")
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("spawn provekit lift python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift python failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    assert_eq!(report["kind"], "ir-document");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");
    assert_eq!(
        ir.len(),
        8,
        "expected callsite fact + assertion contracts: {report:#}"
    );
    assert_eq!(
        implications.len(),
        4,
        "expected one implication per lifted callsite: {report:#}"
    );
    let names: Vec<_> = ir
        .iter()
        .map(|decl| decl["name"].as_str().unwrap_or_default())
        .collect();
    assert!(names.iter().all(|name| name.starts_with("parse_int@")));
    for test_name in [
        "test_parse_value_scope",
        "test_direct_parse",
        "test_two_callsites",
    ] {
        assert!(names.iter().all(|name| !name.contains(test_name)));
    }
    assert_eq!(
        names
            .iter()
            .filter(|name| name.ends_with("::facts"))
            .count(),
        4
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| name.ends_with("::assertion"))
            .count(),
        4
    );
    for implication in implications {
        let antecedent = implication["antecedent"].as_str().unwrap_or_default();
        let consequent = implication["consequent"].as_str().unwrap_or_default();
        assert!(antecedent.ends_with("::facts"));
        assert!(consequent.ends_with("::assertion"));
        assert!(names.contains(&antecedent));
        assert!(names.contains(&consequent));
    }
}

#[test]
fn lift_python_emits_production_wp_callsite_implications() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("app.py"),
        r#"
def f(x):
    if x < 10:
        raise ValueError("x must be >= 10")
    return x

def caller():
    y = 42
    return f(y)
"#,
    )
    .expect("write python fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "python"
"#,
    )
    .expect("write config");
    let manifest_dir = project.path().join(".provekit/lift/python");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let python_src = root.join("implementations/python/provekit-lift-py-tests/src");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"python-lift\"\ncommand = [\"env\", \"PYTHONPATH={}\", \"python3\", \"-m\", \"provekit_lift_py_tests.lsp\"]\nworking_dir = \".\"\n",
            python_src.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(project.path())
        .arg("--json")
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("spawn provekit lift python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift python failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    assert_eq!(report["kind"], "ir-document");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");
    assert_eq!(
        ir.len(),
        3,
        "expected callsite, let, and entry WP edges: {report:#}"
    );
    assert_eq!(
        implications.len(),
        3,
        "expected one pre->post implication per WP edge: {report:#}"
    );

    let names: Vec<_> = ir
        .iter()
        .map(|decl| decl["name"].as_str().unwrap_or_default())
        .collect();
    assert!(names.iter().all(|name| name.starts_with("f@app.py:")));
    assert!(names.iter().any(|name| name.ends_with("::callsite")));
    assert!(names.iter().any(|name| name.ends_with("::let:y")));
    assert!(names.iter().any(|name| name.ends_with("::entry")));

    let let_edge = ir
        .iter()
        .find(|decl| {
            decl["name"]
                .as_str()
                .unwrap_or_default()
                .ends_with("::let:y")
        })
        .expect("let edge");
    assert_eq!(let_edge["pre"]["name"], "≥");
    assert_eq!(let_edge["pre"]["args"][0]["value"], 42);
    assert_eq!(let_edge["post"]["name"], "≥");
    assert_eq!(let_edge["post"]["args"][0]["name"], "y");

    for implication in implications {
        let antecedent = implication["antecedent"].as_str().unwrap_or_default();
        let consequent = implication["consequent"].as_str().unwrap_or_default();
        assert_eq!(antecedent, consequent);
        assert!(names.contains(&antecedent));
        assert_eq!(implication["antecedentSlot"], "pre");
        assert_eq!(implication["consequentSlot"], "post");
        assert_eq!(implication["prover"], "python-wp-walk");
    }
}

#[test]
fn lift_python_shows_production_composes_but_unittest_contracts_conflict() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("app.py"),
        r#"
import unittest

def checked(x):
    if x < 10:
        raise ValueError("x must be >= 10")
    return x

def composed_ok():
    y = 42
    return checked(y)

class CheckedContracts(unittest.TestCase):
    def test_checked_returns_42(self):
        actual = checked(42)
        self.assertEqual(actual, 42)

    def test_checked_does_not_return_42(self):
        actual = checked(42)
        self.assertNotEqual(actual, 42)
"#,
    )
    .expect("write python fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "python"
"#,
    )
    .expect("write config");
    let manifest_dir = project.path().join(".provekit/lift/python");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let python_src = root.join("implementations/python/provekit-lift-py-tests/src");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"python-lift\"\ncommand = [\"env\", \"PYTHONPATH={}\", \"python3\", \"-m\", \"provekit_lift_py_tests.lsp\"]\nworking_dir = \".\"\n",
            python_src.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(project.path())
        .arg("--json")
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("spawn provekit lift python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift python failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");

    let production: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.py:")
                && (name.ends_with("::callsite")
                    || name.ends_with("::let:y")
                    || name.ends_with("::entry"))
        })
        .collect();
    assert_eq!(
        production.len(),
        3,
        "expected production callsite, let, and entry WP edges: {report:#}"
    );
    let let_edge = production
        .iter()
        .copied()
        .find(|decl| {
            decl["name"]
                .as_str()
                .unwrap_or_default()
                .ends_with("::let:y")
        })
        .expect("let edge");
    assert_eq!(let_edge["pre"]["name"], "≥");
    assert_eq!(let_edge["pre"]["args"][0]["value"], 42);
    assert_eq!(let_edge["pre"]["args"][1]["value"], 10);

    let test_assertions: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.py:") && name.ends_with("::assertion")
        })
        .collect();
    assert_eq!(
        test_assertions.len(),
        2,
        "expected two unittest-derived assertion contracts: {report:#}"
    );
    let mut assertion_ops: Vec<_> = test_assertions
        .iter()
        .map(|decl| decl["inv"]["name"].as_str().unwrap_or_default())
        .collect();
    assertion_ops.sort_unstable();
    assert_eq!(assertion_ops, vec!["=", "≠"]);

    let wp_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "python-wp-walk")
        .count();
    let test_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "python-test-value-scope")
        .count();
    assert_eq!(wp_implications, 3);
    assert_eq!(test_implications, 2);
}

#[test]
fn lift_zig_shows_production_composes_but_unit_tests_conflict() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("app.zig"),
        r#"
const std = @import("std");

fn checked(x: i32) !i32 {
    if (x < 10) return error.TooSmall;
    return x;
}

fn composedOk() !i32 {
    const y = 42;
    return checked(y);
}

test "checked returns 42" {
    const actual = try checked(42);
    try std.testing.expectEqual(@as(i32, 42), actual);
}

test "checked does not return 42" {
    const actual = try checked(42);
    try std.testing.expect(actual != 42);
}
"#,
    )
    .expect("write zig fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "zig"
"#,
    )
    .expect("write config");
    let shim = project.path().join("zig-lift.sh");
    write_executable(
        &shim,
        &format!(
            "#!/usr/bin/env sh\ncd '{}'\nexec zig build run -- \"$@\"\n",
            root.join("implementations/zig/provekit-lift-zig").display()
        ),
    );
    let manifest_dir = project.path().join(".provekit/lift/zig");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        "name = \"zig-lift\"\ncommand = [\"./zig-lift.sh\", \"--rpc\"]\nworking_dir = \".\"\n",
    )
    .expect("write manifest");

    let output = output_retrying_etxtbsy(
        Command::new(provekit_bin())
            .arg("lift")
            .arg(project.path())
            .arg("--json")
            .arg("--quiet")
            .current_dir(&root),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift zig failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");

    let production: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.zig:")
                && (name.ends_with("::callsite")
                    || name.ends_with("::let:y")
                    || name.ends_with("::entry"))
        })
        .collect();
    assert_eq!(
        production.len(),
        3,
        "expected production callsite, let, and entry WP edges: {report:#}"
    );
    let let_edge = production
        .iter()
        .copied()
        .find(|decl| {
            decl["name"]
                .as_str()
                .unwrap_or_default()
                .ends_with("::let:y")
        })
        .expect("let edge");
    assert_eq!(let_edge["pre"]["name"], "≥");
    assert_eq!(let_edge["pre"]["args"][0]["value"], 42);
    assert_eq!(let_edge["pre"]["args"][1]["value"], 10);

    let test_assertions: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.zig:") && name.ends_with("::assertion")
        })
        .collect();
    assert_eq!(
        test_assertions.len(),
        2,
        "expected two Zig test-derived assertion contracts: {report:#}"
    );
    let mut assertion_ops: Vec<_> = test_assertions
        .iter()
        .map(|decl| decl["inv"]["name"].as_str().unwrap_or_default())
        .collect();
    assertion_ops.sort_unstable();
    assert_eq!(assertion_ops, vec!["=", "≠"]);

    let wp_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "zig-wp-walk")
        .count();
    let test_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "zig-test-value-scope")
        .count();
    assert_eq!(wp_implications, 3);
    assert_eq!(test_implications, 2);
}

#[test]
fn lift_cpp_shows_production_composes_but_unit_tests_conflict() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("app.cpp"),
        r#"
#include <stdexcept>

int checked(int x) {
    if (x < 10) throw std::invalid_argument("too small");
    return x;
}

int composed_ok() {
    int y = 42;
    return checked(y);
}

TEST(CheckedContracts, returns_42) {
    int actual = checked(42);
    EXPECT_EQ(actual, 42);
}

TEST(CheckedContracts, does_not_return_42) {
    int actual = checked(42);
    EXPECT_NE(actual, 42);
}
"#,
    )
    .expect("write cpp fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "cpp"
"#,
    )
    .expect("write config");
    let shim = project.path().join("cpp-lift.sh");
    write_executable(
        &shim,
        &format!(
            "#!/usr/bin/env sh\nset -eu\nbin=\"$PWD/cpp-lift-bin\"\nclang++ -std=c++17 -O0 -Wall -Wextra -I'{}' '{}' -o \"$bin\"\nexec \"$bin\" --workspace \"$PWD\" \"$@\"\n",
            root.join("implementations/cpp/provekit-ir-symbolic/include").display(),
            root.join("implementations/cpp/provekit-lift-cpp/main.cpp").display()
        ),
    );
    let manifest_dir = project.path().join(".provekit/lift/cpp");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        "name = \"cpp-lift\"\ncommand = [\"./cpp-lift.sh\"]\nworking_dir = \".\"\n",
    )
    .expect("write manifest");

    let output = output_retrying_etxtbsy(
        Command::new(provekit_bin())
            .arg("lift")
            .arg(project.path())
            .arg("--json")
            .arg("--quiet")
            .current_dir(&root),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift cpp failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");

    let production: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.cpp:")
                && (name.ends_with("::callsite")
                    || name.ends_with("::let:y")
                    || name.ends_with("::entry"))
        })
        .collect();
    assert_eq!(
        production.len(),
        3,
        "expected production callsite, let, and entry WP edges: {report:#}"
    );
    let let_edge = production
        .iter()
        .copied()
        .find(|decl| {
            decl["name"]
                .as_str()
                .unwrap_or_default()
                .ends_with("::let:y")
        })
        .expect("let edge");
    assert_eq!(let_edge["pre"]["name"], "≥");
    assert_eq!(let_edge["pre"]["args"][0]["value"], 42);
    assert_eq!(let_edge["pre"]["args"][1]["value"], 10);

    let test_assertions: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.cpp:") && name.ends_with("::assertion")
        })
        .collect();
    assert_eq!(
        test_assertions.len(),
        2,
        "expected two C++ test-derived assertion contracts: {report:#}"
    );
    let mut assertion_ops: Vec<_> = test_assertions
        .iter()
        .map(|decl| decl["inv"]["name"].as_str().unwrap_or_default())
        .collect();
    assertion_ops.sort_unstable();
    assert_eq!(assertion_ops, vec!["=", "≠"]);

    let wp_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "cpp-wp-walk")
        .count();
    let test_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "cpp-test-value-scope")
        .count();
    assert_eq!(wp_implications, 3);
    assert_eq!(test_implications, 2);
}

#[test]
fn lift_php_shows_production_composes_but_unit_tests_conflict() {
    let root = repo_root();
    let project = tempfile::tempdir().expect("create tempdir");
    fs::write(
        project.path().join("app.php"),
        r#"<?php

function checked($x) {
    if ($x < 10) {
        throw new InvalidArgumentException("too small");
    }
    return $x;
}

function composed_ok() {
    $y = 42;
    return checked($y);
}

final class CheckedContracts extends TestCase {
    public function testCheckedReturns42(): void {
        $actual = checked(42);
        $this->assertSame(42, $actual);
    }

    public function testCheckedDoesNotReturn42(): void {
        $actual = checked(42);
        $this->assertNotSame(42, $actual);
    }
}
"#,
    )
    .expect("write php fixture");
    fs::create_dir_all(project.path().join(".provekit")).expect("create config dir");
    fs::write(
        project.path().join(".provekit/config.toml"),
        r#"[authoring.lift]
surface = "php"
"#,
    )
    .expect("write config");
    let manifest_dir = project.path().join(".provekit/lift/php");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"php-lift\"\ncommand = [\"php\", \"provekit-lift/src/lifter.php\", \"--rpc\"]\nworking_dir = \"{}\"\n",
            root.join("implementations/php").display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(project.path())
        .arg("--json")
        .arg("--quiet")
        .current_dir(&root)
        .output()
        .expect("spawn provekit lift php");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit lift php failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("lift JSON parses");
    let ir = report["ir"].as_array().expect("ir array");
    let implications = report["implications"]
        .as_array()
        .expect("implications array");

    let production: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.php:")
                && (name.ends_with("::callsite")
                    || name.ends_with("::let:y")
                    || name.ends_with("::entry"))
        })
        .collect();
    assert_eq!(
        production.len(),
        3,
        "expected production callsite, let, and entry WP edges: {report:#}"
    );
    let let_edge = production
        .iter()
        .copied()
        .find(|decl| {
            decl["name"]
                .as_str()
                .unwrap_or_default()
                .ends_with("::let:y")
        })
        .expect("let edge");
    assert_eq!(let_edge["pre"]["name"], "≥");
    assert_eq!(let_edge["pre"]["args"][0]["value"], 42);
    assert_eq!(let_edge["pre"]["args"][1]["value"], 10);

    let test_assertions: Vec<_> = ir
        .iter()
        .filter(|decl| {
            let name = decl["name"].as_str().unwrap_or_default();
            name.starts_with("checked@app.php:") && name.ends_with("::assertion")
        })
        .collect();
    assert_eq!(
        test_assertions.len(),
        2,
        "expected two PHPUnit-derived assertion contracts: {report:#}"
    );
    let mut assertion_ops: Vec<_> = test_assertions
        .iter()
        .map(|decl| decl["inv"]["name"].as_str().unwrap_or_default())
        .collect();
    assertion_ops.sort_unstable();
    assert_eq!(assertion_ops, vec!["=", "≠"]);

    let wp_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "php-wp-walk")
        .count();
    let test_implications = implications
        .iter()
        .filter(|imp| imp["prover"] == "php-test-value-scope")
        .count();
    assert_eq!(wp_implications, 3);
    assert_eq!(test_implications, 2);
}
