// SPDX-License-Identifier: Apache-2.0
//
// provekit-agent-claude-code — drives the `claude` CLI (Anthropic
// Claude Code) as a subprocess. The agent reads code, edits files,
// runs tests, and returns ContractCandidate / FixResult JSON to us.
//
// Status: skeleton. The `live` feature gates subprocess spawning;
// tests run with the default feature set against a `MockTransport`
// so CI does not need the `claude` binary or an API key.

use std::path::PathBuf;

use provekit_agent::{
    AgentError, AgentProvenance, ContractCandidate, FixContext, FixResult, MustContext,
    ProposeContext, ProvekitAgent,
};

/// Pluggable transport — abstracts the subprocess so tests can drop
/// in a canned-response harness.
pub trait Transport: Send + Sync {
    fn invoke(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, AgentError>;
}

#[derive(Debug, Clone)]
pub struct MockTransport {
    pub responses: std::collections::HashMap<String, serde_json::Value>,
}

impl Transport for MockTransport {
    fn invoke(
        &self,
        method: &str,
        _params: &serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        self.responses
            .get(method)
            .cloned()
            .ok_or_else(|| AgentError::Backend(format!("no canned response for {method}")))
    }
}

#[cfg(feature = "live")]
pub mod subprocess;

pub struct ClaudeCodeAgent<T: Transport> {
    transport: T,
    /// Path to the `claude` binary (informational; only used by the
    /// `live` subprocess transport).
    pub binary: Option<PathBuf>,
    pub model: Option<String>,
}

impl<T: Transport> ClaudeCodeAgent<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            binary: None,
            model: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

impl<T: Transport> ProvekitAgent for ClaudeCodeAgent<T> {
    fn propose_contracts(
        &self,
        ctx: &ProposeContext,
    ) -> Result<Vec<ContractCandidate>, AgentError> {
        let params = serde_json::to_value(ctx).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        let resp = self.transport.invoke("provekit.lift.propose", &params)?;
        let cs: Vec<ContractCandidate> =
            serde_json::from_value(resp).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        Ok(cs)
    }

    fn translate_must(&self, ctx: &MustContext) -> Result<ContractCandidate, AgentError> {
        let params = serde_json::to_value(ctx).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        let resp = self.transport.invoke("provekit.must.translate", &params)?;
        let c: ContractCandidate =
            serde_json::from_value(resp).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        Ok(c)
    }

    fn fix_bug(&self, ctx: &FixContext) -> Result<FixResult, AgentError> {
        let params = serde_json::to_value(ctx).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        let resp = self.transport.invoke("provekit.fix.patch", &params)?;
        let r: FixResult =
            serde_json::from_value(resp).map_err(|e| AgentError::InvalidIr(e.to_string()))?;
        Ok(r)
    }

    fn name(&self) -> &str {
        "claude-code"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

/// Helper: build the standard provenance the live subprocess transport
/// stamps onto candidates that lack their own.
pub fn default_provenance(model: Option<&str>) -> AgentProvenance {
    AgentProvenance {
        agent_name: "claude-code".into(),
        agent_version: env!("CARGO_PKG_VERSION").into(),
        model: model.map(|s| s.to_string()),
        confidence: None,
        rationale: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn good_candidate_json() -> serde_json::Value {
        serde_json::json!({
            "name": "ret_nonneg",
            "post": "{\"kind\":\"atomic\",\"name\":\">=\",\"args\":[{\"kind\":\"var\",\"name\":\"out\"},{\"kind\":\"const\",\"value\":0,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}]}",
            "out_binding": "out",
            "provenance": {
                "agent_name": "claude-code",
                "agent_version": "test",
                "confidence": 0.85
            }
        })
    }

    #[test]
    fn mock_transport_round_trip_must() {
        let mut m = HashMap::new();
        m.insert("provekit.must.translate".into(), good_candidate_json());
        let agent = ClaudeCodeAgent::new(MockTransport { responses: m });
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
    fn mock_transport_propose_round_trip() {
        let mut m = HashMap::new();
        m.insert(
            "provekit.lift.propose".into(),
            serde_json::json!([good_candidate_json()]),
        );
        let agent = ClaudeCodeAgent::new(MockTransport { responses: m });
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
}
