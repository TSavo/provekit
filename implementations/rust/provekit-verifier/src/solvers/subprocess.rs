// SPDX-License-Identifier: Apache-2.0
//
// Subprocess solver. Generic SMT-LIB v2.6 driver: pipe the script to
// `<binary> [flags...]` on stdin, read the first non-empty stdout line,
// map it to ObligationVerdict.
//
// Replaces the old `solve_obligation::run` Z3-only path. The legacy
// path is preserved as `solve_obligation::run_legacy` for back-compat.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::solvers::{SolveResult, Solver};
use crate::types::ObligationVerdict;

#[derive(Debug, Clone)]
pub struct SubprocessSolver {
    name: String,
    version: String,
    ir_compiler: String,
    binary: String,
    flags: Vec<String>,
    timeout: Option<Duration>,
}

impl SubprocessSolver {
    pub fn new(
        name: impl Into<String>,
        binary: impl Into<String>,
        version: impl Into<String>,
        ir_compiler: impl Into<String>,
        flags: Vec<String>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ir_compiler: ir_compiler.into(),
            binary: binary.into(),
            flags,
            timeout,
        }
    }
}

impl Solver for SubprocessSolver {
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn ir_compiler(&self) -> &str {
        &self.ir_compiler
    }
    fn solve(&self, smt: &str) -> SolveResult {
        let started = Instant::now();
        let mut cmd = Command::new(&self.binary);
        for f in &self.flags {
            cmd.arg(f);
        }
        // Feed via stdin, read stdout.
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return SolveResult {
                    verdict: ObligationVerdict::Undecidable,
                    solver_name: self.name.clone(),
                    solver_version: self.version.clone(),
                    error: format!("spawn {}: {e}", self.binary),
                    solver_stdout: String::new(),
                    wall_clock: started.elapsed(),
                    timed_out: false,
                };
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(smt.as_bytes()) {
                return SolveResult {
                    verdict: ObligationVerdict::Undecidable,
                    solver_name: self.name.clone(),
                    solver_version: self.version.clone(),
                    error: format!("write to {} stdin: {e}", self.binary),
                    solver_stdout: String::new(),
                    wall_clock: started.elapsed(),
                    timed_out: false,
                };
            }
        }

        // Soft timeout: poll wait_timeout-style. We avoid pulling in
        // `wait-timeout` to keep the dep set tight; busy-wait with a
        // small sleep is good enough for the verifier's call-site
        // cardinality (10s-100s, not millions).
        let timed_out;
        if let Some(to) = self.timeout {
            let deadline = started + to;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        timed_out = false;
                        break;
                    }
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            return SolveResult {
                                verdict: ObligationVerdict::Undecidable,
                                solver_name: self.name.clone(),
                                solver_version: self.version.clone(),
                                error: format!(
                                    "timeout after {}s",
                                    to.as_secs().max(1)
                                ),
                                solver_stdout: String::new(),
                                wall_clock: started.elapsed(),
                                timed_out: true,
                            };
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(e) => {
                        return SolveResult {
                            verdict: ObligationVerdict::Undecidable,
                            solver_name: self.name.clone(),
                            solver_version: self.version.clone(),
                            error: format!("wait {}: {e}", self.binary),
                            solver_stdout: String::new(),
                            wall_clock: started.elapsed(),
                            timed_out: false,
                        };
                    }
                }
            }
        } else {
            timed_out = false;
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                return SolveResult {
                    verdict: ObligationVerdict::Undecidable,
                    solver_name: self.name.clone(),
                    solver_version: self.version.clone(),
                    error: format!("wait {}: {e}", self.binary),
                    solver_stdout: String::new(),
                    wall_clock: started.elapsed(),
                    timed_out,
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
            solver_name: self.name.clone(),
            solver_version: self.version.clone(),
            error,
            solver_stdout: stdout,
            wall_clock: started.elapsed(),
            timed_out,
        }
    }
}
