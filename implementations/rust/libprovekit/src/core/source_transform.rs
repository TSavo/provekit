// SPDX-License-Identifier: Apache-2.0
//
// Source-transform primitives extracted from `provekit-cli::cmd_materialize`
// per `#1335` (umbrella `#1334`). These functions are the mechanical core of
// turning concept-citation carriers in source files into library-bound source
// by composing the existing LowerKit/realize path. They are surfaced here
// (Phase A) so that the upcoming `SiteTransformKit` trait (Phase B) can be
// built on a stable extraction without disturbing the CLI consumer.
//
// Phase A is a verbatim move: no behavior changes, no formatting changes to
// function bodies, no new error types, no new dependencies. The CLI continues
// to call these via a glob `use` re-export.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use serde_json::Value as Json;

/// A captured stub function block: how many lines it spans, and (when the
/// block is well-formed) the signature-and-close pair used by the
/// substrate-honest signature-preservation path. The consumer's signature
/// (including generics, lifetimes, where-clauses) is the signed claim;
/// the body is what materialize fills from the shim's binding (#1332).
pub struct CapturedStubBlock {
    pub line_count: usize,
    pub signature_and_close: Option<StubSignatureAndClose>,
}

pub struct StubSignatureAndClose {
    /// Concatenated text from the `fn ...` line through (and including) the
    /// opening `{`, exactly as written by the consumer. Used as the
    /// emitted function's signature.
    pub signature_text: String,
    /// Indentation of the closing `}` line as the consumer wrote it.
    /// Reused when re-emitting the closing brace so the materialized
    /// function looks like the consumer's original.
    pub close_indent: String,
}

/// Capture the stub function block that follows a carrier comment, if any.
/// Returns a `CapturedStubBlock` describing how many lines to skip and (if
/// the block is well-formed) the consumer's exact signature text plus the
/// indentation of its closing brace. The caller emits the materialized
/// function by splicing the realize plugin's body into this signature
/// (#1332).
///
/// If no stub function follows the carrier (or the block is unbalanced),
/// `line_count` is 0 and `signature_and_close` is `None`; the caller falls
/// back to emitting the realize plugin's full source as-is.
pub fn capture_stub_function_block(lines: &[&str]) -> CapturedStubBlock {
    let none = CapturedStubBlock {
        line_count: 0,
        signature_and_close: None,
    };
    let Some(first_line) = lines.first() else {
        return none;
    };
    if !line_starts_function_declaration(first_line) {
        return none;
    }
    let mut depth: i32 = 0;
    let mut saw_open = false;
    let mut open_line_idx: Option<usize> = None;
    for (offset, line) in lines.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => {
                    if !saw_open {
                        open_line_idx = Some(offset);
                    }
                    depth += 1;
                    saw_open = true;
                }
                '}' => {
                    depth -= 1;
                    if saw_open && depth == 0 {
                        let line_count = offset + 1;
                        // Signature is lines[0..=open_line_idx], up to and
                        // including the opening `{`. Trim the opening
                        // brace from the signature text and add it back
                        // separately so we can splice a body between.
                        let Some(open_offset) = open_line_idx else {
                            return CapturedStubBlock {
                                line_count,
                                signature_and_close: None,
                            };
                        };
                        let signature_text =
                            lines[..=open_offset].iter().copied().collect::<String>();
                        let close_indent = leading_indent(lines[offset]).to_string();
                        return CapturedStubBlock {
                            line_count,
                            signature_and_close: Some(StubSignatureAndClose {
                                signature_text,
                                close_indent,
                            }),
                        };
                    }
                }
                _ => {}
            }
        }
    }
    none
}

pub fn leading_indent(line: &str) -> &str {
    let trimmed_len = line.trim_start().len();
    &line[..line.len() - trimmed_len]
}

/// Splice the realize plugin's body into the consumer's stub signature.
/// The realize plugin returns a full function declaration; we drop its
/// signature and keep only the body inside its outermost braces. The
/// consumer's stub signature wraps that body. Substrate-honest: the
/// consumer's signed signature is preserved exactly, only the body
/// changes (#1332).
pub fn splice_realized_body_into_stub_signature(
    stub: &StubSignatureAndClose,
    realized_source: &str,
) -> String {
    let body = extract_function_body(realized_source).unwrap_or_default();
    let mut out = String::new();
    out.push_str(&stub.signature_text);
    if !stub.signature_text.ends_with('\n') {
        out.push('\n');
    }
    for line in body.lines() {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&stub.close_indent);
    out.push('}');
    out.push('\n');
    out
}

/// Extract the contents between the outermost `{` and matching `}` of a
/// function source string. The realize plugin's emitted source is a single
/// `fn ... { ... }` declaration; this returns just the inner body, trimmed
/// to the lines between the outermost braces. Body content is returned
/// without surrounding indentation normalization; the caller is responsible
/// for any wrapping or re-indentation.
pub fn extract_function_body(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut end: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if start.is_none() {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if e >= s => {
            let raw = &source[s..e];
            Some(raw.trim_matches('\n').to_string())
        }
        _ => None,
    }
}

pub fn line_starts_function_declaration(line: &str) -> bool {
    let trimmed = line.trim_start();
    const KEYWORDS_TO_STRIP: &[&str] = &[
        "pub ", "async ", "const ", "unsafe ", "extern ", "default ",
    ];
    let mut remaining = trimmed;
    loop {
        let mut stripped = false;
        if remaining.starts_with("pub(") {
            if let Some(rest) = remaining.split_once(')').map(|(_, r)| r.trim_start()) {
                remaining = rest;
                stripped = true;
            }
        }
        for kw in KEYWORDS_TO_STRIP {
            if let Some(rest) = remaining.strip_prefix(kw) {
                remaining = rest.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    remaining.starts_with("fn ") || remaining.starts_with("fn(")
}

pub fn concept_payload_from_line(line: &str) -> Option<(&str, &str)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept: ")
        .map(str::trim)
        .map(|payload| (indent, payload))
}

pub fn concept_payload_cid_from_line(line: &str) -> Option<&str> {
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept-payload-cid: ")
        .map(str::trim)
}

pub fn strip_comment_prefix(line: &str) -> Option<&str> {
    let body = line
        .strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))
        .or_else(|| line.strip_prefix("/*"))?
        .trim_start();
    Some(
        body.trim_end()
            .strip_suffix("*/")
            .map(str::trim_end)
            .unwrap_or(body),
    )
}

pub fn indent_realized_source(source: &str, indent: &str) -> String {
    if indent.is_empty() {
        return source.to_string();
    }
    source
        .split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect()
}

pub fn verify_payload_cid(payload: &str, declared_cid: &str) -> Result<(), String> {
    let parsed: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let canonical = canonical_value_from_json(&parsed)?;
    let actual_cid = blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes());
    if actual_cid != declared_cid {
        return Err(format!(
            "provekit-concept-payload-cid mismatch: declared {declared_cid}, computed {actual_cid}"
        ));
    }
    Ok(())
}

pub fn canonical_value_from_json(value: &Json) -> Result<Arc<CanonicalValue>, String> {
    match value {
        Json::Null => Ok(CanonicalValue::null()),
        Json::Bool(value) => Ok(CanonicalValue::boolean(*value)),
        Json::Number(value) => value.as_i64().map(CanonicalValue::integer).ok_or_else(|| {
            format!("provekit-concept payload contains non-integer number `{value}`")
        }),
        Json::String(value) => Ok(CanonicalValue::string(value)),
        Json::Array(values) => values
            .iter()
            .map(canonical_value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::array),
        Json::Object(entries) => entries
            .iter()
            .map(|(key, value)| canonical_value_from_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::object),
    }
}

// Permissive-defaults for carrier payloads. The materialize command synthesizes
// defaults for missing carrier-payload fields (function, params, param_types,
// return_type) to reduce friction during development. The substrate-honest
// alternative is refuse-on-missing-fields: require each carrier author to
// provide a complete payload. The permissive shape is intentional for the
// build-pipeline ergonomics; the trade-off is that incomplete carriers may
// produce stub-like realize output. A future strict mode (e.g., a
// --strict-payloads flag) could refuse incomplete carriers up front.
// -----------------------------------------------------------------------------
// Phase B (`#1336`): the unified site-transformation primitive.
//
// The materialize and migrate commands both walk a source file, find
// `provekit-concept:` carrier comments, optionally consume the consumer's
// stub function declaration that follows, dispatch to a kit-specific binding
// resolver, and splice the realized body back into the consumer's signature.
// Phase A extracted the mechanical primitives (carrier parsing, stub capture,
// body splice). Phase B unifies the dispatch surface as `SiteTransformKit`
// and the loop as `transform_source_text`. Phase C will reroute
// `cmd_materialize::materialize_source_text` through this trait; Phase D will
// do the same for `cmd_bind_migrate`; Phase E will unify the receipt
// envelope so refusals carry through both flows by the same shape.
//
// This phase changes no CLI behavior. It adds the trait surface that the
// existing commands will route through in subsequent phases.

/// The trichotomy outcome of a single site transformation. Matches the
/// substrate-honest first-principle: every site clears either with
/// byte-exact realization, with declared bounded loss, or with refusal that
/// names the concept hub whose minting would close the gap.
#[derive(Debug, Clone)]
pub enum SiteOutcome {
    /// Site cleared with byte-exact realization. `body` is the spliced
    /// function body (the contents between the outermost braces of the
    /// realized function declaration); `binding_cid` pins the kit binding
    /// that realized it; `loss_record` is the (possibly empty) loss-record
    /// contribution carried by that binding.
    Materialize {
        body: String,
        binding_cid: String,
        loss_record: Json,
    },

    /// Site cleared with declared bounded loss. `body` is the spliced
    /// function body; `binding_cid` pins the kit binding; `declared_loss`
    /// is the list of dimensions where the realization is bounded-lossy.
    LoudlyLossy {
        body: String,
        binding_cid: String,
        declared_loss: Vec<String>,
    },

    /// Site cannot clear under the requested transformation. `reason` is a
    /// substrate-honest sentence explaining why; `would_close_with_concept`
    /// names the concept hub CID (or human-readable concept name) that, if
    /// minted with an `N>=2` cross-library cluster, would close the
    /// refusal.
    Refuse {
        reason: String,
        would_close_with_concept: String,
    },
}

/// A parsed `provekit-concept:` carrier comment in typed form. The
/// materialize and migrate commands both consume this. Phase A moved the
/// carrier-parsing primitive (`concept_payload_from_line`); this struct is
/// the typed projection of the JSON payload that follows it.
#[derive(Debug, Clone)]
pub struct CarrierComment {
    pub concept_name: String,
    pub function: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub library_tag: Option<String>,
    /// The raw JSON payload as it appeared in the source comment. Kept for
    /// the carrier-payload-cid verification path that Phase A preserved
    /// (`verify_payload_cid` recomputes JCS+blake3 over the same string the
    /// consumer signed for).
    pub raw_payload: String,
}

impl CarrierComment {
    /// Parse the payload portion of a `provekit-concept: <JSON>` line.
    /// The payload string is the value Phase A's `concept_payload_from_line`
    /// returns; this function consumes it into typed fields with the same
    /// permissive defaults that `realize_spec_from_payload` applies (missing
    /// `function` becomes `provekit_materialized`, missing `params` becomes
    /// `[]`, missing `return_type` becomes `"void"`).
    pub fn parse(payload: &str) -> Result<Self, String> {
        let raw_payload = payload.to_string();
        let value: Json = serde_json::from_str(payload)
            .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
        let object = value
            .as_object()
            .ok_or_else(|| "provekit-concept payload must be a JSON object".to_string())?;

        let concept_name = object
            .get("concept_name")
            .or_else(|| object.get("conceptName"))
            .and_then(Json::as_str)
            .ok_or_else(|| "provekit-concept payload missing concept_name".to_string())?
            .to_string();

        let function = object
            .get("function")
            .and_then(Json::as_str)
            .unwrap_or("provekit_materialized")
            .to_string();

        let params = json_string_array_field(object.get("params"))
            .map_err(|error| format!("provekit-concept payload `params`: {error}"))?;
        let param_types = json_string_array_field(
            object.get("param_types").or_else(|| object.get("paramTypes")),
        )
        .map_err(|error| format!("provekit-concept payload `param_types`: {error}"))?;

        let return_type = object
            .get("return_type")
            .or_else(|| object.get("returnType"))
            .and_then(Json::as_str)
            .unwrap_or("void")
            .to_string();

        let library_tag = object
            .get("library_tag")
            .or_else(|| object.get("libraryTag"))
            .or_else(|| object.get("library"))
            .and_then(Json::as_str)
            .map(str::to_string);

        Ok(Self {
            concept_name,
            function,
            params,
            param_types,
            return_type,
            library_tag,
            raw_payload,
        })
    }
}

fn json_string_array_field(value: Option<&Json>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| "field must be a JSON array".to_string())?;
    array
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "array entries must be strings".to_string())
        })
        .collect()
}

/// A kit that, given a concept-citation carrier, produces a site outcome
/// (`Materialize`, `LoudlyLossy`, or `Refuse`). Both `provekit materialize`
/// and `provekit bind migrate` implement this; the site-iteration, splice,
/// and write-back machinery is shared by both via `transform_source_text`.
pub trait SiteTransformKit: Send + Sync {
    /// The target source language this kit emits (e.g., `"rust"`,
    /// `"python"`). Used by callers that key per-language defaults off the
    /// kit (e.g., file-extension filters in the file walker).
    fn target_language(&self) -> &str;

    /// Given a parsed carrier, dispatch to the kit's binding lookup and
    /// produce a site outcome. The kit is responsible for any RPC,
    /// path-composition, or binding-resolution it performs internally.
    fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String>;

    /// After a pass of site transformations, return new carriers for sites
    /// that effect propagation discovered. Default is a no-op for kits
    /// (like `MaterializeKit`) that change no effect signatures.
    ///
    /// The effect vocabulary is language-specific and lives inside the kit
    /// impl. A migrate kit reads the post-pass outcomes (each carrying its
    /// own EffectSetMemento via the kit's internal state) and returns
    /// `CarrierComment`s describing additional sites whose containing
    /// functions need a follow-up rewrite once an effect (sync->async,
    /// error-union widening, borrow->owned, panic propagation,
    /// allocator-implicit->allocator-explicit, etc.) propagated through
    /// the call graph. `transform_source_text` re-iterates as long as
    /// `propagate` returns a non-empty vector, capped at 32 iterations.
    fn propagate(
        &self,
        _outcomes: &[(ConceptSite, SiteOutcome)],
    ) -> Result<Vec<CarrierComment>, String> {
        Ok(Vec::new())
    }
}

/// A captured concept-citation site in a source file: the carrier plus the
/// (optional) stub function block that followed it. Returned by the shared
/// site-iteration loop for callers that want to introspect or aggregate
/// outcomes before write-back. Held by `transform_source_text` internally;
/// surfaced so future phases (the migrate receipt envelope, debug tooling)
/// can consume the same captured shape.
#[derive(Debug, Clone)]
pub struct ConceptSite {
    pub carrier: CarrierComment,
    pub indent: String,
    pub stub_line_count: usize,
    pub stub_signature_and_close: Option<StubSignatureAndCloseClone>,
    /// Inclusive starting line and exclusive ending line of the
    /// carrier-plus-stub region in the source, as `split_inclusive('\n')`
    /// indices. Used by callers that want to slice the source by line
    /// range.
    pub line_start: usize,
    pub line_end_exclusive: usize,
}

/// Clone-friendly mirror of `StubSignatureAndClose`. The Phase A struct is
/// owned by the capture step and consumed by the splice step in a single
/// loop iteration; `ConceptSite` (Phase B) is meant to be cheap to clone
/// for aggregation, so we carry the owned-string form.
#[derive(Debug, Clone)]
pub struct StubSignatureAndCloseClone {
    pub signature_text: String,
    pub close_indent: String,
}

impl From<&StubSignatureAndClose> for StubSignatureAndCloseClone {
    fn from(value: &StubSignatureAndClose) -> Self {
        Self {
            signature_text: value.signature_text.clone(),
            close_indent: value.close_indent.clone(),
        }
    }
}

/// Transform a source-file string by walking concept-citation carriers and
/// replacing each carrier-plus-stub region with the kit's site outcome.
/// Returns the rewritten source and the per-site outcomes in the order they
/// appeared.
///
/// Refusals propagate as `Err(String)` carrying the refusal `reason`; the
/// function does not continue past a site that returned `Refuse`, and does
/// not continue past a site whose `transform_site` itself errored.  Phase D
/// (`cmd_bind_migrate`) needs to accumulate refusals into a structured
/// receipt envelope; that flow will sit above `transform_source_text` and
/// catch the propagated `Err` (or, in Phase E, switch to an accumulating
/// variant). For Phase B's narrow surface, the propagate-on-refuse shape
/// mirrors Phase A's `materialize_source_text` (which surfaces realize
/// failures as `Err`).
///
/// The kit's returned `body` is the realize plugin's full
/// `fn ... { ... }` source declaration (the same shape the realize
/// transport returns). When a stub was captured, the inner body is
/// extracted from that source and spliced into the consumer's stub
/// signature via `splice_realized_body_into_stub_signature`, preserving
/// the consumer's signed signature exactly while filling its body from the
/// kit binding. When no stub was captured, the kit's source is emitted as
/// the materialized function, matching the Phase A fallback path. The
/// caller can then re-indent and append as before.
pub fn transform_source_text(
    source: &str,
    kit: &dyn SiteTransformKit,
) -> Result<(String, Vec<SiteOutcome>), String> {
    // Fixed-point loop: run one pass of site transformations, call the kit's
    // `propagate` hook to discover any additional carriers, append them to
    // the source (as fresh carrier comments the next pass will pick up), and
    // re-iterate until `propagate` returns empty. Cap at 32 iterations to
    // prevent divergent fixed points; emit `Err` if the cap is reached. A
    // kit whose `propagate` is the trait-default no-op exits the loop after
    // the first pass.
    const PROPAGATE_PASS_CAP: usize = 32;

    let mut current = source.to_string();
    let mut all_outcomes: Vec<SiteOutcome> = Vec::new();

    for pass in 0..PROPAGATE_PASS_CAP {
        let (rewritten, sites_and_outcomes) = transform_source_text_one_pass(&current, kit)?;
        let pass_outcomes: Vec<SiteOutcome> = sites_and_outcomes
            .iter()
            .map(|(_, outcome)| outcome.clone())
            .collect();

        let new_carriers = kit.propagate(&sites_and_outcomes)?;

        all_outcomes.extend(pass_outcomes);
        current = rewritten;

        if new_carriers.is_empty() {
            return Ok((current, all_outcomes));
        }

        // Append new carriers as fresh `provekit-concept:` comment lines
        // for the next pass to pick up. The kit decides where in the source
        // these belong by virtue of what it puts in the CarrierComment; the
        // shared loop simply re-runs the site walk over the rewritten
        // source plus the appended carriers. The minimal shape: one carrier
        // per line, no stub-function body, so `capture_stub_function_block`
        // returns 0 and the kit's `transform_site` realization is emitted
        // verbatim.
        if !current.ends_with('\n') {
            current.push('\n');
        }
        for carrier in new_carriers {
            current.push_str("// provekit-concept: ");
            current.push_str(&carrier.raw_payload);
            current.push('\n');
        }

        let _ = pass; // suppress unused warning under non-debug builds
    }

    Err(format!(
        "transform_source_text: effect-propagation fixed point did not converge within {PROPAGATE_PASS_CAP} passes"
    ))
}

/// One pass of the site-transform walk. Returns the rewritten source plus
/// the captured `ConceptSite` paired with each site's outcome. Used by
/// `transform_source_text` to drive the propagate-and-re-iterate loop;
/// surfaced as a private helper so the fixed-point machinery can hand the
/// kit a structured view of what just happened.
fn transform_source_text_one_pass(
    source: &str,
    kit: &dyn SiteTransformKit,
) -> Result<(String, Vec<(ConceptSite, SiteOutcome)>), String> {
    let mut out = String::new();
    let mut sites_and_outcomes: Vec<(ConceptSite, SiteOutcome)> = Vec::new();
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        if let Some((indent, payload)) = concept_payload_from_line(line) {
            let line_start = idx;
            let mut consumed = 1usize;
            if idx + consumed < lines.len()
                && concept_payload_cid_from_line(lines[idx + consumed]).is_some()
            {
                let declared_cid = concept_payload_cid_from_line(lines[idx + consumed]).unwrap();
                verify_payload_cid(payload, declared_cid)?;
                consumed += 1;
            }
            let stub_block = capture_stub_function_block(&lines[idx + consumed..]);
            let stub_line_count = stub_block.line_count;
            let stub_signature_and_close = stub_block
                .signature_and_close
                .as_ref()
                .map(StubSignatureAndCloseClone::from);
            consumed += stub_line_count;

            let carrier = CarrierComment::parse(payload)?;
            let outcome = kit.transform_site(&carrier)?;

            let body_opt: Option<&str> = match &outcome {
                SiteOutcome::Materialize { body, .. } => Some(body.as_str()),
                SiteOutcome::LoudlyLossy { body, .. } => Some(body.as_str()),
                SiteOutcome::Refuse { reason, .. } => {
                    return Err(reason.clone());
                }
            };

            if let Some(body) = body_opt {
                let emitted = if let Some(stub) = stub_block.signature_and_close.as_ref() {
                    splice_realized_body_into_stub_signature(stub, body)
                } else {
                    body.to_string()
                };
                let indented = indent_realized_source(&emitted, indent);
                out.push_str(&indented);
                if !indented.ends_with('\n') {
                    out.push('\n');
                }
            }
            let line_end_exclusive = line_start + consumed;
            let site = ConceptSite {
                carrier,
                indent: indent.to_string(),
                stub_line_count,
                stub_signature_and_close,
                line_start,
                line_end_exclusive,
            };
            sites_and_outcomes.push((site, outcome));
            idx += consumed;
            continue;
        }
        out.push_str(line);
        idx += 1;
    }
    Ok((out, sites_and_outcomes))
}

pub fn realize_spec_from_payload(payload: &str) -> Result<Json, String> {
    let mut value: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "provekit-concept payload must be a JSON object".to_string())?;
    let concept_name = object
        .get("concept_name")
        .or_else(|| object.get("conceptName"))
        .and_then(Json::as_str)
        .ok_or_else(|| "provekit-concept payload missing concept_name".to_string())?
        .to_string();
    object.insert(
        "kind".to_string(),
        Json::String("RealizeRequest".to_string()),
    );
    object.insert("concept_name".to_string(), Json::String(concept_name));
    if !object.contains_key("function") {
        object.insert(
            "function".to_string(),
            Json::String("provekit_materialized".to_string()),
        );
    }
    if !object.contains_key("params") {
        object.insert("params".to_string(), Json::Array(Vec::new()));
    }
    if !object.contains_key("param_types") {
        if let Some(param_types) = object.remove("paramTypes") {
            object.insert("param_types".to_string(), param_types);
        } else {
            object.insert("param_types".to_string(), Json::Array(Vec::new()));
        }
    }
    if !object.contains_key("return_type") {
        if let Some(return_type) = object.remove("returnType") {
            object.insert("return_type".to_string(), return_type);
        } else {
            object.insert("return_type".to_string(), Json::String("void".to_string()));
        }
    }
    if !object.contains_key("named_term_tree") {
        if let Some(named_term_tree) = object.remove("namedTermTree") {
            object.insert("named_term_tree".to_string(), named_term_tree);
        }
    }
    Ok(value)
}

#[cfg(test)]
mod phase_b_tests {
    use super::*;
    use serde_json::json;

    struct MockMaterializeKit {
        body: String,
        binding_cid: String,
    }

    impl SiteTransformKit for MockMaterializeKit {
        fn target_language(&self) -> &str {
            "rust"
        }

        fn transform_site(&self, _carrier: &CarrierComment) -> Result<SiteOutcome, String> {
            Ok(SiteOutcome::Materialize {
                body: self.body.clone(),
                binding_cid: self.binding_cid.clone(),
                loss_record: json!([]),
            })
        }
    }

    struct RefusingKit {
        reason: String,
    }

    impl SiteTransformKit for RefusingKit {
        fn target_language(&self) -> &str {
            "rust"
        }

        fn transform_site(&self, _carrier: &CarrierComment) -> Result<SiteOutcome, String> {
            Ok(SiteOutcome::Refuse {
                reason: self.reason.clone(),
                would_close_with_concept: "concept:demo".to_string(),
            })
        }
    }

    const SAMPLE_SOURCE: &str = "fn before() {}\n\
// provekit-concept: {\"concept_name\":\"concept:demo\",\"function\":\"do_thing\",\"params\":[\"x\"],\"param_types\":[\"u32\"],\"return_type\":\"u32\"}\n\
fn do_thing(x: u32) -> u32 {\n\
    unimplemented!()\n\
}\n\
fn after() {}\n";

    #[test]
    fn carrier_comment_parse_extracts_typed_fields() {
        let payload = r#"{"concept_name":"concept:demo","function":"do_thing","params":["x"],"param_types":["u32"],"return_type":"u32","library_tag":"std"}"#;
        let carrier = CarrierComment::parse(payload).expect("parses");
        assert_eq!(carrier.concept_name, "concept:demo");
        assert_eq!(carrier.function, "do_thing");
        assert_eq!(carrier.params, vec!["x".to_string()]);
        assert_eq!(carrier.param_types, vec!["u32".to_string()]);
        assert_eq!(carrier.return_type, "u32");
        assert_eq!(carrier.library_tag.as_deref(), Some("std"));
        assert_eq!(carrier.raw_payload, payload);
    }

    #[test]
    fn transform_source_text_splices_materialize_body_into_stub_signature() {
        // Kit returns the realize plugin's full `fn ... { ... }` source; the
        // stub-splice path extracts the inner body and wraps it in the
        // consumer's signed signature. Same shape the realize transport
        // returns and that `MaterializeKit` forwards.
        let kit = MockMaterializeKit {
            body: "fn realized(x: u32) -> u32 {\n    x.wrapping_add(1)\n}".to_string(),
            binding_cid: "cid:mock".to_string(),
        };
        let (out, outcomes) =
            transform_source_text(SAMPLE_SOURCE, &kit).expect("transform succeeds");
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(outcomes[0], SiteOutcome::Materialize { .. }));
        // Carrier line is consumed; consumer signature is preserved; body is spliced.
        assert!(out.contains("fn before() {}"));
        assert!(out.contains("fn do_thing(x: u32) -> u32 {"));
        assert!(out.contains("x.wrapping_add(1)"));
        assert!(out.contains("fn after() {}"));
        assert!(!out.contains("provekit-concept:"));
        assert!(!out.contains("unimplemented!()"));
    }

    #[test]
    fn transform_source_text_propagates_refusal_as_err() {
        let kit = RefusingKit {
            reason: "no binding for concept:demo".to_string(),
        };
        let error =
            transform_source_text(SAMPLE_SOURCE, &kit).expect_err("refusal propagates as Err");
        assert!(error.contains("no binding for concept:demo"));
    }

    /// A kit whose `propagate` hook fires once with a single fresh carrier,
    /// then returns empty. Exercises the fixed-point loop: pass 1 picks up
    /// the seed carrier, pass 2 picks up the propagate-emitted carrier,
    /// pass 3 sees nothing new and terminates.
    struct PropagatingKit {
        propagate_call_count: std::sync::atomic::AtomicUsize,
    }

    impl SiteTransformKit for PropagatingKit {
        fn target_language(&self) -> &str {
            "rust"
        }

        fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
            Ok(SiteOutcome::Materialize {
                body: format!(
                    "fn {}() {{\n    /* realized {} */\n}}",
                    carrier.function, carrier.concept_name
                ),
                binding_cid: "cid:mock".to_string(),
                loss_record: json!([]),
            })
        }

        fn propagate(
            &self,
            _outcomes: &[(ConceptSite, SiteOutcome)],
        ) -> Result<Vec<CarrierComment>, String> {
            let count = self
                .propagate_call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                let payload = r#"{"concept_name":"concept:propagated","function":"propagated_fn","params":[],"param_types":[],"return_type":"void"}"#;
                let carrier = CarrierComment::parse(payload).unwrap();
                Ok(vec![carrier])
            } else {
                Ok(Vec::new())
            }
        }
    }

    #[test]
    fn transform_source_text_fixed_point_runs_propagate_until_empty() {
        let kit = PropagatingKit {
            propagate_call_count: std::sync::atomic::AtomicUsize::new(0),
        };
        let (out, outcomes) =
            transform_source_text(SAMPLE_SOURCE, &kit).expect("fixed point converges");
        // Two outcomes: the seed carrier in SAMPLE_SOURCE + the propagated one.
        assert_eq!(outcomes.len(), 2);
        // Both should be Materialize variants.
        for outcome in &outcomes {
            assert!(matches!(outcome, SiteOutcome::Materialize { .. }));
        }
        // The propagated carrier's realization is appended to the rewritten source.
        assert!(out.contains("propagated_fn"));
        assert!(out.contains("concept:propagated"));
        // The kit's propagate was called twice (once returning a carrier, once empty).
        assert_eq!(
            kit.propagate_call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    /// A divergent kit that never returns empty from `propagate`; verifies
    /// the cap-at-32 safety net.
    struct DivergentKit;

    impl SiteTransformKit for DivergentKit {
        fn target_language(&self) -> &str {
            "rust"
        }

        fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
            Ok(SiteOutcome::Materialize {
                body: format!("fn {}() {{}}", carrier.function),
                binding_cid: "cid:divergent".to_string(),
                loss_record: json!([]),
            })
        }

        fn propagate(
            &self,
            _outcomes: &[(ConceptSite, SiteOutcome)],
        ) -> Result<Vec<CarrierComment>, String> {
            let payload = r#"{"concept_name":"concept:never-ends","function":"loop_fn","params":[],"param_types":[],"return_type":"void"}"#;
            Ok(vec![CarrierComment::parse(payload).unwrap()])
        }
    }

    #[test]
    fn transform_source_text_fixed_point_caps_divergent_propagation() {
        let kit = DivergentKit;
        let error = transform_source_text(SAMPLE_SOURCE, &kit)
            .expect_err("divergent propagation hits the cap");
        assert!(error.contains("fixed point did not converge"));
    }
}

