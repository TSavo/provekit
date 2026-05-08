// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;

use provekit_bug_zoo::{run, OutputFlags, ZooArgs};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn runner_help_is_self_contained() {
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bug-zoo"))
        .arg("--help")
        .output()
        .expect("spawn provekit-bug-zoo --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bug-zoo --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("provekit-bug-zoo"));
    assert!(stdout.contains("--all"));
    assert!(!stdout.contains("provekit zoo"));
}

#[test]
fn all_specimens_pass() {
    let root = repo_root();
    let code = run(ZooArgs {
        specimen: Some(root.join("bug-zoo/species")),
        all: true,
        out: OutputFlags {
            quiet: true,
            json: false,
        },
    });
    assert_eq!(code, 0, "one or more bug zoo specimens failed");
}

#[test]
fn all_specimens_reports_current_shapes() {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bug-zoo"))
        .arg(root.join("bug-zoo/species"))
        .arg("--all")
        .arg("--json")
        .current_dir(&root)
        .output()
        .expect("spawn provekit-bug-zoo --all --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bug-zoo --all --json failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("bug zoo JSON report parses");
    assert_eq!(report["ok"], true);
    let reports = report["reports"].as_array().expect("reports is an array");
    assert_eq!(
        reports.len(),
        2,
        "bug zoo reports the current shape species"
    );

    let null_boundary = reports
        .iter()
        .find(|entry| entry["id"] == "BZ-SHAPE-005")
        .expect("null-boundary species is reported");
    let null_languages = null_boundary["languages"].as_array().unwrap();
    assert_eq!(null_languages.len(), 3);
    let null_composition_count: usize = null_languages
        .iter()
        .map(|language| language["composition"].as_array().unwrap().len())
        .sum();
    assert_eq!(null_composition_count, 14);
    assert!(
        null_languages.iter().all(|language| {
            language["lab"]["provekitWorkflow"] == "none"
                && language["composition"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|check| {
                        check["witnessSource"] == "lab"
                            && check["provekitSignal"] == "red"
                            && check["provekitStatus"] == "unsatisfied"
                    })
                && language["composition"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|check| {
                        check["witnessSource"] == "proof-ir"
                            && check["provekitSignal"] == "green"
                            && check["provekitStatus"] == "discharged"
                    })
        }),
        "BZ-SHAPE-005 should route lab red and fixed green obligations through provekit"
    );

    let value_scope = reports
        .iter()
        .find(|entry| entry["id"] == "BZ-SHAPE-006")
        .expect("value-scope species is reported");
    assert_eq!(
        value_scope["missingEdge"],
        "eq(value, 42) => gte(value, 43)"
    );
    let languages = value_scope["languages"].as_array().unwrap();
    assert_eq!(languages.len(), 1);
    assert_eq!(languages[0]["id"], "java");
    assert_eq!(languages[0]["lab"]["provekitWorkflow"], "none");
    assert_eq!(languages[0]["proofIrCids"].as_object().unwrap().len(), 2);
    let composition = languages[0]["composition"].as_array().unwrap();
    assert_eq!(
        composition.len(),
        4,
        "JUnit and Spring exhibits each carry exhibit/fixed composition checks"
    );
    assert!(
        composition.iter().any(|check| check["phase"] == "exhibit"
            && check["provekitSignal"] == "red"
            && check["provedBy"] == "provekit prove --formula"),
        "exhibit checks should carry a red provekit prove signal"
    );
    assert!(
        composition.iter().any(|check| check["phase"] == "fixed"
            && check["provekitSignal"] == "green"
            && check["provedBy"] == "provekit prove --formula"),
        "fixed checks should carry a green provekit prove signal"
    );
}

#[test]
fn csharp_discover_cli_finds_null_boundary_with_language_lifter() {
    let root = repo_root();
    let project = root.join("implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj");
    let harness = root.join(
        "bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness",
    );

    let build = Command::new("dotnet")
        .arg("build")
        .arg(&project)
        .arg("--nologo")
        .arg("--verbosity")
        .arg("quiet")
        .current_dir(&root)
        .output()
        .expect("build csharp discover cli");

    let build_stdout = String::from_utf8_lossy(&build.stdout);
    let build_stderr = String::from_utf8_lossy(&build.stderr);
    assert!(
        build.status.success(),
        "csharp discover build failed\nstdout:\n{build_stdout}\nstderr:\n{build_stderr}"
    );

    let output = Command::new("dotnet")
        .arg("run")
        .arg("--project")
        .arg(project)
        .arg("--no-build")
        .arg("--no-restore")
        .arg("--")
        .arg("discover")
        .arg("csharp-linq")
        .arg(harness)
        .current_dir(&root)
        .output()
        .expect("spawn csharp discover cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "csharp discover failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("\"kind\":\"bug-zoo-discovery\""));
    assert!(stdout.contains("\"surface\":\"csharp-linq\""));
    assert!(stdout.contains("\"lifter\":\"LinqLifter\""));
    assert!(stdout.contains("\"missingEdge\":\"maybe_null(name) => non_null(name)\""));
    assert!(stdout.contains("\"irEvidenceCid\":"));
}

#[test]
fn typescript_discover_cli_finds_null_boundary_with_language_lifter() {
    let root = repo_root();
    let discover = root.join(
        "bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts",
    );
    let harness = root.join(
        "bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness",
    );

    let output = Command::new("pnpm")
        .arg("exec")
        .arg("tsx")
        .arg(discover)
        .arg("zod")
        .arg(harness)
        .current_dir(&root)
        .output()
        .expect("spawn typescript discover cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "typescript discover failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("\"kind\":\"bug-zoo-discovery\""));
    assert!(stdout.contains("\"surface\":\"zod\""));
    assert!(stdout.contains("\"lifter\":\"liftPath\""));
    assert!(stdout.contains("\"missingEdge\":\"maybe_null(name) => non_null(name)\""));
    assert!(stdout.contains("\"irEvidenceCid\":"));
}
