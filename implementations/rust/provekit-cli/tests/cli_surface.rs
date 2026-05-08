// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
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
