// SPDX-License-Identifier: Apache-2.0
//
// Rust Product A, end to end: behavioral contract inheritance, ported from the
// numpy story (test_inheritance_e2e.py). A vendor crate mints a behavioral
// contract about a callsite (`addlib::add(2,3) == 5`); a consumer crate stages
// that `.proof` in `.provekit/imports/` and asserts about the SAME call. The
// consumer that CONTRADICTS the inherited contract is REFUSED (z3 finds the
// `#euf#` conjoin UNSAT); the consumer that AGREES is PROVEN.
//
// This is Product A with ZERO Product B (no implication bridges, no panic
// freedom): the contract is a test assertion lifted to a location-independent
// EUF callsite identity, and inheritance is the cross-proof conjoin. Skips
// cleanly when the toolchain (built lifter bins + z3) is absent.

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

/// Build the debug `provekit-lift` bin the fixture manifests dispatch.
fn build_lift_bin() -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let out = Command::new("cargo")
            .current_dir(rust_workspace_root())
            .args(["build", "-p", "provekit-lift", "--bin", "provekit-lift"])
            .output()
            .expect("spawn cargo build provekit-lift");
        assert!(
            out.status.success(),
            "build provekit-lift failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    rust_workspace_root()
        .join("target")
        .join("debug")
        .join("provekit-lift")
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-rust-inherit-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

const SOLVERS: &str = "[solvers]\ndefault = \"z3\"\n[solvers.dispatch]\n\
linear_arithmetic = \"z3\"\ndefault = \"z3\"\n[solvers.z3]\n\
binary = \"z3\"\nflags = [\"-smt2\", \"-in\"]\n";

/// Write a crate that depends on `addlib` and asserts `addlib::add(2,3) == value`.
fn write_consumer_like(dir: &Path, name: &str, lift_bin: &Path, value: i64) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join(".provekit").join("lift").join("rust-contracts")).unwrap();
    fs::create_dir_all(dir.join(".provekit").join("imports")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             [dependencies]\naddlib = {{ path = \"../addlib\" }}\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join("src").join("lib.rs"),
        format!(
            "#[cfg(test)]\nmod tests {{\n    #[test]\n    \
             fn t() {{ assert_eq!(addlib::add(2, 3), {value}); }}\n}}\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join(".provekit").join("config.toml"),
        format!("[[plugins]]\nname = \"rust-contracts\"\nsurface = \"rust-contracts\"\nemit = \"ir-document\"\n{SOLVERS}"),
    )
    .unwrap();
    fs::write(
        dir.join(".provekit").join("lift").join("rust-contracts").join("manifest.toml"),
        format!(
            "name = \"rust-contracts-lift\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \"{}\"\n",
            lift_bin.display(),
            dir.display()
        ),
    )
    .unwrap();
}

fn run_mint(project: &Path, out_dir: &Path) {
    let out = Command::new(provekit_bin())
        .args(["mint", "--project"])
        .arg(project)
        .arg("--out")
        .arg(out_dir)
        .args(["--no-attest", "--quiet", "--json"])
        .output()
        .expect("spawn mint");
    assert!(
        out.status.success(),
        "mint failed for {}:\n{}",
        project.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn prove_with(project: &Path, with_out: &Path) -> Json {
    let out = Command::new(provekit_bin())
        .arg("prove")
        .arg(project)
        .arg("--with")
        .arg(with_out)
        .arg("--json")
        .output()
        .expect("spawn prove");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("prove json parse: {e}\n{stdout}"))
}

fn one_proof(dir: &Path) -> PathBuf {
    let mut proofs: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension().and_then(|s| s.to_str()) == Some("proof")).then_some(p)
        })
        .collect();
    assert_eq!(proofs.len(), 1, "expected exactly one .proof in {}", dir.display());
    proofs.pop().unwrap()
}

/// Run one consumer parametrization against the inherited vendor contract.
fn run_case(asserted: i64, expect_refused: bool) {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping rust inheritance E2E");
        return;
    }
    let lift = build_lift_bin();
    let ws = unique_dir(&format!("a{asserted}"));

    // shared lib crate (the "numpy"): addlib::add
    fs::create_dir_all(ws.join("addlib").join("src")).unwrap();
    fs::write(
        ws.join("addlib").join("Cargo.toml"),
        "[package]\nname = \"addlib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        ws.join("addlib").join("src").join("lib.rs"),
        "pub fn add(a: i64, b: i64) -> i64 { a + b }\n",
    )
    .unwrap();

    // VENDOR: behavioral contract addlib::add(2,3) == 5
    let vendor = ws.join("vendor");
    write_consumer_like(&vendor, "vendor", &lift, 5);
    let vendor_out = unique_dir(&format!("a{asserted}-vendorout"));
    run_mint(&vendor, &vendor_out);
    let vendor_proof = one_proof(&vendor_out);

    // CONSUMER: asserts addlib::add(2,3) == `asserted`; inherits vendor's proof.
    let consumer = ws.join("consumer");
    write_consumer_like(&consumer, "consumer", &lift, asserted);
    fs::copy(
        &vendor_proof,
        consumer
            .join(".provekit")
            .join("imports")
            .join(vendor_proof.file_name().unwrap()),
    )
    .unwrap();
    let consumer_out = unique_dir(&format!("a{asserted}-consout"));
    run_mint(&consumer, &consumer_out);

    let report = prove_with(&consumer, &consumer_out);
    let violations = report["violations"].as_u64().unwrap_or(u64::MAX);
    let pretty = serde_json::to_string_pretty(&report).unwrap_or_default();
    if expect_refused {
        assert!(
            violations >= 1,
            "consumer ==`{asserted}` must be REFUSED (inherits vendor ==5):\n{pretty}"
        );
        assert!(
            pretty.contains("#euf#") && pretty.contains("contradictory"),
            "refusal must be the #euf# conjoin contradiction:\n{pretty}"
        );
    } else {
        assert_eq!(
            violations, 0,
            "consumer ==`{asserted}` (agrees with vendor) must be PROVEN:\n{pretty}"
        );
    }

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&vendor_out);
    let _ = fs::remove_dir_all(&consumer_out);
}

#[test]
fn consumer_contradicting_inherited_contract_is_refused() {
    run_case(6, true);
}

#[test]
fn consumer_agreeing_with_inherited_contract_is_proven() {
    run_case(5, false);
}
