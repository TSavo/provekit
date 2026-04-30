// SPDX-License-Identifier: Apache-2.0
//
// Verifier runner — composes the six stages and fans out stages 3-5
// per callsite using rayon (mirrors the C++ std::async fan-out and
// the Go goroutine fan-out).

use std::path::PathBuf;

use rayon::prelude::*;

use crate::types::{CallSite, MementoPool, ObligationVerdict, Report};
use crate::{
    enumerate_callsites, instantiate, load_all_proofs, report as report_stage, resolve_target,
    smt_emitter, solve_obligation,
};

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub project_root: PathBuf,
    pub z3_path: String,
}

pub struct Runner {
    cfg: RunnerConfig,
}

impl Runner {
    pub fn new(cfg: RunnerConfig) -> Self {
        Self { cfg }
    }

    pub fn run(&self) -> Report {
        let mut report = Report::default();

        // Stage 1.
        let pool = load_all_proofs::run(&self.cfg.project_root);

        // Stage 2.
        let callsites = enumerate_callsites::run(&pool);

        // Stages 3-5 fan out per callsite.
        let z3 = self.cfg.z3_path.clone();
        let per_results: Vec<(CallSite, ObligationVerdict, String)> = callsites
            .par_iter()
            .map(|cs| work_one(cs, &pool, &z3))
            .collect();

        // Stage 6 (report aggregation).
        for (cs, verdict, reason) in per_results {
            report_stage::add_callsite(&cs, verdict, &reason, &mut report);
        }
        report_stage::add_load_errors(&pool.load_errors, &mut report);
        report
    }

    /// Loads the pool but stops short of solving — useful for the
    /// Rust round-trip example (asserts callsite resolution works).
    pub fn run_load_and_enumerate(&self) -> (MementoPool, Vec<CallSite>) {
        let pool = load_all_proofs::run(&self.cfg.project_root);
        let cs = enumerate_callsites::run(&pool);
        (pool, cs)
    }
}

fn work_one(
    cs: &CallSite,
    pool: &MementoPool,
    z3_path: &str,
) -> (CallSite, ObligationVerdict, String) {
    let resolved = match resolve_target::run(cs, pool) {
        Ok(r) => r,
        Err(e) => {
            return (
                cs.clone(),
                ObligationVerdict::Undecidable,
                format!("resolve-target: {e}"),
            );
        }
    };
    let ob = match instantiate::run(&resolved, &cs.arg_term) {
        Ok(o) => o,
        Err(e) => {
            return (
                cs.clone(),
                ObligationVerdict::Undecidable,
                format!("instantiate: {e}"),
            );
        }
    };
    let smt = match smt_emitter::emit(&ob.ir_formula) {
        Ok(s) => s,
        Err(e) => {
            return (
                cs.clone(),
                ObligationVerdict::Undecidable,
                format!("smt-emit: {e}"),
            );
        }
    };
    let res = solve_obligation::run(z3_path, &smt);
    let reason = if !res.error.is_empty() {
        res.error
    } else {
        match res.verdict {
            ObligationVerdict::Discharged => "solver returned unsat: obligation holds".into(),
            ObligationVerdict::Unsatisfied => {
                "solver returned sat (counterexample found): obligation falsifiable".into()
            }
            _ => String::new(),
        }
    };
    (cs.clone(), res.verdict, reason)
}
