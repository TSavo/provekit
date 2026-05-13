// SPDX-License-Identifier: Apache-2.0
//
// `provekit lift --agent <name>` loop. Drives the propose / validate /
// mint cycle for one source file. Validation rejection reasons are fed
// back to the agent for refinement, up to N retries.
//
// Pure logic: no IO. Callers (the CLI) read the source file, supply
// the kit's authoring-API doc, and persist minted mementos.

use crate::{
    mint_validated, validate_candidate, AgentError, ContractCandidate, MintOptions,
    MintedAgentContract, ProposeContext, ProvekitAgent, ValidationOutcome,
};

#[derive(Debug, Clone)]
pub struct LiftLoopOutcome {
    /// Successfully minted contracts.
    pub minted: Vec<MintedAgentContract>,
    /// Candidates the validator rejected, with the rejection reason.
    pub rejected: Vec<(ContractCandidate, String)>,
    /// Total agent calls made.
    pub agent_calls: usize,
}

#[derive(Debug, Clone)]
pub struct LiftLoopOptions {
    /// Maximum agent retries when all candidates were rejected.
    pub max_retries: u32,
    pub mint_options: MintOptions,
}

impl Default for LiftLoopOptions {
    fn default() -> Self {
        Self {
            max_retries: 3,
            mint_options: MintOptions::default(),
        }
    }
}

pub fn run_lift_loop<A: ProvekitAgent + ?Sized>(
    agent: &A,
    initial_ctx: ProposeContext,
    opts: &LiftLoopOptions,
) -> Result<LiftLoopOutcome, AgentError> {
    let mut outcome = LiftLoopOutcome {
        minted: Vec::new(),
        rejected: Vec::new(),
        agent_calls: 0,
    };

    let mut ctx = initial_ctx;
    let mut retries = 0u32;

    loop {
        outcome.agent_calls += 1;
        let candidates = agent.propose_contracts(&ctx)?;
        if candidates.is_empty() {
            // Agent gave up; we keep whatever we already minted.
            break;
        }

        let mut all_rejected_this_round = true;
        let mut last_rejection: Option<String> = None;

        for c in candidates {
            match validate_candidate(&c) {
                ValidationOutcome::Accepted(v) => match mint_validated(&v, &opts.mint_options) {
                    Ok(m) => {
                        outcome.minted.push(m);
                        all_rejected_this_round = false;
                    }
                    Err(e) => {
                        outcome.rejected.push((c, format!("mint failure: {e}")));
                        last_rejection = Some(format!("mint failure: {e}"));
                    }
                },
                ValidationOutcome::Rejected(reason) => {
                    outcome.rejected.push((c, reason.clone()));
                    last_rejection = Some(reason);
                }
            }
        }

        if !all_rejected_this_round {
            break;
        }

        retries += 1;
        if retries > opts.max_retries {
            break;
        }

        // Feed the rejection back; ask the agent to refine.
        ctx.previous_rejection = last_rejection;
        // Tell the agent which names we already covered (if any) so it
        // doesn't loop on the same proposal.
        for m in &outcome.minted {
            if !ctx.existing_contract_names.contains(&m.name) {
                ctx.existing_contract_names.push(m.name.clone());
            }
        }
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubAgent;
    use std::path::PathBuf;

    #[test]
    fn lift_loop_mints_at_least_one_with_stub() {
        let agent = StubAgent::new();
        let ctx = ProposeContext {
            source_path: PathBuf::from("doubleledger.ts"),
            source_text: String::new(),
            function_name: None,
            authoring_api_doc: String::new(),
            existing_contract_names: vec![],
            previous_rejection: None,
        };
        let outcome = run_lift_loop(&agent, ctx, &LiftLoopOptions::default()).expect("loop");
        assert!(!outcome.minted.is_empty(), "expected at least one mint");
        for m in &outcome.minted {
            assert!(m.cid.starts_with("blake3-512:"));
        }
    }
}
