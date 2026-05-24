// SPDX-License-Identifier: Apache-2.0
//
// provekit-agent: pluggable coding-agent surface for ProvekIt.
//
// Two directions, both first-class:
//
//   1. ProvekIt drives an external coding agent. The CLI hands the
//      agent structured context (source file, IR primitives, the lift
//      spec, an English description, or a bug report); the agent
//      proposes ContractCandidates / FilePatches; the CLI validates
//      and mints surviving candidates.
//
//   2. Agents drive ProvekIt. Every CLI subcommand supports `--json`
//      and `--strict-shapes`; the agent reads parseable output and
//      decides next action.
//
// This crate is the seam: a `ProvekitAgent` trait, the message types
// passed across the seam (ProposeContext / MustContext / FixContext +
// ContractCandidate / FixResult), a `StubAgent` that returns canned
// responses for tests + CI, and the propose/validate/mint loop that
// the `must`, `lift`, and `fix` CLI subcommands share.
//
// Network calls are clearly opt-in: this crate does no IO. Real
// backends live in sibling crates (provekit-agent-claude-code,
// provekit-agent-openai) and decide for themselves whether to spawn
// subprocesses or open sockets.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod loop_fix;
pub mod loop_lift;
pub mod loop_must;
pub mod stub;

pub use loop_fix::{run_fix_loop, FixLoopOptions, FixLoopOutcome};
pub use loop_lift::{run_lift_loop, LiftLoopOptions, LiftLoopOutcome};
pub use loop_must::{run_must_loop, MustLoopOptions, MustLoopOutcome};
pub use stub::StubAgent;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent backend error: {0}")]
    Backend(String),
    #[error("agent returned invalid IR-JSON: {0}")]
    InvalidIr(String),
    #[error("agent returned no candidates")]
    Empty,
    #[error("io error: {0}")]
    Io(String),
}

// ---------------------------------------------------------------------------
// Provenance: recorded on every minted memento so the lattice knows
// who proposed what.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvenance {
    /// Backend name: "stub", "claude-code", "openai", etc.
    pub agent_name: String,
    /// Free-form version string from the backend.
    pub agent_version: String,
    /// Optional model identifier (e.g. "claude-opus-4-7", "gpt-4-turbo").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Backend-supplied confidence in [0.0, 1.0]. None when the backend
    /// has no calibrated notion (e.g. the stub).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Free-form rationale string for audit trails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

// ---------------------------------------------------------------------------
// Candidate contract: one proposal in IR-JSON canonical form. The
// agent serializes its formula trees to IR-JSON strings; we parse +
// validate before minting.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCandidate {
    pub name: String,
    /// IR-JSON formula (the kit-shape JSON for a Formula tree).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inv: Option<String>,
    /// Default to "out" if the agent omits it; we'll fill in below.
    #[serde(default = "default_out_binding")]
    pub out_binding: String,
    pub provenance: AgentProvenance,
}

fn default_out_binding() -> String {
    "out".into()
}

// ---------------------------------------------------------------------------
// File patch: minimal shape: { path, new_content }. Optional
// old_content lets the patch carry a precondition (matches what's on
// disk). Unified-diff parsing is intentionally not on the table for
// v0; full-file replacement is the simplest contract that survives
// adversarial agents.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub path: PathBuf,
    pub new_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_content: Option<String>,
}

// ---------------------------------------------------------------------------
// Result returned by `fix_bug`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub patches: Vec<FilePatch>,
    #[serde(default)]
    pub new_contracts: Vec<ContractCandidate>,
    #[serde(default)]
    pub commentary: String,
}

// ---------------------------------------------------------------------------
// Contexts handed to each agent call.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeContext {
    /// Path of the source file the agent should read.
    pub source_path: PathBuf,
    /// File contents (the CLI reads once and forwards; saves the
    /// agent a round-trip).
    pub source_text: String,
    /// Optional function-name filter; None means propose freely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// The kit's authoring API as a documentation string. Backends are
    /// expected to inline this in their prompt.
    pub authoring_api_doc: String,
    /// Names of contracts already in the lattice for this file (so
    /// the agent doesn't re-propose duplicates).
    #[serde(default)]
    pub existing_contract_names: Vec<String>,
    /// Optional rejection feedback from a previous round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_rejection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MustContext {
    pub source_path: PathBuf,
    pub source_text: String,
    /// English description: "parseInt requires positive input".
    pub description: String,
    pub authoring_api_doc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_rejection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixContext {
    /// Repository root.
    pub repo_root: PathBuf,
    /// English bug description.
    pub bug_description: String,
    /// Names of contracts the bug is suspected to violate.
    #[serde(default)]
    pub violated_contracts: Vec<String>,
    /// Files the agent is allowed to read/edit. Empty = whole repo.
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,
    /// Optional report from a previous failed attempt (build errors,
    /// remaining contract violations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_rejection: Option<String>,
}

// ---------------------------------------------------------------------------
// The trait.
// ---------------------------------------------------------------------------

/// The pluggable seam. Implementations live in sibling crates.
pub trait ProvekitAgent: Send + Sync {
    /// Lift contracts from a source file. Returns zero or more
    /// candidates in canonical IR-JSON.
    fn propose_contracts(&self, ctx: &ProposeContext)
        -> Result<Vec<ContractCandidate>, AgentError>;

    /// Translate an English description to one IR contract.
    fn translate_must(&self, ctx: &MustContext) -> Result<ContractCandidate, AgentError>;

    /// Fix a bug. Returns code patches + any new contracts the fix
    /// implies.
    fn fix_bug(&self, ctx: &FixContext) -> Result<FixResult, AgentError>;

    fn name(&self) -> &str;
    fn version(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Validation: shared between the lift / must loops. Parses the
// candidate's IR-JSON, returns a typed `ContractDecl` on success or a
// rejection reason the agent can read.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    /// Candidate parsed and is structurally valid. Carries the parsed
    /// canonical-Value forms suitable for `mint_contract`.
    Accepted(ValidatedCandidate),
    /// Candidate is malformed. Carries a human-readable reason that
    /// will be sent back to the agent for the next round.
    Rejected(String),
}

#[derive(Debug, Clone)]
pub struct ValidatedCandidate {
    pub name: String,
    pub out_binding: String,
    /// Canonicalizer Value forms ready to feed into `mint_contract`.
    pub pre_value: Option<std::sync::Arc<provekit_canonicalizer::Value>>,
    pub post_value: Option<std::sync::Arc<provekit_canonicalizer::Value>>,
    pub inv_value: Option<std::sync::Arc<provekit_canonicalizer::Value>>,
    pub provenance: AgentProvenance,
}

/// Validate a single candidate: parse each IR-JSON formula, ensure
/// at least one of pre/post/inv is present, and convert to the
/// canonicalizer Value form the claim envelope expects.
pub fn validate_candidate(c: &ContractCandidate) -> ValidationOutcome {
    use provekit_ir_symbolic::{parse, serialize};

    if c.name.trim().is_empty() {
        return ValidationOutcome::Rejected("candidate name is empty".into());
    }
    if c.pre.is_none() && c.post.is_none() && c.inv.is_none() {
        return ValidationOutcome::Rejected(
            "at least one of `pre`, `post`, or `inv` must be provided".into(),
        );
    }
    if c.out_binding.trim().is_empty() {
        return ValidationOutcome::Rejected("out_binding must be non-empty".into());
    }

    fn parse_one(
        label: &str,
        s: &str,
    ) -> Result<std::sync::Arc<provekit_canonicalizer::Value>, String> {
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("`{label}` is not valid JSON: {e}"))?;
        let f =
            parse::parse_formula(&v).map_err(|e| format!("`{label}` is not valid IR-JSON: {e}"))?;
        Ok(serialize::formula_to_value(&f))
    }

    let pre_value = match c.pre.as_deref().map(|s| parse_one("pre", s)) {
        None => None,
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => return ValidationOutcome::Rejected(e),
    };
    let post_value = match c.post.as_deref().map(|s| parse_one("post", s)) {
        None => None,
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => return ValidationOutcome::Rejected(e),
    };
    let inv_value = match c.inv.as_deref().map(|s| parse_one("inv", s)) {
        None => None,
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => return ValidationOutcome::Rejected(e),
    };

    ValidationOutcome::Accepted(ValidatedCandidate {
        name: c.name.clone(),
        out_binding: c.out_binding.clone(),
        pre_value,
        post_value,
        inv_value,
        provenance: c.provenance.clone(),
    })
}

// ---------------------------------------------------------------------------
// Mint a validated candidate as a signed memento.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MintOptions {
    pub produced_by: String,
    pub produced_at: String,
    pub signer_seed: provekit_proof_envelope::Ed25519Seed,
}

impl Default for MintOptions {
    fn default() -> Self {
        Self {
            produced_by: format!("provekit-agent@{}", env!("CARGO_PKG_VERSION")),
            produced_at: "2026-04-30T00:00:00.000Z".into(),
            signer_seed: [0x42; 32],
        }
    }
}

#[derive(Debug, Clone)]
pub struct MintedAgentContract {
    pub name: String,
    pub cid: String,
    pub canonical_bytes: Vec<u8>,
    pub provenance: AgentProvenance,
}

pub fn mint_validated(
    v: &ValidatedCandidate,
    opts: &MintOptions,
) -> Result<MintedAgentContract, String> {
    use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};

    let evidence = format!(
        "agent {}@{} proposed via provekit-agent",
        v.provenance.agent_name, v.provenance.agent_version
    );

    let args = MintContractArgs {
        formals: Vec::new(),
        formal_sorts: Vec::new(),
        contract_name: v.name.clone(),
        pre: v.pre_value.clone(),
        post: v.post_value.clone(),
        inv: v.inv_value.clone(),
        out_binding: v.out_binding.clone(),
        produced_by: opts.produced_by.clone(),
        produced_at: opts.produced_at.clone(),
        input_cids: vec![],
        // Distinguish agent-authored from kit-author / lift authoring.
        // We use Authoring::Llm because the provenance maps cleanly to
        // its (llm, llm_version, prompt_cid, confidence, rationale)
        // fields; the stub agent gets prompt_cid = its own name hash.
        authoring: Authoring::Llm {
            llm: v.provenance.agent_name.clone(),
            llm_version: v.provenance.agent_version.clone(),
            prompt_cid: format!("blake3-512:{}", hex_zero_pad(&v.provenance.agent_name)),
            confidence: v.provenance.confidence.unwrap_or(0.5),
            rationale: v
                .provenance
                .rationale
                .clone()
                .or_else(|| Some(evidence.clone())),
        },
        signer_seed: opts.signer_seed,
    };

    let m = mint_contract(&args).map_err(|e| e.to_string())?;
    Ok(MintedAgentContract {
        name: v.name.clone(),
        cid: m.cid,
        canonical_bytes: m.canonical_bytes,
        provenance: v.provenance.clone(),
    })
}

/// Pad a string into a 128-hex-char placeholder. The agent's
/// `prompt_cid` is filled in this way for v0; future revisions will
/// hash the actual prompt bytes.
fn hex_zero_pad(s: &str) -> String {
    let mut out = String::with_capacity(128);
    for b in s.bytes() {
        out.push_str(&format!("{b:02x}"));
        if out.len() >= 128 {
            break;
        }
    }
    while out.len() < 128 {
        out.push('0');
    }
    out
}

// ---------------------------------------------------------------------------
// Tool descriptor: dropped into Claude Code / Continue / Cursor /
// Aider config so external agents can call ProvekIt as a tool.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub provekit_version: String,
    pub protocol_cid: String,
    pub tools: Vec<ToolEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub exit_codes: Vec<ExitCodeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitCodeEntry {
    pub code: u8,
    pub meaning: String,
}

/// Build the canonical descriptor. Callers supply the catalog CID
/// because this crate is decoupled from `provekit-cli` (no cycles).
pub fn build_tool_descriptor(provekit_version: &str, protocol_cid: &str) -> ToolDescriptor {
    use serde_json::json;
    ToolDescriptor {
        provekit_version: provekit_version.into(),
        protocol_cid: protocol_cid.into(),
        tools: vec![
            ToolEntry {
                name: "provekit prove".into(),
                description: "Run the verifier across all .proof catalogs in a project. Returns total callsites, discharged count, violations list.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "project": {"type": "string", "description": "Project root (default: cwd)"},
                        "z3": {"type": "string", "description": "Path to z3 binary"},
                        "json": {"type": "boolean"}
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": {
                        "totalCallsites": {"type": "integer"},
                        "discharged": {"type": "integer"},
                        "violations": {"type": "integer"},
                        "rows": {"type": "array"}
                    }
                }),
                exit_codes: vec![
                    ExitCodeEntry { code: 0, meaning: "all callsites discharged".into() },
                    ExitCodeEntry { code: 1, meaning: "verification failure (one or more violations)".into() },
                    ExitCodeEntry { code: 3, meaning: "solver unavailable or timeout".into() },
                ],
            },
            ToolEntry {
                name: "provekit hash".into(),
                description: "Compute the BLAKE3-512 self-identifying CID of a file (or stdin).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "path": {"type": "string"} }
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": { "cid": {"type": "string"} }
                }),
                exit_codes: vec![ExitCodeEntry { code: 0, meaning: "ok".into() }],
            },
            ToolEntry {
                name: "provekit must".into(),
                description: "Translate an English description to a ProvekIt contract. Hands the source file + description to the configured agent; validates and mints the proposal.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "description": {"type": "string"},
                        "source_file": {"type": "string"},
                        "agent": {"type": "string", "default": "stub"},
                        "max_retries": {"type": "integer", "default": 3}
                    },
                    "required": ["description", "source_file"]
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": {
                        "minted_cid": {"type": "string"},
                        "name": {"type": "string"}
                    }
                }),
                exit_codes: vec![
                    ExitCodeEntry { code: 0, meaning: "candidate accepted + minted".into() },
                    ExitCodeEntry { code: 1, meaning: "all retries exhausted with rejected candidates".into() },
                    ExitCodeEntry { code: 2, meaning: "user error (bad args, file not found)".into() },
                ],
            },
            ToolEntry {
                name: "provekit lift".into(),
                description: "Run the configured agent over a source file to propose contracts. Each candidate is validated and minted; rejected candidates are returned to the agent for refinement.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": {"type": "string"},
                        "agent": {"type": "string", "default": "stub"},
                        "function": {"type": "string"},
                        "max_retries": {"type": "integer", "default": 3}
                    },
                    "required": ["file"]
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": {
                        "minted": {"type": "array"},
                        "rejected": {"type": "array"}
                    }
                }),
                exit_codes: vec![
                    ExitCodeEntry { code: 0, meaning: "at least one candidate minted".into() },
                    ExitCodeEntry { code: 1, meaning: "all candidates rejected".into() },
                ],
            },
            ToolEntry {
                name: "provekit fix".into(),
                description: "Hand the configured agent a bug description; receive code patches; apply them in a temp tree; rerun the build/verify loop; on success, prompt the user to apply.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "bug": {"type": "string"},
                        "agent": {"type": "string", "default": "stub"},
                        "max_retries": {"type": "integer", "default": 3},
                        "auto_apply": {"type": "boolean", "default": false}
                    },
                    "required": ["bug"]
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": {
                        "patches": {"type": "array"},
                        "applied": {"type": "boolean"}
                    }
                }),
                exit_codes: vec![
                    ExitCodeEntry { code: 0, meaning: "fix verified (and applied if --auto-apply)".into() },
                    ExitCodeEntry { code: 1, meaning: "fix failed verification after retries".into() },
                ],
            },
            ToolEntry {
                name: "provekit ask".into(),
                description: "Look up an IR-JSON formula by content hash. Returns the canonical CID.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "formula": {"type": "string"} }
                }),
                output_schema: json!({
                    "type": "object",
                    "properties": { "cid": {"type": "string"} }
                }),
                exit_codes: vec![ExitCodeEntry { code: 0, meaning: "ok".into() }],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn good_atom(out_binding: &str, name: &str) -> ContractCandidate {
        // A valid IR-JSON post: out >= 0
        let post = r#"{"kind":"atomic","name":">=","args":[{"kind":"var","name":"out"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}"#;
        ContractCandidate {
            name: name.into(),
            pre: None,
            post: Some(post.into()),
            inv: None,
            out_binding: out_binding.into(),
            provenance: AgentProvenance {
                agent_name: "test".into(),
                agent_version: "0.0.1".into(),
                model: None,
                confidence: Some(0.9),
                rationale: None,
            },
        }
    }

    #[test]
    fn validate_accepts_well_formed() {
        let c = good_atom("out", "ret_nonneg");
        match validate_candidate(&c) {
            ValidationOutcome::Accepted(v) => {
                assert_eq!(v.name, "ret_nonneg");
                assert!(v.post_value.is_some());
                assert!(v.pre_value.is_none());
            }
            ValidationOutcome::Rejected(r) => panic!("expected accept; got rejected: {r}"),
        }
    }

    #[test]
    fn validate_rejects_empty_contract() {
        let mut c = good_atom("out", "x");
        c.post = None;
        match validate_candidate(&c) {
            ValidationOutcome::Rejected(r) => {
                assert!(r.contains("pre"), "reason: {r}");
            }
            _ => panic!("expected rejection"),
        }
    }

    #[test]
    fn validate_rejects_malformed_ir_json() {
        let mut c = good_atom("out", "x");
        c.post = Some("{not valid json}".into());
        match validate_candidate(&c) {
            ValidationOutcome::Rejected(r) => {
                assert!(r.contains("not valid"), "reason: {r}");
            }
            _ => panic!("expected rejection"),
        }
    }

    #[test]
    fn validate_rejects_empty_name() {
        let c = good_atom("out", "   ");
        match validate_candidate(&c) {
            ValidationOutcome::Rejected(r) => {
                assert!(r.contains("name"), "reason: {r}");
            }
            _ => panic!("expected rejection"),
        }
    }

    #[test]
    fn mint_round_trip_produces_cid() {
        let c = good_atom("out", "ret_nonneg");
        let v = match validate_candidate(&c) {
            ValidationOutcome::Accepted(v) => v,
            ValidationOutcome::Rejected(r) => panic!("rejected: {r}"),
        };
        let opts = MintOptions::default();
        let m = mint_validated(&v, &opts).expect("mint");
        assert!(m.cid.starts_with("blake3-512:"), "cid: {}", m.cid);
        assert!(!m.canonical_bytes.is_empty());
    }

    #[test]
    fn tool_descriptor_lists_known_tools() {
        let d = build_tool_descriptor("0.1.0", "blake3-512:dead");
        assert_eq!(d.provekit_version, "0.1.0");
        assert!(d.tools.len() >= 5);
        let names: Vec<&str> = d.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("must")));
        assert!(names.iter().any(|n| n.contains("lift")));
        assert!(names.iter().any(|n| n.contains("fix")));
    }

    #[test]
    fn tool_descriptor_round_trips_to_json() {
        let d = build_tool_descriptor("0.1.0", "blake3-512:dead");
        let s = serde_json::to_string(&d).expect("ser");
        let d2: ToolDescriptor = serde_json::from_str(&s).expect("de");
        assert_eq!(d2.tools.len(), d.tools.len());
    }
}
