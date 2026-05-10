// SPDX-License-Identifier: Apache-2.0

use provekit_verifier::solvers::{
    plan::run_plan, registry, LeanSubprocessSolver, Solver, SolverPlan, SolversConfig,
};
use provekit_verifier::types::ObligationVerdict;

#[test]
fn lean_file_cid_uses_provekit_canonicalizer_hash() {
    let source = "theorem provekit_obligation : True := by trivial\n";
    assert_eq!(
        LeanSubprocessSolver::lean_file_cid(source),
        provekit_canonicalizer::blake3_512_of(source.as_bytes())
    );
}

#[test]
fn axiom_parser_detects_sorry_ax() {
    let output = "axioms provekit_obligation: [propext, Quot.sound, sorryAx]\n";
    let axioms = LeanSubprocessSolver::parse_axiom_set(output, "provekit_obligation");
    assert!(axioms.iter().any(|a| a == "sorryAx"));
    assert!(LeanSubprocessSolver::uses_sorry_or_sorry_ax(
        "theorem provekit_obligation : True := by trivial\n",
        output
    ));
}

#[test]
fn registry_recognizes_lean_ir_compiler() {
    let cfg = SolversConfig::from_toml(
        r#"
[solvers]
default = "lean"

[solvers.lean]
binary = "lake"
ir_compiler = "lean"
"#,
    )
    .expect("parse");
    let plan = SolverPlan::from_config(&cfg);
    let registry = registry::build(&cfg);
    let solver = registry.get("lean").expect("lean registered");
    assert_eq!(solver.ir_compiler(), "lean");
    match plan {
        SolverPlan::Single(name) => assert_eq!(name, "lean"),
        _ => panic!("expected single lean solver"),
    }
    let result = solver.solve("(check-sat)");
    assert_eq!(result.verdict, ObligationVerdict::Undecidable);
    assert!(result.error.contains("IR-JSON"));
}

#[test]
fn run_plan_feeds_ir_json_to_lean_solver() {
    let cfg = SolversConfig::from_toml(
        r#"
[solvers]
default = "lean"

[solvers.lean]
binary = "/definitely/missing/lake"
ir_compiler = "lean"
"#,
    )
    .expect("parse");
    let plan = SolverPlan::from_config(&cfg);
    let registry = registry::build(&cfg);
    let formula = serde_json::json!({"kind": "atomic", "name": "true", "args": []});
    let (verdict, _reason, invocations) = run_plan(&plan, &registry, "(check-sat)", Some(&formula));
    assert_eq!(verdict, ObligationVerdict::Undecidable);
    let error = &invocations[0].result.error;
    assert!(
        error.contains("spawn") && !error.contains("parse IR-JSON"),
        "Lean should receive formula JSON before spawning lake, got: {error}"
    );
}

#[test]
fn mathlib_commit_parser_reads_lake_manifest() {
    let dir = std::env::temp_dir().join(format!(
        "provekit-lean-manifest-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("lake-manifest.json"),
        r#"{"packages":[{"name":"mathlib","rev":"abc123"}]}"#,
    )
    .expect("write manifest");
    let commit = LeanSubprocessSolver::mathlib_commit_from_project(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(commit.as_deref(), Some("abc123"));
}

#[test]
#[ignore = "requires lake, lean, and a local mathlib lake project"]
fn lean_solver_discharges_reflexivity_with_local_mathlib() {
    let project = std::env::var("PROVEKIT_LEAN_PROJECT")
        .expect("set PROVEKIT_LEAN_PROJECT to a mathlib lake project");
    let solver = LeanSubprocessSolver::new(
        "lean",
        "lake",
        "4.x",
        Some(std::time::Duration::from_secs(60)),
        Some(project),
        None,
    );
    let ir = serde_json::json!({
        "kind": "forall",
        "name": "x",
        "sort": {"kind": "primitive", "name": "Int"},
        "body": {
            "kind": "atomic",
            "name": "=",
            "args": [
                {"kind": "var", "name": "x"},
                {"kind": "var", "name": "x"}
            ]
        }
    });
    let result = solver.solve(&ir.to_string());
    assert_eq!(result.verdict, ObligationVerdict::Discharged);
    assert!(!result.solver_stdout.contains("sorryAx"));
}
