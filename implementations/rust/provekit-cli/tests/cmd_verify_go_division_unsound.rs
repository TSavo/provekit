// SPDX-License-Identifier: Apache-2.0
//
// CARDINAL-SIN REGRESSION (PR #1445 review blocker): the verify-facing Go
// lifter must NEVER sign a witness for a Go-false statement via an op whose
// SMT-LIB semantics diverge from Go.
//
// The proven bug: `NormalizeCoreArith` mapped `go:div`/`go:mod` to SMT-LIB
// `div`/`mod`, but they DIVERGE on negatives:
//   Go  `-7 / 2 == -3` (truncate toward zero)
//   SMT `(div -7 2) == -4` (floor toward -inf)
// so on `func Halve(x int) int { return x / 2 }`, the assertion
// `Halve(-7) == -4` (FALSE in Go) discharged with a SIGNED WITNESS, exit 0 --
// an inverted proof.
//
// The fix leaves division/modulo UNINTERPRETED (`go:div` stays namespaced), so
// the obligation retains an opaque symbol and verify returns Undecidable
// (EXIT_SOLVER_FAIL = 3) with NO witness -- the honest "I cannot prove this".
//
// This test drives the REAL Go lifter + REAL `provekit mint`/`verify` on the
// reviewer's exact probe and asserts: NO discharge, NO signed witness, exit 3.
// Both the Go-false (`-4`) and Go-true (`-3`) assertions become Undecidable
// with div uninterpreted -- that is correct; the cardinal point is that
// neither false-discharges.
//
// Requires `go` and `z3` on PATH; guards skip loudly otherwise.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

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

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-go-divunsound-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn build_go_lift_verify() -> PathBuf {
    let go_module = repo_root().join("implementations").join("go");
    let out = std::env::temp_dir().join(format!(
        "provekit-lift-go-verify-div-{}",
        std::process::id()
    ));
    let built = Command::new("go")
        .current_dir(&go_module)
        .args([
            "build",
            "-o",
            out.to_str().unwrap(),
            "./cmd/provekit-lift-go-verify",
        ])
        .output()
        .expect("spawn go build");
    assert!(
        built.status.success(),
        "go build provekit-lift-go-verify failed\n  stderr: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    out
}

/// Stage a `Halve(x) = x / 2` project whose harvested assertion is
/// `Halve(-7) == expected`. `expected = -4` is the Go-FALSE/SMT-floor case;
/// `-3` is the Go-TRUE case. Both must be Undecidable with div uninterpreted.
fn stage_halve_project(suffix: &str, lift_bin: &Path, expected: i64) -> PathBuf {
    let example = repo_root().join("examples").join("go-double");
    let project = unique_dir(suffix);

    fs::write(
        project.join("halve.go"),
        "package sample\n\nfunc Halve(x int) int {\n\treturn x / 2\n}\n",
    )
    .expect("write halve.go");
    fs::write(
        project.join("halve_test.go"),
        format!(
            "package sample\n\nimport (\n\t\"testing\"\n\n\t\"github.com/stretchr/testify/assert\"\n)\n\nfunc TestHalve(t *testing.T) {{\n\tassert.Equal(t, Halve(-7), {expected})\n}}\n"
        ),
    )
    .expect("write halve_test.go");
    // Reuse the example's go.mod (testify dep; parser-only, never compiled).
    fs::copy(example.join("go.mod"), project.join("go.mod")).expect("copy go.mod");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("go")).expect("mkdir lift/go");
    fs::copy(
        example.join(".provekit").join("config.toml"),
        provekit.join("config.toml"),
    )
    .expect("copy config.toml");
    let manifest = format!(
        "name = \"go\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        lift_bin.display()
    );
    fs::write(
        provekit.join("lift").join("go").join("manifest.toml"),
        manifest,
    )
    .expect("write manifest");
    project
}

fn run_mint(project: &Path) {
    let out = Command::new(provekit_bin())
        .args(["mint", "--project"])
        .arg(project)
        .arg("--out")
        .arg(project)
        .args(["--no-attest", "--quiet"])
        .output()
        .expect("spawn mint");
    assert!(
        out.status.success(),
        "mint must succeed\n  stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_verify(project: &Path, witness_dir: &Path) -> (Json, i32) {
    let out = Command::new(provekit_bin())
        .args(["verify", "--project"])
        .arg(project)
        .arg("--emit-witnesses")
        .arg(witness_dir)
        .arg("--json")
        .output()
        .expect("spawn verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify JSON parse failed: {e}\nstdout: {stdout}"));
    (receipt, out.status.code().unwrap_or(-1))
}

/// Assert that the `Halve(-7) == expected` claim does NOT discharge, writes NO
/// witness, and exits 3 (Undecidable) -- never a false discharge.
fn assert_undecidable_no_witness(suffix: &str, expected: i64) {
    let lift_bin = build_go_lift_verify();
    let project = stage_halve_project(suffix, &lift_bin, expected);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], false,
        "an integer-division claim must NEVER discharge (div is uninterpreted); claim: {claim}"
    );
    assert_ne!(
        claim["status"], "discharged",
        "CARDINAL SIN: a division claim discharged -- inverted proof regression; claim: {claim}"
    );
    assert_eq!(
        claim["status"], "undecidable",
        "division claim must be Undecidable; claim: {claim}"
    );
    assert!(
        claim["witnessCid"].is_null(),
        "NO signed witness may exist for a division claim; claim: {claim}"
    );

    // No witness file may have been written for any /-containing contract.
    let witness_files: Vec<_> = fs::read_dir(&witnesses)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        witness_files.is_empty(),
        "witness dir must be empty for a division claim; found {} files",
        witness_files.len()
    );

    assert_eq!(
        code, 3,
        "division claim must exit 3 (EXIT_SOLVER_FAIL / undecidable), not 0 (discharged); got {code}"
    );
    eprintln!(
        "GO_DIV_UNSOUND_GUARD expected={expected} status=undecidable exit={code} witnesses=0"
    );

    let _ = fs::remove_dir_all(&project);
}

/// The Go-FALSE assertion `Halve(-7) == -4` (true only under SMT floor div)
/// must NOT discharge and must write NO witness. This is the exact inverted
/// proof PR #1445 review caught.
#[test]
fn go_division_go_false_assertion_does_not_false_discharge() {
    if !go_available() || !z3_available() {
        eprintln!("go/z3 not on PATH: skipping go division unsoundness regression");
        return;
    }
    assert_undecidable_no_witness("false-neg4", -4);
}

/// The Go-TRUE assertion `Halve(-7) == -3` is ALSO Undecidable with div
/// uninterpreted -- correct: we refuse rather than risk an unfaithful model.
#[test]
fn go_division_go_true_assertion_is_undecidable_not_discharged() {
    if !go_available() || !z3_available() {
        eprintln!("go/z3 not on PATH: skipping");
        return;
    }
    assert_undecidable_no_witness("true-neg3", -3);
}
