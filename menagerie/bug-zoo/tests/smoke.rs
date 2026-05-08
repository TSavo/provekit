// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::{env, ffi::OsString};

use provekit_bug_zoo::{run, OutputFlags, ZooArgs};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn shared_host_tool_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("shared host tool lock poisoned")
}

struct CliEnvGuard {
    provekit_cli: Option<OsString>,
    external_cli: Option<OsString>,
}

impl CliEnvGuard {
    fn force_source_cli() -> Self {
        let guard = Self {
            provekit_cli: env::var_os("PROVEKIT_CLI"),
            external_cli: env::var_os("PROVEKIT_BUG_ZOO_EXTERNAL_CLI"),
        };
        env::remove_var("PROVEKIT_CLI");
        env::remove_var("PROVEKIT_BUG_ZOO_EXTERNAL_CLI");
        guard
    }
}

impl Drop for CliEnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.provekit_cli.take() {
            env::set_var("PROVEKIT_CLI", value);
        } else {
            env::remove_var("PROVEKIT_CLI");
        }
        if let Some(value) = self.external_cli.take() {
            env::set_var("PROVEKIT_BUG_ZOO_EXTERNAL_CLI", value);
        } else {
            env::remove_var("PROVEKIT_BUG_ZOO_EXTERNAL_CLI");
        }
    }
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
    let _guard = shared_host_tool_lock();
    let _cli_env = CliEnvGuard::force_source_cli();
    let root = repo_root();
    let code = run(ZooArgs {
        specimen: Some(root.join("menagerie/bug-zoo/species")),
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
    let _guard = shared_host_tool_lock();
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bug-zoo"))
        .arg(root.join("menagerie/bug-zoo/species"))
        .arg("--all")
        .arg("--json")
        .current_dir(&root)
        .env_remove("PROVEKIT_CLI")
        .env_remove("PROVEKIT_BUG_ZOO_EXTERNAL_CLI")
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
        3,
        "bug zoo reports the current shape species"
    );
    assert!(
        reports.iter().all(|entry| {
            entry["workflow"]["runner"] == "provekit-bug-zoo"
                && entry["workflow"]["provekitCli"]["kind"] == "cargo-run-source"
        }),
        "Bug Zoo receipts should report the current source-routed provekit CLI"
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

    let polyglot = reports
        .iter()
        .find(|entry| entry["id"] == "BZ-SHAPE-007")
        .expect("polyglot link species is reported");
    assert_eq!(polyglot["missingEdge"], "post_caller => pre_callee");
    assert_eq!(polyglot["proofIrCids"].as_object().unwrap().len(), 0);
    assert_eq!(polyglot["receiptCids"].as_object().unwrap().len(), 2);
    let languages = polyglot["languages"].as_array().unwrap();
    assert_eq!(languages.len(), 1);
    assert_eq!(languages[0]["id"], "rust-go");
    assert_eq!(languages[0]["lab"]["provekitWorkflow"], "none");
    assert_eq!(languages[0]["proofIrCids"].as_object().unwrap().len(), 0);
    assert_eq!(languages[0]["linkBundleCids"].as_object().unwrap().len(), 1);
    assert_eq!(
        languages[0]["fixedLinkBundleCids"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn polyglot_fixed_link_bundle_keeps_cross_kit_bridge() {
    let root = repo_root();
    let bundle_path = root.join(
        "menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/fixed/cgo-rust-callee/harness/link-bundle.json",
    );
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(bundle_path).expect("read fixed bundle"))
            .expect("parse fixed bundle");

    assert_eq!(bundle["linkerErrors"].as_array().unwrap().len(), 0);
    let bridges = bundle["bridges"].as_array().unwrap();
    assert_eq!(
        bridges.len(),
        1,
        "fixed BZ-007 must close the same Go->Rust edge, not erase it"
    );
    assert_eq!(
        bridges[0]["metadata"]["callSite"]["file"],
        "menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/fixed/cgo-rust-callee/harness/go-caller/caller_ok.go"
    );
}

#[test]
fn csharp_discover_cli_finds_null_boundary_with_language_lifter() {
    let _guard = shared_host_tool_lock();
    let root = repo_root();
    let project = root.join("implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj");
    let harness = root.join(
        "menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness",
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
        "menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts",
    );
    let harness = root.join(
        "menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness",
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
