// SPDX-License-Identifier: Apache-2.0
//
// provekit-agent-openai — agent backend speaking the OpenAI chat
// completions API. Compatible with any provider that implements that
// API: Codex, vLLM, LMStudio, Ollama (with LiteLLM proxy), Together,
// Anyscale, etc.
//
// Auth: `OPENAI_API_KEY` (or whatever the configured env var is).
// Endpoint: `OPENAI_BASE_URL` for self-hosted / alternate providers.
//
// Status: skeleton; the real HTTP path is gated behind the `live`
// feature. Tests use `MockTransport` (canned JSON responses).

use std::path::PathBuf;

use provekit_agent::{
    AgentError, AgentProvenance, ContractCandidate, FixContext, FixResult, MustContext,
    ProposeContext, ProvekitAgent,
};

pub trait Transport: Send + Sync {
    fn complete(&self, system: &str, user: &str) -> Result<String, AgentError>;
}

#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    pub canned: String,
}

impl Transport for MockTransport {
    fn complete(&self, _: &str, _: &str) -> Result<String, AgentError> {
        Ok(self.canned.clone())
    }
}

#[cfg(feature = "live")]
pub mod http;

pub struct OpenAiAgent<T: Transport> {
    transport: T,
    pub model: String,
    pub api_key_env: String,
    pub base_url: Option<String>,
    pub system_prompt: String,
}

impl<T: Transport> OpenAiAgent<T> {
    pub fn new(transport: T, model: impl Into<String>) -> Self {
        Self {
            transport,
            model: model.into(),
            api_key_env: "OPENAI_API_KEY".into(),
            base_url: None,
            system_prompt: default_system_prompt().into(),
        }
    }

    pub fn with_system_prompt(mut self, sp: impl Into<String>) -> Self {
        self.system_prompt = sp.into();
        self
    }
}

pub fn default_system_prompt() -> &'static str {
    "You are a coding agent for ProvekIt. Respond ONLY with canonical \
     IR-JSON ContractCandidate(s). The validation gate will reject \
     malformed output and feed the rejection reason back; iterate."
}

fn parse_candidate(s: &str) -> Result<ContractCandidate, AgentError> {
    serde_json::from_str(s).map_err(|e| AgentError::InvalidIr(e.to_string()))
}

fn parse_candidates(s: &str) -> Result<Vec<ContractCandidate>, AgentError> {
    serde_json::from_str(s).map_err(|e| AgentError::InvalidIr(e.to_string()))
}

fn parse_fix(s: &str) -> Result<FixResult, AgentError> {
    serde_json::from_str(s).map_err(|e| AgentError::InvalidIr(e.to_string()))
}

impl<T: Transport> ProvekitAgent for OpenAiAgent<T> {
    fn propose_contracts(
        &self,
        ctx: &ProposeContext,
    ) -> Result<Vec<ContractCandidate>, AgentError> {
        let user = serde_json::to_string(ctx).unwrap_or_default();
        let resp = self.transport.complete(&self.system_prompt, &user)?;
        parse_candidates(&resp)
    }

    fn translate_must(&self, ctx: &MustContext) -> Result<ContractCandidate, AgentError> {
        let user = serde_json::to_string(ctx).unwrap_or_default();
        let resp = self.transport.complete(&self.system_prompt, &user)?;
        parse_candidate(&resp)
    }

    fn fix_bug(&self, ctx: &FixContext) -> Result<FixResult, AgentError> {
        let user = serde_json::to_string(ctx).unwrap_or_default();
        let resp = self.transport.complete(&self.system_prompt, &user)?;
        parse_fix(&resp)
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

pub fn default_provenance(model: &str) -> AgentProvenance {
    AgentProvenance {
        agent_name: "openai".into(),
        agent_version: env!("CARGO_PKG_VERSION").into(),
        model: Some(model.into()),
        confidence: None,
        rationale: None,
    }
}

/// Convenience: locate a project-local prompt override.
pub fn locate_prompt(project: &PathBuf, command: &str) -> Option<PathBuf> {
    let candidates = [
        project.join(".provekit/prompts").join(format!("{command}.openai.md")),
        project.join(".provekit/prompts").join(format!("{command}.md")),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn good_candidate_json() -> String {
        r#"{
            "name": "ret_nonneg",
            "post": "{\"kind\":\"atomic\",\"name\":\">=\",\"args\":[{\"kind\":\"var\",\"name\":\"out\"},{\"kind\":\"const\",\"value\":0,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}]}",
            "out_binding": "out",
            "provenance": {
                "agent_name": "openai",
                "agent_version": "test",
                "model": "gpt-4-turbo"
            }
        }"#
        .into()
    }

    #[test]
    fn mock_must_round_trip() {
        let t = MockTransport {
            canned: good_candidate_json(),
        };
        let agent = OpenAiAgent::new(t, "gpt-4-turbo");
        let ctx = MustContext {
            source_path: PathBuf::from("foo.ts"),
            source_text: String::new(),
            description: "x".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        let c = agent.translate_must(&ctx).expect("must");
        assert_eq!(c.name, "ret_nonneg");
    }

    #[test]
    fn mock_propose_round_trip() {
        let t = MockTransport {
            canned: format!("[{}]", good_candidate_json()),
        };
        let agent = OpenAiAgent::new(t, "gpt-4-turbo");
        let ctx = ProposeContext {
            source_path: PathBuf::from("foo.ts"),
            source_text: String::new(),
            function_name: None,
            authoring_api_doc: String::new(),
            existing_contract_names: vec![],
            previous_rejection: None,
        };
        let cs = agent.propose_contracts(&ctx).expect("propose");
        assert_eq!(cs.len(), 1);
    }

    #[test]
    fn mock_invalid_json_rejected() {
        let t = MockTransport {
            canned: "{not json}".into(),
        };
        let agent = OpenAiAgent::new(t, "gpt-4-turbo");
        let ctx = MustContext {
            source_path: PathBuf::from("foo.ts"),
            source_text: String::new(),
            description: "x".into(),
            authoring_api_doc: String::new(),
            previous_rejection: None,
        };
        match agent.translate_must(&ctx) {
            Err(AgentError::InvalidIr(_)) => {}
            other => panic!("expected InvalidIr; got {other:?}"),
        }
    }
}
