// SPDX-License-Identifier: Apache-2.0
//
// Build the runtime solver registry from a parsed SolversConfig.
// Maps each `[solvers.<name>]` block to one of:
//
//   * StubSolver           - when `binary = "stub:..."`.
//   * CoqSubprocessSolver  - when `ir_compiler = "coq"` (or, for
//                            forward compat, the `provekit-ir-compiler-coq`
//                            DIALECT constant). Coq is a non-SMT solver:
//                            it reads IR-JSON, compiles to Coq via
//                            `CoqCompiler`, and runs `coqc` on the
//                            generated `.v` file.
//   * SubprocessSolver     - default. Generic SMT-LIB v2.6 driver
//                            (Z3, cvc5, bitwuzla, MathSAT, ...).
//
// The Coq branch makes Coq a real producer in the multi-solver
// portfolio rather than a compile-time-only translator. Three-way
// consensus (z3 + cvc5 + coq) is now a registry-resolvable plan.
//
// Spec: protocol/specs/2026-05-02-multi-solver-protocol-v2.md
//       (Coq's seat in `Portfolio { consensus, coverage_required: true }`).
// Note: this file ships the registry seat. The §5 ConsensusCoverage
// 7-step rule and the OpacityManifest types are out of scope for this
// change; see the PR body for the staged-rollout plan.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use provekit_ir_compiler_coq::DIALECT as COQ_DIALECT;

use crate::solvers::{
    CoqSubprocessSolver, SolverConfig, SolverHandle, SolversConfig, StubSolver, SubprocessSolver,
};

pub fn build(cfg: &SolversConfig) -> HashMap<String, SolverHandle> {
    let mut out: HashMap<String, SolverHandle> = HashMap::new();
    for (name, sc) in &cfg.solvers {
        out.insert(name.clone(), build_one(name, sc));
    }
    out
}

/// Returns true if the configured `ir_compiler` selects the Coq
/// pipeline. Accepts the canonical `coq` value alongside the
/// dialect constant exported by `provekit-ir-compiler-coq` so that
/// renaming the dialect at the compiler crate is the only edit
/// required.
fn is_coq_compiler(ir_compiler: &str) -> bool {
    ir_compiler == "coq" || ir_compiler == COQ_DIALECT
}

fn build_one(name: &str, sc: &SolverConfig) -> SolverHandle {
    if let Some(stub) = StubSolver::from_binary(name, &sc.binary) {
        return Arc::new(stub) as SolverHandle;
    }
    let timeout = sc.timeout_seconds.map(Duration::from_secs);
    let bin = if sc.binary.is_empty() {
        name.to_string()
    } else {
        sc.binary.clone()
    };
    if is_coq_compiler(&sc.ir_compiler) {
        return Arc::new(CoqSubprocessSolver::new(
            name,
            bin,
            sc.version.clone(),
            timeout,
        )) as SolverHandle;
    }
    Arc::new(SubprocessSolver::new(
        name,
        bin,
        sc.version.clone(),
        sc.ir_compiler.clone(),
        sc.flags.clone(),
        timeout,
    )) as SolverHandle
}

/// Convenience: build a registry with a single Z3 SubprocessSolver
/// at the given binary path. Used by the legacy `RunnerConfig.z3_path`
/// fallback when no `.provekit/config.toml` is present.
///
/// A 30-second per-invocation timeout is applied as defense-in-depth.
/// Without a timeout, a Z3 invocation that reads from stdin without
/// receiving EOF (e.g. when inherited in a subprocess chain) can block
/// indefinitely.  30 s is a conservative upper bound for any SMT-LIB
/// obligation that would arise from a ProvekIt proof graph.
pub fn build_default_z3(z3_path: &str) -> HashMap<String, SolverHandle> {
    let mut out: HashMap<String, SolverHandle> = HashMap::new();
    out.insert(
        "z3".into(),
        Arc::new(SubprocessSolver::new(
            "z3",
            z3_path,
            "4.x",
            "smt-lib-v2.6",
            vec!["-smt2".into(), "-in".into()],
            Some(Duration::from_secs(30)),
        )) as SolverHandle,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_from_stub_config() {
        let toml = r#"
[solvers]
default = "stubA"
[solvers.stubA]
binary = "stub:unsat"
"#;
        let c = SolversConfig::from_toml(toml).unwrap();
        let r = build(&c);
        assert!(r.contains_key("stubA"));
        let s = r.get("stubA").unwrap();
        let res = s.solve("");
        assert_eq!(res.verdict, crate::types::ObligationVerdict::Discharged);
    }

    #[test]
    fn default_z3_built() {
        let r = build_default_z3("/usr/bin/z3");
        assert!(r.contains_key("z3"));
        assert_eq!(r.get("z3").unwrap().name(), "z3");
    }

    #[test]
    fn build_recognizes_coq_ir_compiler() {
        // When `ir_compiler = "coq"` the registry must instantiate
        // CoqSubprocessSolver, NOT a generic SMT-LIB SubprocessSolver
        // pointed at coqc (which would silently fail because Coq does
        // not speak SMT-LIB).
        //
        // The empirical assertion is by ir_compiler tag: the Coq
        // solver reports DIALECT as its ir_compiler tag, while the
        // generic subprocess solver reports the configured one
        // (which here would be "coq" round-tripped). They happen to
        // be equal strings, so we additionally probe behavior: the
        // Coq solver returns Undecidable with a parse error when
        // handed SMT-LIB instead of IR-JSON; the SMT subprocess
        // solver returns a binary-not-found error. We assert the
        // Coq error path.
        let toml = r#"
[solvers]
default = "coq"
[solvers.coq]
binary = "coqc"
ir_compiler = "coq"
"#;
        let c = SolversConfig::from_toml(toml).unwrap();
        let r = build(&c);
        let s = r.get("coq").expect("coq registered");
        assert_eq!(s.ir_compiler(), "coq");
        // Hand the solver SMT-LIB. CoqSubprocessSolver expects
        // IR-JSON, so this MUST return Undecidable with a JSON parse
        // error, never a "binary not found" error from the SMT
        // subprocess driver. This is the load-bearing assertion: if
        // the registry built a SubprocessSolver instead of a
        // CoqSubprocessSolver, the error message would mention spawn
        // failure on `coqc`, not JSON parsing.
        let res = s.solve("(check-sat)");
        assert_eq!(res.verdict, crate::types::ObligationVerdict::Undecidable);
        assert!(
            res.error.contains("parse IR-JSON") || res.error.contains("IR-JSON"),
            "expected IR-JSON parse error from CoqSubprocessSolver, got: {}",
            res.error
        );
    }

    #[test]
    fn build_keeps_smt_solvers_for_non_coq_ir_compiler() {
        // Sanity check: an `ir_compiler = "smt-lib-v2.6"` solver is
        // still built as a SubprocessSolver. This confirms the Coq
        // branch is selective rather than capturing every entry.
        let toml = r#"
[solvers]
default = "z3"
[solvers.z3]
binary = "z3"
ir_compiler = "smt-lib-v2.6"
"#;
        let c = SolversConfig::from_toml(toml).unwrap();
        let r = build(&c);
        let s = r.get("z3").expect("z3 registered");
        assert_eq!(s.ir_compiler(), "smt-lib-v2.6");
    }
}
