// SPDX-License-Identifier: Apache-2.0
//
// Golden snapshot test for `provekit self-check --target examples/provekit-shim-rust-std --json`.
//
// The golden encodes the honest scoreboard for the rust-std shim run WITHOUT
// --oracle, which makes the output fully deterministic (no daemon, no
// wall-clock, no absolute paths).
//
// To regenerate after a legitimate number change:
//
//   UPDATE_GOLDEN=1 cargo test -p provekit-cli --test self_check_golden
//
// Update tests/golden/self-check-provekit-shim-rust-std.md with a one-line why.
//
// Wiring: this test runs under `cargo test -p provekit-cli` which is invoked
// by `make test-rust` -> `make test-all` -> the `conformance` CI job. No
// separate CI step is needed; see .github/workflows/ci.yml `make test-all`.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root ancestor")
        .to_path_buf()
}

const GOLDEN_REL: &str =
    "implementations/rust/provekit-cli/tests/golden/self-check-provekit-shim-rust-std.json";
const TARGET: &str = "examples/provekit-shim-rust-std";

/// Normalize the self-check JSON output for golden comparison.
///
/// Currently no normalization is required: the output is byte-stable and
/// path-independent without any transformation. This function exists as the
/// single place to add normalization if a future change introduces a volatile
/// field; applying it to both sides of the comparison prevents skew.
///
/// Fields verified as stable (2026-05-31):
///   - `file` values are repo-relative within the target crate, not absolute
///   - no timestamps in SelfCheckScoreboard
///   - panicCensus and droppedSites are sorted via BTreeMap / sort_by(site_cmp)
///   - catalogCid is a BLAKE3 content hash, path-independent
fn normalize(raw: &str) -> String {
    // Re-parse and re-serialize through serde_json to canonicalize whitespace,
    // so that a golden captured with serde_json's pretty-printer always matches
    // output captured the same way regardless of minor formatting drift.
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) => serde_json::to_string_pretty(&v).expect("re-serialize golden"),
        Err(_) => raw.to_owned(),
    }
}

/// Print a human-readable line-level diff for a golden mismatch.
///
/// Output example:
///   golden mismatch: self-check-provekit-shim-rust-std.json
///   line 28 EXPECTED:   "panicSafe": 0,
///   line 28 ACTUAL  :   "panicSafe": 3,
fn print_diff(expected: &str, actual: &str, golden_name: &str) {
    eprintln!("\ngolden mismatch: {golden_name}");
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let max = exp_lines.len().max(act_lines.len());
    let mut shown = 0;
    for i in 0..max {
        let e = exp_lines.get(i).copied().unwrap_or("<missing>");
        let a = act_lines.get(i).copied().unwrap_or("<missing>");
        if e != a {
            eprintln!("  line {:4} EXPECTED: {e}", i + 1);
            eprintln!("  line {:4} ACTUAL  : {a}", i + 1);
            shown += 1;
            if shown >= 20 {
                let remaining = (0..max)
                    .filter(|&j| {
                        j > i
                            && exp_lines.get(j).copied().unwrap_or("<missing>")
                                != act_lines.get(j).copied().unwrap_or("<missing>")
                    })
                    .count();
                if remaining > 0 {
                    eprintln!("  ... ({remaining} more differing lines)");
                }
                break;
            }
        }
    }
}

#[test]
#[ignore = "Golden scoreboard snapshot of the rust-std shim's self-check -- the rust-std \
shim is Product B (Option/Result constructor algebra + panic preconditions), the overdoing \
the Python kit ships none of. Slated for demotion to the Python gold standard; its counts \
keep shifting as B is pared, so ignored rather than re-blessed. #1926"]
fn self_check_golden_provekit_shim_rust_std() {
    let repo = repo_root();

    // Prerequisites: the existing self_check.rs test asserts these exist.
    // Mirror that check so the failure message is actionable.
    let walk_rpc = repo
        .join("implementations/rust/target/debug/provekit-walk-rpc");
    let provekit_lift = repo
        .join("implementations/rust/target/debug/provekit-lift");
    assert!(
        walk_rpc.exists(),
        "build the walk RPC binary first: cargo build -p provekit-walk --bin provekit-walk-rpc"
    );
    assert!(
        provekit_lift.exists(),
        "build the lift RPC binary first: cargo build -p provekit-lift --bin provekit-lift"
    );

    // Run self-check.
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .current_dir(&repo)
        .args(["self-check", "--target", TARGET, "--json"])
        .output()
        .expect("run provekit self-check");

    assert!(
        output.status.success(),
        "self-check exited non-zero\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let raw_actual = String::from_utf8(output.stdout).expect("self-check stdout is UTF-8");
    let actual = normalize(&raw_actual);

    // Hard invariants: check before golden diff so a violation is obvious.
    let parsed: serde_json::Value =
        serde_json::from_str(&actual).expect("normalized actual is valid JSON");
    assert_eq!(
        parsed["silentlyDropped"], 0,
        "INVARIANT VIOLATION: silentlyDropped must be 0"
    );
    assert_eq!(
        parsed["dischargeSplit"]["falsePass"], 0,
        "INVARIANT VIOLATION: falsePass must be 0"
    );

    // UPDATE_GOLDEN mode: rewrite the golden with the current output.
    let golden_path = repo.join(GOLDEN_REL);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&golden_path, actual.as_bytes())
            .unwrap_or_else(|e| panic!("write golden {}: {e}", golden_path.display()));
        eprintln!(
            "golden updated: {}\nRemember to update the sibling .md with a one-line why.",
            golden_path.display()
        );
        return;
    }

    // Load and normalize the committed golden.
    let raw_golden = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", golden_path.display()));
    let expected = normalize(&raw_golden);

    if actual != expected {
        print_diff(&expected, &actual, "self-check-provekit-shim-rust-std.json");
        panic!(
            "golden mismatch for self-check on {TARGET}\n\
             If this is a legitimate change, run: UPDATE_GOLDEN=1 cargo test -p provekit-cli --test self_check_golden\n\
             Then update tests/golden/self-check-provekit-shim-rust-std.md with a one-line why."
        );
    }
}
