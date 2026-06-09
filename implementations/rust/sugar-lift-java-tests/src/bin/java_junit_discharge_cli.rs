// SPDX-License-Identifier: Apache-2.0
//
// JUnit witness discharge command. Re-runs the suite and refuses unless the
// pinned bundle reproduces and every test passed.

use std::path::Path;

use sugar_lift_java_tests as kit;

fn emit(verdict: &str, reason: &str) -> i32 {
    let line = serde_json::json!({"verdict": verdict, "reason": reason});
    println!("{line}");
    if verdict == "DISCHARGED" {
        0
    } else {
        1
    }
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.len() < 2 {
        return emit("REFUSED", "usage: <witness.proof> <project_dir>");
    }
    let proof_path = &argv[0];
    let project_dir = &argv[1];

    let evidence_json = match std::fs::read_to_string(proof_path) {
        Ok(s) => s,
        Err(e) => return emit("REFUSED", &format!("discharge error: read proof: {e}")),
    };
    let pd = match kit::parse_evidence_proof_data(&evidence_json) {
        Ok(v) => v,
        Err(e) => return emit("REFUSED", &format!("discharge error: {e}")),
    };
    if pd.get("kind").and_then(|v| v.as_str()) != Some("witness-package") {
        return emit(
            "REFUSED",
            "discharge error: proofData is not a witness-package",
        );
    }
    let Some(package_cid) = pd.get("packageCid").and_then(|v| v.as_str()) else {
        return emit("REFUSED", "discharge error: proofData missing packageCid");
    };
    let code_files = kit::memento_str_list(&pd, "codeFiles");
    let expected_count = pd.get("count").and_then(|v| v.as_u64()).map(|v| v as usize);
    let expected_passed = pd
        .get("passed")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let (verdict, reason) = kit::discharge_bundle_or_package(
        package_cid,
        &code_files,
        Path::new(project_dir),
        expected_count,
        expected_passed,
    );
    emit(&verdict, &reason)
}

fn main() {
    std::process::exit(run());
}
