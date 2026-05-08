// SPDX-License-Identifier: Apache-2.0
//
// Stub agent — canned responses for tests + CI. Required so the
// verification loops can be exercised without network or a real
// coding-agent installation.
//
// The stub recognises a handful of English phrases and returns
// canonical IR-JSON. Anything it doesn't recognise becomes a generic
// "out >= 0" postcondition (which is structurally valid IR-JSON and
// passes `validate_candidate`). For `fix` it produces a no-op patch
// that overwrites the file with its existing contents and a single
// "// fix attempted by stub agent" comment line, plus an associated
// new contract whose name is derived from the bug description.
//
// The stub also carries a small built-in dictionary of "named demos"
// so the doubleledger acceptance test can request the conservation
// contract by phrase.

use crate::{
    AgentError, AgentProvenance, ContractCandidate, FilePatch, FixContext, FixResult, MustContext,
    ProposeContext, ProvekitAgent,
};

/// Stub agent — returns canned ContractCandidates for every request.
/// Use in CI and during development; never as a default in production
/// (the user should always pick a real backend explicitly).
#[derive(Debug, Clone, Default)]
pub struct StubAgent;

impl StubAgent {
    pub fn new() -> Self {
        Self
    }
}

fn provenance() -> AgentProvenance {
    AgentProvenance {
        agent_name: "stub".into(),
        agent_version: env!("CARGO_PKG_VERSION").into(),
        model: None,
        confidence: Some(0.5),
        rationale: Some("canned stub response".into()),
    }
}

/// Default safe candidate: postcondition `out >= 0`.
fn default_post_nonneg(name: impl Into<String>) -> ContractCandidate {
    let post = r#"{"kind":"atomic","name":">=","args":[{"kind":"var","name":"out"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}"#;
    ContractCandidate {
        name: name.into(),
        pre: None,
        post: Some(post.into()),
        inv: None,
        out_binding: "out".into(),
        provenance: provenance(),
    }
}

/// Double-entry conservation: forall txn. sum(debits)(txn) == sum(credits)(txn).
/// We encode this as an invariant whose IR-JSON shape uses kit primitives:
///   forall t:Int. atomic("=") [ ctor("sumDebits")(var t), ctor("sumCredits")(var t) ]
fn doubleledger_conservation() -> ContractCandidate {
    let inv = r#"{"kind":"forall","name":"txn","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","name":"=","args":[{"kind":"ctor","name":"sumDebits","args":[{"kind":"var","name":"txn"}]},{"kind":"ctor","name":"sumCredits","args":[{"kind":"var","name":"txn"}]}]}}"#;
    ContractCandidate {
        name: "doubleledger_conservation".into(),
        pre: None,
        post: None,
        inv: Some(inv.into()),
        out_binding: "out".into(),
        provenance: AgentProvenance {
            agent_name: "stub".into(),
            agent_version: env!("CARGO_PKG_VERSION").into(),
            model: None,
            confidence: Some(0.95),
            rationale: Some(
                "double-entry conservation: every transaction must satisfy sum(debits) == sum(credits)"
                    .into(),
            ),
        },
    }
}

/// Recognise a few canonical English phrases and map them to known
/// contracts. Anything else → fall back to `out >= 0` with a name
/// derived from the description.
fn recognise(description: &str, fallback_name: &str) -> ContractCandidate {
    let d = description.to_lowercase();
    if d.contains("not lose money")
        || d.contains("conservation")
        || d.contains("double-entry")
        || d.contains("double entry")
        || d.contains("debits == credits")
    {
        return doubleledger_conservation();
    }
    if d.contains("non-negative") || d.contains("nonneg") || d.contains("positive") {
        return default_post_nonneg(fallback_name);
    }
    default_post_nonneg(fallback_name)
}

/// Slugify a phrase into a snake_case identifier suitable for use as
/// a contract name when the agent needs to invent one.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "stub_contract".into()
    } else {
        out
    }
}

impl ProvekitAgent for StubAgent {
    fn propose_contracts(
        &self,
        ctx: &ProposeContext,
    ) -> Result<Vec<ContractCandidate>, AgentError> {
        // Canned: one candidate per public function we can heuristically
        // detect, plus a doubleledger demo if the source path file name
        // hints at it.
        let mut out = Vec::new();
        let stem = ctx
            .source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source");
        if stem.contains("doubleledger") || stem.contains("ledger") {
            out.push(doubleledger_conservation());
        }
        // Always at least one candidate so callers have something to work
        // with; the loop layer dedupes against `existing_contract_names`.
        let fallback = format!("{stem}_safe");
        if !ctx.existing_contract_names.iter().any(|n| n == &fallback) {
            out.push(default_post_nonneg(fallback));
        }
        Ok(out)
    }

    fn translate_must(&self, ctx: &MustContext) -> Result<ContractCandidate, AgentError> {
        let stem = ctx
            .source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source");
        let fallback_name = format!("{}_{}", stem, slugify(&ctx.description));
        Ok(recognise(&ctx.description, &fallback_name))
    }

    fn fix_bug(&self, ctx: &FixContext) -> Result<FixResult, AgentError> {
        // Stub fix: produces a no-op informational patch only when an
        // allowed_path is given; otherwise empty patches. Returns a
        // single new contract derived from the bug description.
        let mut patches = Vec::new();
        if let Some(p) = ctx.allowed_paths.first() {
            // Append a comment marker without losing existing content.
            let original = std::fs::read_to_string(p).unwrap_or_default();
            let new_content = format!("{original}\n// fix attempted by stub agent\n");
            patches.push(FilePatch {
                path: p.clone(),
                new_content,
                old_content: Some(original),
            });
        }
        let new_contract = default_post_nonneg(slugify(&ctx.bug_description));
        Ok(FixResult {
            patches,
            new_contracts: vec![new_contract],
            commentary: format!(
                "stub fix for: {}",
                ctx.bug_description.chars().take(120).collect::<String>()
            ),
        })
    }

    fn name(&self) -> &str {
        "stub"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stub_recognises_doubleledger() {
        let agent = StubAgent::new();
        let ctx = MustContext {
            source_path: PathBuf::from("doubleledger.ts"),
            source_text: "// ledger".into(),
            description: "not lose money".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        let c = agent.translate_must(&ctx).expect("translate");
        assert_eq!(c.name, "doubleledger_conservation");
        assert!(c.inv.is_some());
    }

    #[test]
    fn stub_fallback_for_unknown_english() {
        let agent = StubAgent::new();
        let ctx = MustContext {
            source_path: PathBuf::from("foo.ts"),
            source_text: String::new(),
            description: "do something reasonable".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        let c = agent.translate_must(&ctx).expect("translate");
        assert!(c.post.is_some());
        assert!(c.name.contains("foo"));
    }

    #[test]
    fn stub_propose_returns_at_least_one() {
        let agent = StubAgent::new();
        let ctx = ProposeContext {
            source_path: PathBuf::from("doubleledger.ts"),
            source_text: String::new(),
            function_name: None,
            authoring_api_doc: String::new(),
            existing_contract_names: vec![],
            previous_rejection: None,
        };
        let cs = agent.propose_contracts(&ctx).expect("propose");
        assert!(!cs.is_empty());
    }
}
