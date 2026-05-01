// SPDX-License-Identifier: Apache-2.0
//
// Build the runtime solver registry from a parsed SolversConfig.
// Maps each `[solvers.<name>]` block to either a SubprocessSolver or
// (for `binary = "stub:..."`) a StubSolver.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::solvers::{SolverConfig, SolverHandle, SolversConfig, StubSolver, SubprocessSolver};

pub fn build(cfg: &SolversConfig) -> HashMap<String, SolverHandle> {
    let mut out: HashMap<String, SolverHandle> = HashMap::new();
    for (name, sc) in &cfg.solvers {
        out.insert(name.clone(), build_one(name, sc));
    }
    out
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
            None,
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
}
