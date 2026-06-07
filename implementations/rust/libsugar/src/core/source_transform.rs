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

use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
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
    const KEYWORDS_TO_STRIP: &[&str] =
        &["pub ", "async ", "const ", "unsafe ", "extern ", "default "];
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
        contract_cid: Option<String>,
        loss_record: Json,
    },

    /// Site cleared with declared bounded loss. `body` is the spliced
    /// function body; `binding_cid` pins the kit binding; `declared_loss`
    /// is the list of dimensions where the realization is bounded-lossy.
    LoudlyLossy {
        body: String,
        binding_cid: String,
        contract_cid: Option<String>,
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
    /// The enclosing function or module the carrier-cited site lives in.
    /// Distinct from `function` (which is the carrier's own concept-bound
    /// function name): `containing_function` names the SCOPE the site sits
    /// inside, so effect-propagation can identify callers when a site's
    /// effect signature widens or narrows. Populated by
    /// `transform_source_text_one_pass` when scanning carriers; absent when
    /// the carrier sits at module scope (no enclosing fn) or when the
    /// payload itself carries no scope hint (Phase E `#1339`).
    pub containing_function: Option<String>,
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
            object
                .get("param_types")
                .or_else(|| object.get("paramTypes")),
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

        // `containing_function` is OPTIONAL in the JSON payload. Existing
        // carriers that pre-date Phase E (`#1339`) do not declare it, so
        // parsing must default to `None` rather than fail. The site walk
        // populates it by source-scan when the JSON didn't (most cases).
        let containing_function = object
            .get("containing_function")
            .or_else(|| object.get("containingFunction"))
            .and_then(Json::as_str)
            .map(str::to_string);

        Ok(Self {
            concept_name,
            function,
            params,
            param_types,
            return_type,
            library_tag,
            containing_function,
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

            let mut carrier = CarrierComment::parse(payload)?;
            // Phase E (`#1339`): populate `containing_function` by source-
            // scan if the payload did not declare it. Scan upward from the
            // carrier line for the nearest enclosing `fn ` declaration whose
            // open brace has not yet closed by the carrier's line. The scan
            // is conservative (matches Rust-shaped declarations); kits whose
            // target language uses other declaration shapes can override by
            // setting `containing_function` in the JSON payload.
            if carrier.containing_function.is_none() {
                carrier.containing_function = enclosing_function_name(&lines, idx);
            }
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

/// Scan upward from `carrier_idx` looking for the nearest enclosing `fn `
/// declaration whose open brace has not yet closed by the carrier line.
/// Returns the function name (the identifier after `fn ` and before `(` or
/// `<` or whitespace) when found.
///
/// The scan is conservative and shape-matches Rust-style declarations.
/// Other languages (Python `def`, TypeScript `function`, Java methods) are
/// handled by the kit setting `containing_function` directly in the JSON
/// payload before parsing. Phase E (`#1339`) uses this for the migrate kit's
/// effect-propagation graph, which is exercised today only on Rust source.
fn enclosing_function_name(lines: &[&str], carrier_idx: usize) -> Option<String> {
    // Walk upward from the line above the carrier. Track brace depth so we
    // skip past sibling blocks (e.g., earlier `fn` blocks that already
    // closed). When depth is non-negative and we hit a line that starts a
    // function declaration, return that fn's name.
    let mut depth: i32 = 0;
    let mut idx = carrier_idx;
    while idx > 0 {
        idx -= 1;
        let line = lines[idx];
        for ch in line.chars().rev() {
            match ch {
                '}' => depth += 1,
                '{' => {
                    depth -= 1;
                    if depth < 0 {
                        // Entered the enclosing block. Look at this line
                        // (and earlier lines if the declaration spans) for
                        // a `fn <name>` shape.
                        if let Some(name) = function_name_from_declaration_line(line) {
                            return Some(name);
                        }
                        // Declaration may sit on the line above (the `{`
                        // landed on a separate line). Walk back one more
                        // and try there.
                        if idx > 0 {
                            if let Some(name) = function_name_from_declaration_line(lines[idx - 1])
                            {
                                return Some(name);
                            }
                        }
                        return None;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract the function name from a Rust-shaped `fn <name>(` line, stripping
/// modifiers (`pub`, `async`, `unsafe`, etc.). Returns `None` if the line
/// does not start a function declaration.
fn function_name_from_declaration_line(line: &str) -> Option<String> {
    if !line_starts_function_declaration(line) {
        return None;
    }
    let trimmed = line.trim_start();
    const KEYWORDS_TO_STRIP: &[&str] =
        &["pub ", "async ", "const ", "unsafe ", "extern ", "default "];
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
    let after_fn = remaining.strip_prefix("fn ")?.trim_start();
    let end = after_fn
        .find(|c: char| c == '(' || c == '<' || c == ' ' || c == '\t' || c == '\n')
        .unwrap_or(after_fn.len());
    let name = &after_fn[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// -----------------------------------------------------------------------------
// Phase E (`#1339`): unified SourceTransformReceipt.
//
// Both `provekit materialize` (N=1) and `provekit bind migrate` (N=2) end a
// run with a structured per-site receipt: each site cleared as `Exact`,
// `LoudlyLossy`, or `Refused`. Phase E exposes the shared shape so:
//   1. Materialize stops aborting on first refusal (now first-class in
//      `refusal_mementos`) and emits a receipt the CLI can print.
//   2. Migrate's existing `MigrateReceiptEnvelope` (which carries language-
//      transition + propagation-decision + witness machinery) is preserved
//      byte-identically for the run_inner path; the per-site outcomes
//      collected through `SiteTransformKit` are exposed via this shape so
//      downstream consumers see the same trichotomy structure regardless
//      of which CLI emitted it.
//
// Per the umbrella plan, language-specific effect-set details land in each
// `loss_records` entry's `dimensions` array; the field shape is language-
// agnostic.

/// Per-run audit of every concept-citation site a source transformation
/// touched. The trichotomy is preserved at field level: exact realizations
/// flow into `aggregate_summary.exact`, declared-lossy realizations into
/// `aggregate_summary.lossy` + `loss_records`, refusals into
/// `aggregate_summary.refused` + `refusal_mementos`. The `site_witnesses`
/// list pins one entry per site in source order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceTransformReceipt {
    /// Schema version for the receipt envelope. Bumped when field shape
    /// changes in a way that breaks downstream consumers' parsers.
    pub schema_version: String,
    pub source_language: String,
    /// `None` for materialize (which has no source binding to compare
    /// against); `Some` for migrate (N=2 specialization).
    pub source_library: Option<String>,
    pub target_language: String,
    pub target_library: String,
    pub aggregate_summary: AggregateSummary,
    pub site_witnesses: Vec<SiteWitness>,
    pub loss_records: Vec<LossRecord>,
    pub refusal_mementos: Vec<RefusalMemento>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AggregateSummary {
    pub exact: usize,
    pub lossy: usize,
    pub refused: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SiteWitness {
    pub source_binding_cid: Option<String>,
    pub target_binding_cid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_cid: Option<String>,
    pub concept_name: String,
    pub function_name: String,
    pub outcome_kind: OutcomeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutcomeKind {
    Exact,
    LoudlyLossy,
    Refused,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LossRecord {
    pub concept_name: String,
    pub dimensions: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefusalMemento {
    pub concept_name: String,
    pub reason: String,
    pub would_close_with_concept: String,
}

/// Assemble a `SourceTransformReceipt` from a kit and its per-site outcomes.
/// The kit names the target language; the caller supplies the source-side
/// labels (materialize passes `source_lib: None`; migrate passes both).
///
/// `site_outcomes` is the `(ConceptSite, SiteOutcome)` pair list returned
/// by a `transform_source_text_one_pass` walk OR an accumulated list across
/// fixed-point passes. The receipt mirrors the order it sees.
pub fn build_receipt(
    kit: &dyn SiteTransformKit,
    source_lang: &str,
    source_lib: Option<&str>,
    target_lib: &str,
    site_outcomes: &[(ConceptSite, SiteOutcome)],
) -> SourceTransformReceipt {
    let mut aggregate_summary = AggregateSummary::default();
    let mut site_witnesses = Vec::with_capacity(site_outcomes.len());
    let mut loss_records = Vec::new();
    let mut refusal_mementos = Vec::new();
    for (site, outcome) in site_outcomes {
        let concept_name = site.carrier.concept_name.clone();
        let function_name = site
            .carrier
            .containing_function
            .clone()
            .unwrap_or_else(|| site.carrier.function.clone());
        match outcome {
            SiteOutcome::Materialize {
                binding_cid,
                contract_cid,
                ..
            } => {
                aggregate_summary.exact += 1;
                site_witnesses.push(SiteWitness {
                    source_binding_cid: source_lib.map(|_| String::new()),
                    target_binding_cid: binding_cid.clone(),
                    contract_cid: contract_cid.clone(),
                    concept_name,
                    function_name,
                    outcome_kind: OutcomeKind::Exact,
                });
            }
            SiteOutcome::LoudlyLossy {
                binding_cid,
                contract_cid,
                declared_loss,
                ..
            } => {
                aggregate_summary.lossy += 1;
                loss_records.push(LossRecord {
                    concept_name: concept_name.clone(),
                    dimensions: declared_loss.clone(),
                });
                site_witnesses.push(SiteWitness {
                    source_binding_cid: source_lib.map(|_| String::new()),
                    target_binding_cid: binding_cid.clone(),
                    contract_cid: contract_cid.clone(),
                    concept_name,
                    function_name,
                    outcome_kind: OutcomeKind::LoudlyLossy,
                });
            }
            SiteOutcome::Refuse {
                reason,
                would_close_with_concept,
            } => {
                aggregate_summary.refused += 1;
                refusal_mementos.push(RefusalMemento {
                    concept_name: concept_name.clone(),
                    reason: reason.clone(),
                    would_close_with_concept: would_close_with_concept.clone(),
                });
                site_witnesses.push(SiteWitness {
                    source_binding_cid: source_lib.map(|_| String::new()),
                    target_binding_cid: String::new(),
                    contract_cid: None,
                    concept_name,
                    function_name,
                    outcome_kind: OutcomeKind::Refused,
                });
            }
        }
    }
    SourceTransformReceipt {
        schema_version: "1".to_string(),
        source_language: source_lang.to_string(),
        source_library: source_lib.map(str::to_string),
        target_language: kit.target_language().to_string(),
        target_library: target_lib.to_string(),
        aggregate_summary,
        site_witnesses,
        loss_records,
        refusal_mementos,
    }
}

/// Variant of `transform_source_text` that does NOT abort on
/// `SiteOutcome::Refuse`; refusals are collected into the per-site outcome
/// list and become first-class entries in a `SourceTransformReceipt`
/// (Phase E `#1339`). The fixed-point propagate loop is preserved; the only
/// difference from `transform_source_text` is that the refuse leg no longer
/// shortcuts to `Err`. The substrate honesty contract (trichotomy)
/// holds: a refused site **leaves the carrier comment + stub function intact
/// in the rewritten source** so another library's materialize pass can pick
/// the boundary up (substrate gap #84: multi-library, multi-file consumers
/// need refused = no-op on the source, not destructive). Downstream tooling
/// reads the receipt to learn which refusals landed.
pub fn transform_source_text_collecting_refusals(
    source: &str,
    kit: &dyn SiteTransformKit,
) -> Result<(String, Vec<(ConceptSite, SiteOutcome)>), String> {
    const PROPAGATE_PASS_CAP: usize = 32;

    // Pre-pass: synthesize `// provekit-concept:` carriers from any
    // `#[provekit::boundary(...)]` attribute call-sites in the source.
    // The boundary primitive is the substrate's newer source-side
    // annotation for "this is where a per-target library realization
    // gets substituted"; this pre-pass bridges it to the line-based
    // carrier walker below, so the same downstream realize-dispatch
    // pipeline that already handles `provekit-concept:` carriers
    // processes @boundary-tagged sources unchanged.
    let mut current = inject_boundary_carriers(source);
    let mut all: Vec<(ConceptSite, SiteOutcome)> = Vec::new();

    for _pass in 0..PROPAGATE_PASS_CAP {
        let (rewritten, sites_and_outcomes) =
            transform_source_text_one_pass_collecting_refusals(&current, kit)?;
        let new_carriers = kit.propagate(&sites_and_outcomes)?;

        all.extend(sites_and_outcomes);
        current = rewritten;

        if new_carriers.is_empty() {
            return Ok((current, all));
        }

        if !current.ends_with('\n') {
            current.push('\n');
        }
        for carrier in new_carriers {
            current.push_str("// provekit-concept: ");
            current.push_str(&carrier.raw_payload);
            current.push('\n');
        }
    }

    Err(format!(
        "transform_source_text_collecting_refusals: effect-propagation fixed point did not converge within {PROPAGATE_PASS_CAP} passes"
    ))
}

/// Single-pass variant that collects refusals rather than aborting. Shares
/// the splice/indent machinery with `transform_source_text_one_pass`; the
/// only behavioral fork is the refuse leg, which here emits the carrier
/// comment + stub region verbatim into the output (so a different library
/// pass can pick the refused boundary up) and pushes the Refuse outcome
/// onto the returned list. Substrate gap #84: multi-library, multi-file
/// consumers need refused = no-op on source, not destructive drop.
fn transform_source_text_one_pass_collecting_refusals(
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

            let mut carrier = CarrierComment::parse(payload)?;
            if carrier.containing_function.is_none() {
                carrier.containing_function = enclosing_function_name(&lines, idx);
            }
            let outcome = kit.transform_site(&carrier)?;

            match &outcome {
                SiteOutcome::Materialize { body, .. } | SiteOutcome::LoudlyLossy { body, .. } => {
                    let emitted = if let Some(stub) = stub_block.signature_and_close.as_ref() {
                        splice_realized_body_into_stub_signature(stub, body.as_str())
                    } else {
                        body.to_string()
                    };
                    let indented = indent_realized_source(&emitted, indent);
                    out.push_str(&indented);
                    if !indented.ends_with('\n') {
                        out.push('\n');
                    }
                }
                SiteOutcome::Refuse { .. } => {
                    // Substrate gap #84: leave the carrier comment + stub
                    // intact verbatim so another library's materialize pass
                    // (or a --family-library route to a different vendor)
                    // can pick this boundary up. The Refuse outcome still
                    // lands in the receipt so the user can see what was
                    // not filled, but the source file is untouched at this
                    // site — multi-library and multi-vendor consumers
                    // depend on this no-op invariant.
                    for original_line in &lines[line_start..(line_start + consumed)] {
                        out.push_str(original_line);
                    }
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

// ---------------------------------------------------------------------
// Boundary-carrier injection: bridge from `#[provekit::boundary(...)]`
// attribute call-sites to `// provekit-concept:` line carriers so the
// existing line-based walker can process @boundary-tagged sources.
// Substrate-honest: the boundary primitive is a newer source-side
// annotation; this pre-pass translates it into the carrier form the
// realize-dispatch pipeline already consumes. Idempotent and additive
// (sources without `#[provekit::boundary]` pass through unchanged).
// ---------------------------------------------------------------------

/// Synthesize `// provekit-concept:` carriers from
/// `#[provekit::boundary(...)]` attributes in `source`. For each
/// matched boundary attribute, inserts a carrier comment on the line
/// immediately before the attribute, with the carrier payload built
/// from (attribute fields, function signature). Lines without
/// `#[provekit::boundary]` pass through unchanged.
pub fn inject_boundary_carriers(source: &str) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let mut out = String::with_capacity(source.len() + 1024);
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx];
        if let Some(indent) = boundary_attr_start_indent(line) {
            // Capture the (possibly multi-line) attribute block until
            // the line containing `)]`.
            let mut close_idx = idx;
            while close_idx < lines.len() && !lines[close_idx].contains(")]") {
                close_idx += 1;
            }
            if close_idx >= lines.len() {
                // Unterminated attribute: emit verbatim and continue.
                out.push_str(line);
                idx += 1;
                continue;
            }
            let attr_text: String = lines[idx..=close_idx].concat();
            // Look ahead to the function declaration following the attr.
            // Skip blank lines and other attributes that may sit between
            // `#[provekit::boundary]` and `fn`.
            let mut probe = close_idx + 1;
            while probe < lines.len() {
                let trimmed = lines[probe].trim();
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#[") {
                    probe += 1;
                    continue;
                }
                break;
            }
            if probe < lines.len() && line_starts_fn(lines[probe]) {
                // Collect signature lines up through the opening `{`.
                let mut sig_end = probe;
                while sig_end < lines.len() && !lines[sig_end].contains('{') {
                    sig_end += 1;
                }
                let sig_end = sig_end.min(lines.len() - 1);
                let sig_text: String = lines[probe..=sig_end].concat();
                if let Some(payload) = build_boundary_carrier_payload(&attr_text, &sig_text) {
                    out.push_str(indent);
                    out.push_str("// provekit-concept: ");
                    out.push_str(&payload);
                    out.push('\n');
                    // Skip the #[provekit::boundary(...)] attribute lines —
                    // the carrier above carries the same information, and
                    // leaving the attribute in place breaks
                    // capture_stub_function_block (which expects the line
                    // immediately after the carrier to start with `fn` /
                    // `pub fn`, not an attribute).
                    idx = close_idx + 1;
                    continue;
                }
            }
            // No payload built (parser failed) — emit the attribute verbatim
            // so the source remains valid Rust.
            out.push_str(&attr_text);
            idx = close_idx + 1;
        } else {
            out.push_str(line);
            idx += 1;
        }
    }
    out
}

fn boundary_attr_start_indent(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("#[provekit::boundary(") || trimmed.starts_with("#[provekit::boundary]")
    {
        let leading_len = line.len() - trimmed.len();
        Some(&line[..leading_len])
    } else {
        None
    }
}

fn line_starts_fn(line: &str) -> bool {
    let mut remaining = line.trim_start();
    let prefixes = [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const ",
        "unsafe ",
        "extern \"C\" ",
        "extern ",
    ];
    let mut changed = true;
    while changed {
        changed = false;
        for prefix in prefixes {
            if let Some(rest) = remaining.strip_prefix(prefix) {
                remaining = rest;
                changed = true;
                break;
            }
        }
    }
    remaining.starts_with("fn ")
}

fn build_boundary_carrier_payload(attr_text: &str, sig_text: &str) -> Option<String> {
    let concept = extract_attr_string(attr_text, "concept")?;
    let library = extract_attr_string(attr_text, "library");
    let version = extract_attr_string(attr_text, "version");
    let family = extract_attr_string(attr_text, "family");
    let (fn_name, params, param_types, return_type) = parse_fn_signature(sig_text)?;
    let mut fields = Vec::with_capacity(10);
    fields.push(format!("\"concept_name\":\"{}\"", escape_json(&concept)));
    fields.push(format!("\"function\":\"{}\"", escape_json(&fn_name)));
    fields.push(format!("\"params\":{}", format_string_array(&params)));
    // SUBSTRATE-HONEST: param_types + return_type (raw kit-internal syntax)
    // are intentionally NOT emitted into the carrier payload. The carrier
    // crosses into substrate; the substrate's channel must not carry kit-
    // internal source strings — that's the same lie as body_text. Only
    // concept-hub identities cross. When the rust kit can't translate a
    // type to a concept-hub sort, it emits empty in param_sort_cids /
    // return_sort_cid — a substrate-honest gap signal that downstream
    // realize binaries refuse on, instead of being able to fall back on
    // raw rust syntax. Forces gap mints rather than hiding them.
    let mut parametric_sort_expansions: Vec<crate::core::lower_plugin::ParametricSortExpansion> =
        Vec::new();
    let param_sort_cids: Vec<String> = param_types
        .iter()
        .map(|t| {
            rust_source_type_to_concept_hub_sort_cid(t, &mut parametric_sort_expansions)
                .unwrap_or_default()
        })
        .collect();
    fields.push(format!(
        "\"param_sort_cids\":{}",
        format_string_array(&param_sort_cids)
    ));
    let return_sort_cid =
        rust_source_type_to_concept_hub_sort_cid(&return_type, &mut parametric_sort_expansions)
            .unwrap_or_default();
    fields.push(format!(
        "\"return_sort_cid\":\"{}\"",
        escape_json(&return_sort_cid)
    ));
    // #1369: emit parametric content-addressing expansions so realize
    // plugin can decompose composite CIDs into (constructor, args).
    if !parametric_sort_expansions.is_empty() {
        let expansions_json =
            serde_json::to_string(&parametric_sort_expansions).unwrap_or_else(|_| "[]".to_string());
        fields.push(format!(
            "\"parametric_sort_expansions\":{}",
            expansions_json
        ));
    }
    if let Some(lib) = library {
        fields.push(format!("\"library\":\"{}\"", escape_json(&lib)));
    }
    // #1357: thread version + family from the @boundary attribute through
    // the synthesized carrier comment so realize_spec_from_payload (and the
    // dispatch downstream in #1359) can see the full pinned tuple. Absent
    // on attribute → absent in carrier (substrate-honest floating).
    if let Some(v) = version {
        fields.push(format!("\"library_version\":\"{}\"", escape_json(&v)));
    }
    if let Some(f) = family {
        fields.push(format!("\"family\":\"{}\"", escape_json(&f)));
    }
    Some(format!("{{{}}}", fields.join(",")))
}

/// #1361 chunk 2 part B / #1355: rust-source-syntax → concept-hub sort CID.
///
/// The rust kit owns this translation. The carrier emission is the
/// kit/substrate boundary — beyond it, ONLY concept-hub identities
/// remain in the IR. Rust-internal sort labels (rust:Int, rust:Str)
/// stay inside the rust kit's catalog and morphism lookups; they
/// never appear in the carrier payload or in cmd_materialize.
///
/// Substrate-canonical sort CIDs (from menagerie/concept-shapes/catalog/sorts/):
///   concept:Int    → blake3-512:30ffc513...
///   concept:Float  → blake3-512:b979e70c...
///   concept:Bool   → blake3-512:0ee13bf3...
///   concept:String → blake3-512:be8721d2...
///   concept:Unit   → blake3-512: (look up if/when minted)
///   concept:Bytes  → blake3-512:7116ef6e...
///   concept:Null   → blake3-512:62f6040b...
///   concept:List<T> → blake3-512:e3f8d174...
///   concept:Map<K,V> → blake3-512:b81923e3...
// Substrate-canonical primitive sort CIDs.
/// Catalog-driven rust-source-syntax → concept-hub sort CID (#1370).
///
/// NO hardcoded source-token names. Reads kit-source-alias mementos via
/// crate::core::lower_plugin::load_kit_source_aliases("rust") and dispatches
/// via the recursive resolver in libsugar::core::lower_plugin.
fn rust_source_type_to_concept_hub_sort_cid(
    rust_type: &str,
    expansions: &mut Vec<crate::core::lower_plugin::ParametricSortExpansion>,
) -> Option<String> {
    use std::sync::OnceLock;
    static RUST_ALIASES: OnceLock<
        std::collections::BTreeMap<String, crate::core::lower_plugin::KitSourceAliasEntry>,
    > = OnceLock::new();
    let aliases =
        RUST_ALIASES.get_or_init(|| crate::core::lower_plugin::load_kit_source_aliases("rust"));
    crate::core::lower_plugin::rust_type_to_concept_hub_sort_cid(rust_type, aliases, expansions)
}

fn extract_attr_string(attr_text: &str, key: &str) -> Option<String> {
    let needle = format!("{} = \"", key);
    let start = attr_text.find(&needle)?;
    let value_start = start + needle.len();
    let end_rel = attr_text[value_start..].find('"')?;
    Some(attr_text[value_start..value_start + end_rel].to_string())
}

fn parse_fn_signature(sig: &str) -> Option<(String, Vec<String>, Vec<String>, String)> {
    let fn_pos = sig.find("fn ")?;
    let after_fn = &sig[fn_pos + 3..];
    // Extract the function name (up to `(` or `<`).
    let name_end = after_fn
        .find(|c: char| c == '(' || c == '<' || c.is_whitespace())
        .unwrap_or(after_fn.len());
    let fn_name = after_fn[..name_end].trim().to_string();
    if fn_name.is_empty() {
        return None;
    }
    // Find the opening `(` of the parameter list.
    let paren_open = after_fn[name_end..].find('(')? + name_end;
    // Find the matching closing `)`.
    let mut depth = 0;
    let mut close = None;
    for (i, c) in after_fn[paren_open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(paren_open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params_text = &after_fn[paren_open + 1..close];
    let (params, param_types) = split_params(params_text);
    let after_close = &after_fn[close + 1..];
    let return_type = if let Some(arrow) = after_close.find("->") {
        let after_arrow = &after_close[arrow + 2..];
        let end = after_arrow
            .find('{')
            .or_else(|| after_arrow.find("where"))
            .unwrap_or(after_arrow.len());
        after_arrow[..end].trim().to_string()
    } else {
        "()".to_string()
    };
    Some((fn_name, params, param_types, return_type))
}

fn split_params(text: &str) -> (Vec<String>, Vec<String>) {
    let mut params = Vec::new();
    let mut param_types = Vec::new();
    let mut depth_paren = 0i32;
    let mut depth_angle = 0i32;
    let mut depth_bracket = 0i32;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '(' => {
                depth_paren += 1;
                current.push(c);
            }
            ')' => {
                depth_paren -= 1;
                current.push(c);
            }
            '[' => {
                depth_bracket += 1;
                current.push(c);
            }
            ']' => {
                depth_bracket -= 1;
                current.push(c);
            }
            '<' => {
                depth_angle += 1;
                current.push(c);
            }
            '>' => {
                depth_angle -= 1;
                current.push(c);
            }
            ',' if depth_paren == 0 && depth_angle == 0 && depth_bracket == 0 => {
                if let Some((n, t)) = parse_param(&current) {
                    params.push(n);
                    param_types.push(t);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        if let Some((n, t)) = parse_param(&current) {
            params.push(n);
            param_types.push(t);
        }
    }
    (params, param_types)
}

fn parse_param(p: &str) -> Option<(String, String)> {
    let p = p.trim();
    if p.is_empty() || p == "self" || p.starts_with("&self") || p.starts_with("&mut self") {
        return None;
    }
    let colon = p.find(':')?;
    let raw_name = p[..colon].trim();
    let name = raw_name.trim_start_matches('_').trim().to_string();
    let ty = p[colon + 1..].trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some((name, ty))
}

fn format_string_array(items: &[String]) -> String {
    let mut s = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(&escape_json(item));
        s.push('"');
    }
    s.push(']');
    s
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod phase_b_tests {
    use super::*;
    use serde_json::json;

    // ---------------------------------------------------------------------
    // #1357 / #1355: family + version threaded through the carrier payload
    // ---------------------------------------------------------------------

    #[test]
    fn boundary_carrier_includes_family_and_version_when_present() {
        let source = r#"
#[provekit::boundary(
    concept = "concept:sql-query",
    library = "rusqlite",
    version = "0.39.0",
    family = "concept:family:sql",
    loss = [],
)]
pub fn query(_conn: &i64, _sql: &str) -> i64 {
    unimplemented!()
}
"#;
        let injected = inject_boundary_carriers(source);
        let carrier_line = injected
            .lines()
            .find(|l| l.trim_start().starts_with("// provekit-concept:"))
            .expect("carrier line synthesized");
        assert!(
            carrier_line.contains("\"library\":\"rusqlite\""),
            "library pin in carrier: {carrier_line}"
        );
        assert!(
            carrier_line.contains("\"library_version\":\"0.39.0\""),
            "version pin in carrier: {carrier_line}"
        );
        assert!(
            carrier_line.contains("\"family\":\"concept:family:sql\""),
            "family pin in carrier: {carrier_line}"
        );
        assert!(
            carrier_line.contains("\"concept_name\":\"concept:sql-query\""),
            "concept_name in carrier: {carrier_line}"
        );
    }

    // -----------------------------------------------------------------
    // #1361 chunk 2 part B / #1355: carrier emits concept-hub sort CIDs
    // for each parameter (rust kit's lift-to-substrate translation).
    // -----------------------------------------------------------------

    #[test]
    fn boundary_carrier_emits_concept_hub_sort_cids_for_params() {
        let source = r#"
#[provekit::boundary(
    concept = "concept:sql-query",
    library = "rusqlite",
)]
pub fn query(_conn: &i64, _sql: &str) -> i64 {
    unimplemented!()
}
"#;
        let injected = inject_boundary_carriers(source);
        let carrier_line = injected
            .lines()
            .find(|l| l.trim_start().starts_with("// provekit-concept:"))
            .expect("carrier line synthesized");
        // concept:Int CID for i64
        let int_cid =
            "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
        // concept:String CID for &str
        let string_cid =
            "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
        assert!(
            carrier_line.contains(int_cid),
            "param i64 should emit concept:Int CID in carrier: {carrier_line}"
        );
        assert!(
            carrier_line.contains(string_cid),
            "param &str should emit concept:String CID in carrier: {carrier_line}"
        );
        assert!(
            carrier_line.contains("\"return_sort_cid\":"),
            "return_sort_cid field present: {carrier_line}"
        );
        // CRITICAL: NO rust-kit-internal sort labels (rust:Int, rust:Str)
        // appear in the carrier — only concept-hub CIDs cross into substrate.
        assert!(
            !carrier_line.contains("\"rust:"),
            "kit-internal `rust:` labels MUST NOT appear in substrate-level carrier payload: {carrier_line}"
        );
    }

    #[test]
    fn boundary_carrier_emits_empty_sort_cid_for_unrecognized_type() {
        // For types the rust kit doesn't have a concept-hub morphism for
        // (e.g. a user-defined struct), emit empty string in param_sort_cids.
        // Substrate-honest: empty signals "no morphism yet, gap remains".
        let source = r#"
#[provekit::boundary(
    concept = "concept:custom",
    library = "myshim",
)]
pub fn op(_x: MyCustomType) -> i64 {
    unimplemented!()
}
"#;
        let injected = inject_boundary_carriers(source);
        let carrier_line = injected
            .lines()
            .find(|l| l.trim_start().starts_with("// provekit-concept:"))
            .expect("carrier line synthesized");
        // The custom type's slot in param_sort_cids should be "" (empty).
        assert!(
            carrier_line.contains("\"param_sort_cids\":[\"\"]"),
            "unrecognized type emits empty sort-cid (substrate-honest gap signal): {carrier_line}"
        );
    }

    #[test]
    fn boundary_carrier_omits_family_and_version_when_absent() {
        let source = r#"
#[provekit::boundary(
    concept = "concept:blake3-512-of",
    library = "blake3",
    loss = [],
)]
pub fn h(_bytes: &[u8]) -> i64 {
    unimplemented!()
}
"#;
        let injected = inject_boundary_carriers(source);
        let carrier_line = injected
            .lines()
            .find(|l| l.trim_start().starts_with("// provekit-concept:"))
            .expect("carrier line synthesized");
        // Substrate-honest: absent on attribute → absent in carrier.
        // NOT empty strings. Same convention as walk_rpc's binding-entry
        // emission (#1357 chunk 1).
        assert!(
            !carrier_line.contains("\"library_version\""),
            "library_version field absent when not pinned: {carrier_line}"
        );
        assert!(
            !carrier_line.contains("\"family\""),
            "family field absent when not pinned: {carrier_line}"
        );
    }

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
                contract_cid: None,
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
                contract_cid: None,
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
                contract_cid: None,
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

    // -----------------------------------------------------------------
    // Phase E (`#1339`) tests.
    // -----------------------------------------------------------------

    #[test]
    fn carrier_comment_parse_accepts_payload_without_containing_function() {
        // Existing payloads (pre-Phase-E) do not declare
        // `containing_function`. They must continue to parse with the
        // field defaulting to None.
        let payload = r#"{"concept_name":"concept:demo","function":"do_thing","params":[],"param_types":[],"return_type":"void"}"#;
        let carrier = CarrierComment::parse(payload).expect("parses without containing_function");
        assert_eq!(carrier.concept_name, "concept:demo");
        assert!(
            carrier.containing_function.is_none(),
            "containing_function must default to None when payload omits it"
        );
    }

    #[test]
    fn carrier_comment_parse_accepts_containing_function_field() {
        let payload = r#"{"concept_name":"concept:demo","function":"do_thing","containing_function":"outer_fn","params":[],"param_types":[],"return_type":"void"}"#;
        let carrier = CarrierComment::parse(payload).expect("parses with containing_function");
        assert_eq!(carrier.containing_function.as_deref(), Some("outer_fn"));
    }

    #[test]
    fn enclosing_function_name_finds_outer_rust_fn() {
        // The carrier sits inside `outer_fn`; the scan walks upward and
        // returns it.
        let source =
            "fn elsewhere() {}\nfn outer_fn() {\n    // provekit-concept: {}\n    do_thing();\n}\n";
        let lines = source.split_inclusive('\n').collect::<Vec<_>>();
        // The carrier is on line index 2 (0-based).
        let carrier_idx = 2;
        let name = enclosing_function_name(&lines, carrier_idx);
        assert_eq!(name.as_deref(), Some("outer_fn"));
    }

    #[test]
    fn enclosing_function_name_returns_none_at_module_scope() {
        let source = "// provekit-concept: {}\nfn after() {}\n";
        let lines = source.split_inclusive('\n').collect::<Vec<_>>();
        let name = enclosing_function_name(&lines, 0);
        assert!(name.is_none());
    }

    #[test]
    fn transform_source_text_populates_containing_function_from_scan() {
        // The carrier sits inside `outer_fn`; the site walk auto-fills
        // `containing_function` from the source scan when the JSON
        // payload omits it.
        let source = "fn before() {}\nfn outer_fn() {\n    // provekit-concept: {\"concept_name\":\"concept:demo\",\"function\":\"do_thing\",\"params\":[\"x\"],\"param_types\":[\"u32\"],\"return_type\":\"u32\"}\n    fn do_thing(x: u32) -> u32 {\n        unimplemented!()\n    }\n}\nfn after() {}\n";
        struct CaptureKit {
            captured: std::sync::Mutex<Option<CarrierComment>>,
        }
        impl SiteTransformKit for CaptureKit {
            fn target_language(&self) -> &str {
                "rust"
            }
            fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
                *self.captured.lock().unwrap() = Some(carrier.clone());
                Ok(SiteOutcome::Materialize {
                    body: "fn do_thing(x: u32) -> u32 {\n    x\n}".to_string(),
                    binding_cid: "cid:mock".to_string(),
                    contract_cid: None,
                    loss_record: json!([]),
                })
            }
        }
        let kit = CaptureKit {
            captured: std::sync::Mutex::new(None),
        };
        let _ = transform_source_text(source, &kit).expect("transform succeeds");
        let captured = kit.captured.lock().unwrap().clone().expect("site visited");
        assert_eq!(captured.containing_function.as_deref(), Some("outer_fn"));
    }

    #[test]
    fn build_receipt_routes_outcomes_into_trichotomy_buckets() {
        // Three sites: one Materialize, one LoudlyLossy, one Refuse. The
        // receipt should land each in the right bucket and preserve
        // source-order in `site_witnesses`.
        struct MixedKit;
        impl SiteTransformKit for MixedKit {
            fn target_language(&self) -> &str {
                "rust"
            }
            fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
                match carrier.function.as_str() {
                    "exact" => Ok(SiteOutcome::Materialize {
                        body: "fn exact() {}".to_string(),
                        binding_cid: "cid:exact".to_string(),
                        contract_cid: None,
                        loss_record: json!([]),
                    }),
                    "lossy" => Ok(SiteOutcome::LoudlyLossy {
                        body: "fn lossy() {}".to_string(),
                        binding_cid: "cid:lossy".to_string(),
                        contract_cid: None,
                        declared_loss: vec!["dim:overflow".to_string()],
                    }),
                    _ => Ok(SiteOutcome::Refuse {
                        reason: "no binding".to_string(),
                        would_close_with_concept: "concept:demo".to_string(),
                    }),
                }
            }
        }
        let source = "// provekit-concept: {\"concept_name\":\"c:a\",\"function\":\"exact\",\"params\":[],\"param_types\":[],\"return_type\":\"void\"}\n\
// provekit-concept: {\"concept_name\":\"c:b\",\"function\":\"lossy\",\"params\":[],\"param_types\":[],\"return_type\":\"void\"}\n\
// provekit-concept: {\"concept_name\":\"c:c\",\"function\":\"refused\",\"params\":[],\"param_types\":[],\"return_type\":\"void\"}\n";
        let kit = MixedKit;
        let (_, sites_and_outcomes) =
            transform_source_text_collecting_refusals(source, &kit).expect("transform");
        let receipt = build_receipt(&kit, "rust", None, "rusqlite", &sites_and_outcomes);
        assert_eq!(receipt.aggregate_summary.exact, 1);
        assert_eq!(receipt.aggregate_summary.lossy, 1);
        assert_eq!(receipt.aggregate_summary.refused, 1);
        assert_eq!(receipt.site_witnesses.len(), 3);
        assert_eq!(receipt.loss_records.len(), 1);
        assert_eq!(receipt.refusal_mementos.len(), 1);
        assert_eq!(receipt.refusal_mementos[0].reason, "no binding");
        assert_eq!(
            receipt.refusal_mementos[0].would_close_with_concept,
            "concept:demo"
        );
    }

    #[test]
    fn refused_site_leaves_carrier_and_stub_intact_in_rewritten_source() {
        // Substrate gap #84 regression: a refused boundary MUST preserve
        // its carrier comment + stub function verbatim in the rewritten
        // source so a different library's materialize pass can pick it
        // up. Multi-library and multi-vendor consumers depend on this
        // no-op invariant — refused-destroys-stub broke them.

        struct RefuseAllKit;
        impl SiteTransformKit for RefuseAllKit {
            fn target_language(&self) -> &str {
                "rust"
            }
            fn transform_site(&self, _carrier: &CarrierComment) -> Result<SiteOutcome, String> {
                Ok(SiteOutcome::Refuse {
                    reason: "library X does not provide this concept".to_string(),
                    would_close_with_concept: "concept:elsewhere".to_string(),
                })
            }
        }

        let carrier_line = "// provekit-concept: {\"concept_name\":\"c:x\",\"function\":\"f\",\"params\":[],\"param_types\":[],\"return_type\":\"i64\"}\n";
        let stub = "pub fn f() -> i64 {\n    unimplemented!()\n}\n";
        let source = format!("{carrier_line}{stub}");

        let (rewritten, sites_and_outcomes) =
            transform_source_text_collecting_refusals(&source, &RefuseAllKit)
                .expect("refuse-only kit must not error");

        // Receipt records the refusal as a first-class outcome.
        assert_eq!(sites_and_outcomes.len(), 1);
        assert!(matches!(
            sites_and_outcomes[0].1,
            SiteOutcome::Refuse { .. }
        ));

        // The carrier comment is preserved so another library pass can find it.
        assert!(
            rewritten.contains("// provekit-concept:"),
            "carrier comment must survive a Refuse (gap #84): {rewritten}"
        );
        // The stub function signature is preserved so the next pass has
        // somewhere to splice into.
        assert!(
            rewritten.contains("pub fn f() -> i64"),
            "stub signature must survive a Refuse (gap #84): {rewritten}"
        );
        // The stub body is preserved verbatim (no silent partial drop).
        assert!(
            rewritten.contains("unimplemented!()"),
            "stub body must survive a Refuse (gap #84): {rewritten}"
        );
    }

    #[test]
    fn mixed_outcomes_only_rewrite_resolved_sites_keep_refused_intact() {
        // Substrate gap #84: in a file containing BOTH a resolvable
        // boundary AND a refused boundary, only the resolved one gets
        // its stub spliced; the refused one stays verbatim. This is the
        // multi-library invariant the voltron-demo depends on.

        struct OneResolvesOneRefuses;
        impl SiteTransformKit for OneResolvesOneRefuses {
            fn target_language(&self) -> &str {
                "rust"
            }
            fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
                if carrier.function == "resolves" {
                    Ok(SiteOutcome::Materialize {
                        body: "{ 42 }".to_string(),
                        binding_cid: "cid:resolves".to_string(),
                        contract_cid: None,
                        loss_record: json!([]),
                    })
                } else {
                    Ok(SiteOutcome::Refuse {
                        reason: "different library".to_string(),
                        would_close_with_concept: "concept:elsewhere".to_string(),
                    })
                }
            }
        }

        let source = "\
// provekit-concept: {\"concept_name\":\"c:a\",\"function\":\"resolves\",\"params\":[],\"param_types\":[],\"return_type\":\"i64\"}
pub fn resolves() -> i64 {
    unimplemented!()
}
// provekit-concept: {\"concept_name\":\"c:b\",\"function\":\"refused\",\"params\":[],\"param_types\":[],\"return_type\":\"i64\"}
pub fn refused() -> i64 {
    unimplemented!()
}
";
        let (rewritten, outcomes) =
            transform_source_text_collecting_refusals(source, &OneResolvesOneRefuses)
                .expect("mixed transform");

        assert_eq!(outcomes.len(), 2);

        // Resolved site: body was spliced (the `{ 42 }` shows up, the
        // unimplemented placeholder is gone for THIS function only).
        assert!(rewritten.contains("42"), "resolved body should be spliced");

        // Refused site: full carrier + stub preserved verbatim.
        assert!(
            rewritten.contains("\"function\":\"refused\""),
            "refused carrier must survive: {rewritten}"
        );
        assert!(
            rewritten.contains("pub fn refused() -> i64"),
            "refused stub signature must survive: {rewritten}"
        );
    }
}
