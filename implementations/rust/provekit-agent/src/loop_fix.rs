// SPDX-License-Identifier: Apache-2.0
//
// `provekit fix <bug>` loop. The agent produces patches; we apply
// them to a sandboxed copy of the working tree, run the build /
// verifier as oracle, and either ship or feed failures back.
//
// This crate stays IO-light: the verification step is supplied as a
// closure (`Verifier`), so callers (the CLI) plug in `cargo build`
// or `pnpm test` or whatever the project uses. Tests inject a fake
// verifier that returns canned outcomes.

use std::path::PathBuf;

use crate::{
    mint_validated, validate_candidate, AgentError, FilePatch, FixContext, FixResult, MintOptions,
    MintedAgentContract, ProvekitAgent, ValidationOutcome,
};

#[derive(Debug, Clone)]
pub struct FixLoopOutcome {
    pub patches: Vec<FilePatch>,
    pub minted_contracts: Vec<MintedAgentContract>,
    pub commentary: String,
    pub agent_calls: usize,
    /// True if the verifier returned green for the final patch set.
    pub verified: bool,
    /// Last failure description, if any.
    pub last_failure: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FixLoopOptions {
    pub max_retries: u32,
    pub mint_options: MintOptions,
    /// Sandbox dir to apply patches into. The CLI is responsible for
    /// preparing it (typically a `git worktree` or copy of the repo).
    pub sandbox: Option<PathBuf>,
}

impl Default for FixLoopOptions {
    fn default() -> Self {
        Self {
            max_retries: 3,
            mint_options: MintOptions::default(),
            sandbox: None,
        }
    }
}

/// Result of running the verifier over a candidate patch set.
#[derive(Debug, Clone)]
pub enum VerifyOutcome {
    /// Build + verifier green; safe to ship.
    Green,
    /// Verifier rejected with this human-readable reason. The reason
    /// is fed back to the agent for refinement.
    Failed(String),
}

pub trait Verifier: Send + Sync {
    fn verify(&self, patches: &[FilePatch]) -> VerifyOutcome;
}

pub fn run_fix_loop<A: ProvekitAgent + ?Sized, V: Verifier + ?Sized>(
    agent: &A,
    initial_ctx: FixContext,
    verifier: &V,
    opts: &FixLoopOptions,
) -> Result<FixLoopOutcome, AgentError> {
    let mut ctx = initial_ctx;
    let mut calls = 0usize;
    let mut last_failure: Option<String> = None;

    let max_total = (opts.max_retries as usize) + 1;

    for _ in 0..max_total {
        calls += 1;
        let result: FixResult = agent.fix_bug(&ctx)?;
        match verifier.verify(&result.patches) {
            VerifyOutcome::Green => {
                // Try to mint each new contract; soft-fail individual mints.
                let mut minted = Vec::new();
                for c in &result.new_contracts {
                    if let ValidationOutcome::Accepted(v) = validate_candidate(c) {
                        if let Ok(m) = mint_validated(&v, &opts.mint_options) {
                            minted.push(m);
                        }
                    }
                }
                return Ok(FixLoopOutcome {
                    patches: result.patches,
                    minted_contracts: minted,
                    commentary: result.commentary,
                    agent_calls: calls,
                    verified: true,
                    last_failure: None,
                });
            }
            VerifyOutcome::Failed(reason) => {
                last_failure = Some(reason.clone());
                ctx.previous_rejection = Some(reason);
            }
        }
    }

    Ok(FixLoopOutcome {
        patches: vec![],
        minted_contracts: vec![],
        commentary: "verifier never went green".into(),
        agent_calls: calls,
        verified: false,
        last_failure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubAgent;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct AlwaysGreen;
    impl Verifier for AlwaysGreen {
        fn verify(&self, _: &[FilePatch]) -> VerifyOutcome {
            VerifyOutcome::Green
        }
    }

    struct AlwaysRed;
    impl Verifier for AlwaysRed {
        fn verify(&self, _: &[FilePatch]) -> VerifyOutcome {
            VerifyOutcome::Failed("simulated build failure".into())
        }
    }

    struct GreenAfterN {
        count: AtomicU32,
        threshold: u32,
    }
    impl Verifier for GreenAfterN {
        fn verify(&self, _: &[FilePatch]) -> VerifyOutcome {
            let n = self.count.fetch_add(1, Ordering::SeqCst);
            if n >= self.threshold {
                VerifyOutcome::Green
            } else {
                VerifyOutcome::Failed(format!("attempt {}: not yet", n + 1))
            }
        }
    }

    #[test]
    fn fix_loop_returns_green_on_first_pass() {
        let agent = StubAgent::new();
        let ctx = FixContext {
            repo_root: PathBuf::from("."),
            bug_description: "rejects negative line numbers".into(),
            violated_contracts: vec![],
            allowed_paths: vec![],
            previous_rejection: None,
        };
        let outcome =
            run_fix_loop(&agent, ctx, &AlwaysGreen, &FixLoopOptions::default()).expect("loop");
        assert!(outcome.verified);
        assert_eq!(outcome.agent_calls, 1);
    }

    #[test]
    fn fix_loop_exhausts_retries_when_red() {
        let agent = StubAgent::new();
        let ctx = FixContext {
            repo_root: PathBuf::from("."),
            bug_description: "won't ever pass".into(),
            violated_contracts: vec![],
            allowed_paths: vec![],
            previous_rejection: None,
        };
        let opts = FixLoopOptions {
            max_retries: 2,
            ..Default::default()
        };
        let outcome = run_fix_loop(&agent, ctx, &AlwaysRed, &opts).expect("loop");
        assert!(!outcome.verified);
        assert_eq!(outcome.agent_calls, 3); // initial + 2 retries
        assert!(outcome.last_failure.is_some());
    }

    #[test]
    fn fix_loop_recovers_after_one_failure() {
        let agent = StubAgent::new();
        let ctx = FixContext {
            repo_root: PathBuf::from("."),
            bug_description: "needs one retry".into(),
            violated_contracts: vec![],
            allowed_paths: vec![],
            previous_rejection: None,
        };
        let v = GreenAfterN {
            count: AtomicU32::new(0),
            threshold: 1,
        };
        let outcome = run_fix_loop(&agent, ctx, &v, &FixLoopOptions::default()).expect("loop");
        assert!(outcome.verified);
        assert_eq!(outcome.agent_calls, 2);
    }
}
