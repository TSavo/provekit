// SPDX-License-Identifier: Apache-2.0
//
// Rust project implication sweep. These fixtures already mint callable
// Rust contracts; their project configs must include the consumer
// implication surface so prove is not vacuous.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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

fn rust_workspace_root() -> PathBuf {
    repo_root().join("implementations").join("rust")
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_rust_lifter_bins() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let workspace = rust_workspace_root();
        for (package, bin) in [
            ("provekit-walk", "provekit-walk-rpc"),
            ("provekit-lift", "provekit-lift"),
        ] {
            let output = Command::new("cargo")
                .current_dir(&workspace)
                .args(["build", "-p", package, "--bin", bin])
                .output()
                .unwrap_or_else(|e| panic!("spawn cargo build -p {package} --bin {bin}: {e}"));
            assert!(
                output.status.success(),
                "cargo build -p {package} --bin {bin} failed\n  stdout: {}\n  stderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let bin_dir = provekit_bin()
            .parent()
            .expect("provekit bin parent")
            .to_path_buf();
        let walk_rpc = bin_dir.join("provekit-walk-rpc");
        let lift = bin_dir.join("provekit-lift");
        assert!(
            walk_rpc.exists(),
            "cargo build produced no {}",
            walk_rpc.display()
        );
        assert!(lift.exists(), "cargo build produced no {}", lift.display());
    });
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-rust-project-imp-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn run_mint(project: &Path, out_dir: &Path) {
    let output = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(out_dir)
        .arg("--no-attest")
        .arg("--quiet")
        .arg("--json")
        .output()
        .expect("spawn provekit mint");
    assert!(
        output.status.success(),
        "provekit mint failed for {}\nstdout:\n{}\nstderr:\n{}",
        project.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_prove_json(out_dir: &Path) -> (Json, i32) {
    let output = Command::new(provekit_bin())
        .arg("prove")
        .arg(out_dir)
        .arg("--json")
        .output()
        .expect("spawn provekit prove");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Json = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("prove JSON parse failed: {e}\nstdout: {stdout}"));
    (report, output.status.code().unwrap_or(-1))
}

#[test]
fn configured_rust_shims_emit_nonvacuous_implication_claims() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Rust project implication sweep");
        return;
    }
    build_rust_lifter_bins();

    let repo = repo_root();
    for project_rel in [
        "examples/provekit-shim-blake3-rust",
        "examples/provekit-shim-postgres",
        "examples/provekit-shim-rfc8785-jcs-rust",
        "examples/provekit-shim-rusqlite",
        "examples/provekit-shim-serde-json-rust",
    ] {
        let project = repo.join(project_rel);
        let out_dir = unique_dir(project_rel.rsplit('/').next().unwrap_or("project"));
        run_mint(&project, &out_dir);
        let (report, code) = run_prove_json(&out_dir);
        assert_eq!(
            code, 0,
            "{} should prove cleanly once implication bridges are minted; report: {report}",
            project_rel
        );
        let total = report["totalCallsites"].as_u64().unwrap_or(0);
        assert!(
            total > 0,
            "{} must not prove vacuously; report: {report}",
            project_rel
        );
        assert_eq!(report["violations"], 0, "{} report: {report}", project_rel);
        assert_eq!(
            report["discharged"], report["totalCallsites"],
            "{} report: {report}",
            project_rel
        );
        let _ = fs::remove_dir_all(out_dir);
    }
}
