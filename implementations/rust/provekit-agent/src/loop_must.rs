// SPDX-License-Identifier: Apache-2.0
//
// `provekit must <file> "<english>"` loop. Drives translate / validate
// / mint for a single English description. Rejection reasons are fed
// back to the agent for up to N retries.
//
// The CLI is responsible for writing the `.invariant.ts` companion
// file (or `.invariant.rs` etc.); this loop returns the validated
// candidate + the minted memento. Source-file edits the agent
// proposes are returned alongside so the caller can apply them.

use crate::{
    mint_validated, validate_candidate, AgentError, ContractCandidate, MintOptions,
    MintedAgentContract, MustContext, ProvekitAgent, ValidationOutcome,
};

#[derive(Debug, Clone)]
pub struct MustLoopOutcome {
    /// The accepted candidate that produced the mint.
    pub candidate: ContractCandidate,
    pub minted: MintedAgentContract,
    /// Rejected candidates, with reasons (in the order tried).
    pub rejected: Vec<(ContractCandidate, String)>,
    pub agent_calls: usize,
}

#[derive(Debug, Clone)]
pub struct MustLoopOptions {
    pub max_retries: u32,
    pub mint_options: MintOptions,
}

impl Default for MustLoopOptions {
    fn default() -> Self {
        Self {
            max_retries: 3,
            mint_options: MintOptions::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MustLoopError {
    #[error("agent backend error: {0}")]
    Backend(#[from] AgentError),
    #[error("agent could not produce a valid contract after {tries} tries; last reason: {last}")]
    Exhausted { tries: u32, last: String },
}

pub fn run_must_loop<A: ProvekitAgent + ?Sized>(
    agent: &A,
    initial_ctx: MustContext,
    opts: &MustLoopOptions,
) -> Result<MustLoopOutcome, MustLoopError> {
    let mut ctx = initial_ctx;
    let mut rejected: Vec<(ContractCandidate, String)> = Vec::new();
    let mut last_reason = "no candidate produced".to_string();
    let mut calls = 0usize;

    let max_total = (opts.max_retries as usize) + 1; // initial + retries
    for attempt in 0..max_total {
        calls += 1;
        let candidate = agent.translate_must(&ctx)?;
        match validate_candidate(&candidate) {
            ValidationOutcome::Accepted(v) => {
                let minted = mint_validated(&v, &opts.mint_options).map_err(|e| {
                    MustLoopError::Exhausted {
                        tries: (attempt + 1) as u32,
                        last: format!("mint failure: {e}"),
                    }
                })?;
                return Ok(MustLoopOutcome {
                    candidate,
                    minted,
                    rejected,
                    agent_calls: calls,
                });
            }
            ValidationOutcome::Rejected(reason) => {
                last_reason = reason.clone();
                rejected.push((candidate, reason.clone()));
                ctx.previous_rejection = Some(reason);
            }
        }
    }

    Err(MustLoopError::Exhausted {
        tries: max_total as u32,
        last: last_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubAgent;
    use std::path::PathBuf;

    #[test]
    fn must_loop_produces_doubleledger_contract() {
        let agent = StubAgent::new();
        let ctx = MustContext {
            source_path: PathBuf::from("doubleledger.ts"),
            source_text: "// fixture".into(),
            description: "not lose money".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        let out = run_must_loop(&agent, ctx, &MustLoopOptions::default()).expect("loop");
        assert_eq!(out.candidate.name, "doubleledger_conservation");
        assert!(out.minted.cid.starts_with("blake3-512:"));
        assert_eq!(out.rejected.len(), 0);
    }

    #[test]
    fn must_loop_passes_rejection_back_to_agent() {
        use std::sync::atomic::{AtomicU32, Ordering};
        // Use a wrapping agent that returns malformed first, then a
        // valid candidate. The loop should retry.
        struct FlakyAgent {
            calls: AtomicU32,
        }
        impl ProvekitAgent for FlakyAgent {
            fn propose_contracts(
                &self,
                _: &crate::ProposeContext,
            ) -> Result<Vec<ContractCandidate>, AgentError> {
                Ok(vec![])
            }
            fn translate_must(&self, ctx: &MustContext) -> Result<ContractCandidate, AgentError> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    // Malformed: empty contract.
                    Ok(ContractCandidate {
                        name: "bad".into(),
                        pre: None,
                        post: None,
                        inv: None,
                        out_binding: "out".into(),
                        provenance: crate::AgentProvenance {
                            agent_name: "flaky".into(),
                            agent_version: "0".into(),
                            model: None,
                            confidence: None,
                            rationale: None,
                        },
                    })
                } else {
                    // Now with rejection feedback in ctx, we return valid.
                    assert!(ctx.previous_rejection.is_some());
                    let post = r#"{"kind":"atomic","name":">=","args":[{"kind":"var","name":"out"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}"#;
                    Ok(ContractCandidate {
                        name: "good".into(),
                        pre: None,
                        post: Some(post.into()),
                        inv: None,
                        out_binding: "out".into(),
                        provenance: crate::AgentProvenance {
                            agent_name: "flaky".into(),
                            agent_version: "0".into(),
                            model: None,
                            confidence: None,
                            rationale: None,
                        },
                    })
                }
            }
            fn fix_bug(&self, _: &crate::FixContext) -> Result<crate::FixResult, AgentError> {
                unimplemented!()
            }
            fn name(&self) -> &str {
                "flaky"
            }
            fn version(&self) -> &str {
                "0"
            }
        }
        let agent = FlakyAgent {
            calls: AtomicU32::new(0),
        };
        let ctx = MustContext {
            source_path: PathBuf::from("foo.ts"),
            source_text: String::new(),
            description: "anything".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        let out = run_must_loop(&agent, ctx, &MustLoopOptions::default()).expect("loop");
        assert_eq!(out.candidate.name, "good");
        assert_eq!(out.rejected.len(), 1);
        assert_eq!(out.agent_calls, 2);
    }
}
