// SPDX-License-Identifier: Apache-2.0
//
// Stage 6: solve_obligation. Run z3 as a subprocess on the SMT-LIB
// script. Map the first non-empty stdout line:
//   "unsat" -> Discharged
//   "sat"   -> Unsatisfied
//   other   -> Undecidable
//
// Mirrors .../verifier/solve_obligation.cpp.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::types::ObligationVerdict;

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub verdict: ObligationVerdict,
    pub error: String,
    pub solver_stdout: String,
}

pub fn run(z3_path: &str, smt_script: &str) -> SolveResult {
    let mut child = match Command::new(z3_path)
        .arg("-smt2")
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return SolveResult {
                verdict: ObligationVerdict::Undecidable,
                error: format!("spawn z3: {e}"),
                solver_stdout: String::new(),
            };
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(smt_script.as_bytes()) {
            return SolveResult {
                verdict: ObligationVerdict::Undecidable,
                error: format!("write to z3 stdin: {e}"),
                solver_stdout: String::new(),
            };
        }
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            return SolveResult {
                verdict: ObligationVerdict::Undecidable,
                error: format!("wait z3: {e}"),
                solver_stdout: String::new(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let verdict_line = stdout
        .lines()
        .map(|s| s.trim_end_matches('\r'))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_string();
    let (verdict, error) = match verdict_line.as_str() {
        "unsat" => (ObligationVerdict::Discharged, String::new()),
        "sat" => (ObligationVerdict::Unsatisfied, String::new()),
        other => (
            ObligationVerdict::Undecidable,
            format!("unrecognized solver verdict: {other}"),
        ),
    };
    SolveResult {
        verdict,
        error,
        solver_stdout: stdout,
    }
}
