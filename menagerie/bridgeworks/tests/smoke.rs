// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::{env, ffi::OsString, fs};

use provekit_bridgeworks::{run, BridgeworksArgs, OutputFlags};

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
            external_cli: env::var_os("PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI"),
        };
        env::remove_var("PROVEKIT_CLI");
        env::remove_var("PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI");
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
            env::set_var("PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI", value);
        } else {
            env::remove_var("PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI");
        }
    }
}

#[test]
fn runner_help_is_self_contained() {
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bridgeworks"))
        .arg("--help")
        .output()
        .expect("spawn provekit-bridgeworks --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bridgeworks --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("provekit-bridgeworks"));
    assert!(stdout.contains("--all"));
    assert!(!stdout.contains("provekit zoo"));
}

#[test]
fn walkthrough_invokes_named_binaries_not_cargo_run() {
    let root = repo_root();
    let exhibit = root.join("menagerie/bridgeworks/checked-add-u8");
    let mut checked = Vec::new();

    for entry in fs::read_dir(exhibit.join("walkthrough")).expect("read walkthrough directory") {
        let entry = entry.expect("read walkthrough entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("sh") {
            checked.push(path);
        }
    }
    checked.push(exhibit.join("kit-rpc/run-bridgeworks-lifter.sh"));
    checked.push(exhibit.join("kit-rpc/run-bridgeworks-c-lowerer.sh"));

    for path in checked {
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            !text.contains("cargo run"),
            "{} should invoke named binaries, not cargo run",
            path.display()
        );
    }

    for script in [
        "02-show-native-contracts.sh",
        "03-lift-to-proofir.sh",
        "04-show-bridge-edges.sh",
    ] {
        let path = exhibit.join("walkthrough").join(script);
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            text.contains("print_provekit lift"),
            "{} should demonstrate lift through the provekit CLI",
            path.display()
        );
        if script == "02-show-native-contracts.sh" {
            assert!(
                text.contains("--identify-only"),
                "{} should use identify-only lift",
                path.display()
            );
        }
        assert!(
            !text.contains("print_lifter_self_test") && !text.contains("run_lifter_self_test"),
            "{} should not call the Bridgeworks lifter directly",
            path.display()
        );
    }
}

#[test]
fn walkthrough_surfaces_lifted_provekit_json() {
    let root = repo_root();
    let exhibit = root.join("menagerie/bridgeworks/checked-add-u8");

    for (script, section) in [
        ("02-show-native-contracts.sh", "Lifted Identifier JSON"),
        ("03-lift-to-proofir.sh", "Lifted ProofIR JSON"),
        ("04-show-bridge-edges.sh", "Raw Bridge Edge Lines"),
    ] {
        let path = exhibit.join("walkthrough").join(script);
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            text.contains(section),
            "{} should show the actual provekit JSON output section `{section}`",
            path.display()
        );
        assert!(
            text.contains("show_json_file"),
            "{} should print captured provekit JSON, not only derived summaries",
            path.display()
        );
        assert!(
            text.contains("highlight_raw_line"),
            "{} should highlight raw line-numbered fields after showing the full output",
            path.display()
        );
    }
}

#[test]
fn walkthrough_surfaces_lowered_c_emitter() {
    let root = repo_root();
    let path =
        root.join("menagerie/bridgeworks/checked-add-u8/walkthrough/15-break-software-witness.sh");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    assert!(
        text.contains("Raw Lowered C Emitter Line"),
        "{} should show the C emitter generated by `provekit lower`",
        path.display()
    );
    assert!(
        text.contains("witnessArtifact.source") || text.contains("\"source\": \"#include"),
        "{} should read the generated C emitter from the lower result JSON",
        path.display()
    );
    assert!(
        text.contains("highlight_raw_line"),
        "{} should highlight raw line-numbered fields after showing the full lower output",
        path.display()
    );
}

#[test]
fn walkthrough_breakages_show_raw_diffs() {
    let root = repo_root();
    let exhibit = root.join("menagerie/bridgeworks/checked-add-u8");
    let common = fs::read_to_string(exhibit.join("walkthrough/common.sh"))
        .expect("read walkthrough common.sh");

    assert!(
        common.contains("show_mutation_diff"),
        "shared mutation runner should print raw unified diffs"
    );
    assert!(
        common.contains("diff -u"),
        "mutation diffs should be real unified diffs"
    );

    let witness = fs::read_to_string(exhibit.join("walkthrough/15-break-software-witness.sh"))
        .expect("read witness break script");
    assert!(
        witness.contains("show_mutation_diff"),
        "software witness break should show the C source diff before lowering"
    );
}

#[test]
fn walkthrough_scripts_explain_then_prompt_before_work() {
    let root = repo_root();
    let exhibit = root.join("menagerie/bridgeworks/checked-add-u8");
    let common = fs::read_to_string(exhibit.join("walkthrough/common.sh"))
        .expect("read walkthrough common.sh");

    assert!(
        common.contains("explain_then_pause"),
        "walkthrough should have a shared explanation/pause primitive"
    );
    assert!(
        common.contains("analysis_with_receipts"),
        "walkthrough should have a shared post-output analysis primitive"
    );
    assert!(
        common.contains("Press Enter"),
        "interactive walkthrough runs should prompt before executing each step"
    );
    assert!(
        common.contains("[ ! -t 0 ]"),
        "non-interactive walkthrough runs should not hang waiting for input"
    );

    for script in [
        "00-start-here.sh",
        "01-map-stack.sh",
        "02-show-native-contracts.sh",
        "03-lift-to-proofir.sh",
        "04-show-bridge-edges.sh",
        "05-mint-proof-dag.sh",
        "06-walk-proof-cids.sh",
        "07-break-experiment.sh",
        "08-break-device-physics.sh",
        "09-break-cells.sh",
        "10-break-gates.sh",
        "11-break-rtl.sh",
        "12-break-isa.sh",
        "13-break-compiler.sh",
        "14-break-software-identity.sh",
        "15-break-software-witness.sh",
        "16-run-whole-exhibit.sh",
    ] {
        let path = exhibit.join("walkthrough").join(script);
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for required in [
            "What ProvekIt is doing here:",
            "Value ProvekIt adds:",
            "Relationship to the chain:",
            "What to look for:",
        ] {
            assert!(
                text.contains(required),
                "{script} should include briefing section `{required}`"
            );
        }
        let explain = text
            .find("explain_then_pause")
            .unwrap_or_else(|| panic!("{script} should explain and prompt before doing work"));
        let work = [
            "ensure_walkthrough_bins",
            "cat <<",
            "print_provekit",
            "mint_positive",
            "run_mutation_expect_refusal",
            "show_mutation_diff",
            "print_bridgeworks",
        ]
        .iter()
        .filter_map(|needle| text.find(needle))
        .min()
        .unwrap_or_else(|| panic!("{script} should have a recognized walkthrough action"));
        assert!(
            explain < work,
            "{script} should explain and prompt before its first walkthrough action"
        );
        let analysis = text
            .find("analysis_with_receipts")
            .unwrap_or_else(|| panic!("{script} should analyze the machine output with receipts"));
        assert!(
            work < analysis,
            "{script} should analyze with receipts after machine output begins"
        );
    }

    for (script, minimum_phases) in [
        ("06-walk-proof-cids.sh", 2),
        ("15-break-software-witness.sh", 3),
    ] {
        let path = exhibit.join("walkthrough").join(script);
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let phases = text.matches("explain_then_pause").count();
        assert!(
            phases >= minimum_phases,
            "{script} should have at least {minimum_phases} explanatory phases, found {phases}"
        );
    }
}

#[test]
fn walkthrough_binary_prep_ignores_test_only_cli_edits() {
    let root = repo_root();
    let common = root.join("menagerie/bridgeworks/checked-add-u8/walkthrough/common.sh");
    let text =
        fs::read_to_string(&common).unwrap_or_else(|e| panic!("read {}: {e}", common.display()));

    assert!(
        text.contains("source_newer_than_binary"),
        "walkthrough binary prep should check scoped runtime sources"
    );
    assert!(
        text.contains("printf \"  %6d: %s\\n\", NR, $0"),
        "walkthrough JSON output should be line-numbered before highlights refer back to it"
    );
    assert!(
        text.contains("provekit-cli/src"),
        "provekit rebuild checks should include CLI runtime sources"
    );
    assert!(
        text.contains("bridgeworks-lifter.rs") && text.contains("bridgeworks-c-witness-lowerer.rs"),
        "kit rebuild checks should compare each binary to its own source file"
    );
    assert!(
        !text.contains("\"$REPO_ROOT/implementations/rust\""),
        "provekit rebuild checks should not scan all Rust tests and unrelated crates"
    );
}

#[test]
fn checked_add_exhibit_passes() {
    let _guard = shared_host_tool_lock();
    let _cli_env = CliEnvGuard::force_source_cli();
    let root = repo_root();
    let code = run(BridgeworksArgs {
        specimen: Some(root.join("menagerie/bridgeworks/checked-add-u8")),
        all: false,
        out: OutputFlags {
            quiet: true,
            json: false,
        },
    });
    assert_eq!(code, 0, "Bridgeworks checked-add exhibit failed");
}

#[test]
fn all_exhibits_reports_contract_and_implication_mementos() {
    let _guard = shared_host_tool_lock();
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bridgeworks"))
        .arg(root.join("menagerie/bridgeworks"))
        .arg("--all")
        .arg("--json")
        .current_dir(&root)
        .env_remove("PROVEKIT_CLI")
        .env_remove("PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI")
        .output()
        .expect("spawn provekit-bridgeworks --all --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bridgeworks --all --json failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("Bridgeworks JSON report parses");
    assert_eq!(report["ok"], true);
    let reports = report["reports"].as_array().expect("reports array");
    assert_eq!(reports.len(), 1);
    let exhibit = &reports[0];
    assert_eq!(exhibit["id"], "bridgeworks-checked-add-u8");
    assert!(exhibit["proofCid"]
        .as_str()
        .unwrap_or_default()
        .starts_with("blake3-512:"));
    assert_eq!(exhibit["memberCounts"]["contract"], 8);
    assert_eq!(exhibit["memberCounts"]["implication"], 7);
    assert_eq!(exhibit["memberCounts"]["authority"], 16);
    let mutations = exhibit["mutations"].as_array().unwrap();
    assert_eq!(mutations.len(), 9);
    assert!(mutations
        .iter()
        .all(|mutation| mutation["refused"] == true && mutation["expectedRefusalMatched"] == true));
    assert!(mutations
        .iter()
        .any(|mutation| mutation["id"] == "software-overflow-add-u8"
            && mutation["expectedRefusal"] == "checked_add_u8.postcondition"));
    let software = mutations
        .iter()
        .find(|mutation| mutation["id"] == "software-overflow-add-u8")
        .expect("software overflow mutation report");
    let software_error = software["detail"]["error"]
        .as_str()
        .expect("software overflow mutation error");
    assert!(software_error.contains(
        "bridge edge failed: compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition"
    ));
    assert!(software_error.contains("needed: checked_add_u8.postcondition"));
    assert!(software_error.contains("software emitted: overflow_add_u8.postcondition"));
    let counterfeit = mutations
        .iter()
        .find(|mutation| mutation["id"] == "software-counterfeit-contract")
        .expect("software counterfeit mutation report");
    let counterfeit_error = counterfeit["detail"]["error"]
        .as_str()
        .expect("software counterfeit mutation error");
    assert!(counterfeit_error.contains("ORP witness failed: checked_add_u8.postcondition"));
    assert!(counterfeit_error.contains("counterexample: a=1 b=255"));
    assert!(counterfeit_error.contains("expected: overflow=true value=0"));
    assert!(counterfeit_error.contains("observed: overflow=false value=0"));
}
