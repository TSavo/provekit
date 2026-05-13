// SPDX-License-Identifier: Apache-2.0
//
// provekit-realize-rust-core
//
// PEP 1.7.0 realize plugin for Rust. Lowers ProofIR concept bindings to
// idiomatic Rust source. Parallel to `implementations/java/provekit-realize-java-core/`
// which is the template this mirrors.
//
// Federation principle: ALL Rust surface emission lives here. Zero Rust
// syntax knowledge belongs in `provekit-cli` or any other Rust crate.
//
// Fallthrough chain per the body-template-memento spec §1.2 and the
// coordinator amendment:
//   1. sugar dict match (contract clause sugar -- currently comment-style for Rust)
//   2. body-template match (canonical body for the concept)
//   3. language stub: `todo!("provekit-bind canonical: <concept>")`
//
// `is_stub` is true only when fallthrough reaches step 3.

use serde_json::Value;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// CID constants (minted by `mint-plugin-cid`, pinned in substrate_default_cids.rs)
// ---------------------------------------------------------------------------

/// CID for `menagerie/rust-language-signature/specs/sugar/rust-canonical.json`
pub const SUGAR_PLUGIN_CID: &str =
    "blake3-512:666480f85eafb36d750c4fef4e5df42e33740ceb1f8e0bff2c82743beeccb0aff11d0a65e1c05827782d5c1023b853e5a2cccc3755d5a161c07668e4e7a5ae4a";

/// CID for `menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json`
pub const BODY_TEMPLATE_PLUGIN_CID: &str =
    "blake3-512:39bf0c5b81d7769d60e82326a36daeb66241c7527e4ac542b1ce9e4ab40cb19a25a4e4b25e406106341f1c414f7ccc3b7523fab4aa3ee34ddc751f036d26e949";

// ---------------------------------------------------------------------------
// Realization result
// ---------------------------------------------------------------------------

/// Result of lowering one function binding.
///
/// `is_stub = true` means the body fell through to the language stub
/// (`todo!(...)`); `is_stub = false` means a body-template entry rendered
/// a real body. `cmd_bind` uses this to emit accurate per-concept
/// `bind-stub-body-emitted` gap entries.
#[derive(Debug, Clone)]
pub struct Realization {
    pub source: String,
    pub is_stub: bool,
}

// ---------------------------------------------------------------------------
// Body-template entry (per body-template-memento.md §2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BodyTemplateEntry {
    concept_name: String,
    template_kind: String,
    template: String,
    min_params: Option<usize>,
    max_params: Option<usize>,
}

// ---------------------------------------------------------------------------
// Sugar dict entry (per sugar-dict-memento spec)
// ---------------------------------------------------------------------------

/// Which pattern shape this sugar entry uses.
///
/// `Predicate` entries match contract-clause predicates (requires/ensures) and
/// emit comment-style annotations before the function.
///
/// `Op` entries match concept-level ops (e.g. concept:free) and emit inline
/// code at the op site (e.g. `drop(${val});` for Rust RAII).
#[derive(Debug, Clone)]
enum SugarKind {
    Predicate,
    Op,
}

#[derive(Debug, Clone)]
struct SugarEntry {
    kind: SugarKind,
    /// `head` is the `predicate_pattern.head` or `op_pattern.head` value.
    head: String,
    template: String,
    surface_locator: String,
}

// ---------------------------------------------------------------------------
// Plugin state (loaded once, shared across requests)
// ---------------------------------------------------------------------------

static BODY_ENTRIES: OnceLock<Vec<BodyTemplateEntry>> = OnceLock::new();
static SUGAR_ENTRIES: OnceLock<Vec<SugarEntry>> = OnceLock::new();

fn body_entries() -> &'static Vec<BodyTemplateEntry> {
    BODY_ENTRIES.get_or_init(load_body_entries)
}

fn sugar_entries() -> &'static Vec<SugarEntry> {
    SUGAR_ENTRIES.get_or_init(load_sugar_entries)
}

/// Load body-template entries from the embedded JSON bytes.
///
/// The JSON is embedded at compile time from the menagerie path. If the
/// resource is absent (integration test shim, unusual install), degrades to
/// "no entries" so the stub fallthrough still works.
fn load_body_entries() -> Vec<BodyTemplateEntry> {
    const BODIES_JSON: &str =
        include_str!("../../../../menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json");
    parse_body_entries(BODIES_JSON)
}

fn load_sugar_entries() -> Vec<SugarEntry> {
    const SUGAR_JSON: &str =
        include_str!("../../../../menagerie/rust-language-signature/specs/sugar/rust-canonical.json");
    parse_sugar_entries(SUGAR_JSON)
}

/// Return the raw JCS-canonical content JSON string, extracted from the embedded JSON.
///
/// This is used in `provekit.plugin.describe` responses. The content block
/// is serialized as-is from the embedded file's `header.content` object;
/// callers use this to verify the CID over the wire matches SUGAR_PLUGIN_CID.
pub fn sugar_content_json_from_embedded() -> String {
    const SUGAR_JSON: &str =
        include_str!("../../../../menagerie/rust-language-signature/specs/sugar/rust-canonical.json");
    let root: serde_json::Value = serde_json::from_str(SUGAR_JSON).expect("sugar json parse");
    let content = root
        .get("header")
        .and_then(|h| h.get("content"))
        .expect("header.content missing");
    serde_json::to_string(content).expect("serialize content")
}

fn parse_body_entries(raw: &str) -> Vec<BodyTemplateEntry> {
    let root: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let entries = match root
        .get("header")
        .and_then(|h| h.get("content"))
        .and_then(|c| c.get("entries"))
        .and_then(|e| e.as_array())
    {
        Some(a) => a.clone(),
        None => return vec![],
    };

    let mut out = Vec::with_capacity(entries.len());
    for item in &entries {
        let concept_name = match item.get("concept_name").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let tmpl_obj = match item.get("emission_template") {
            Some(v) => v,
            None => continue,
        };
        let kind = match tmpl_obj.get("kind").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let template = match tmpl_obj.get("template").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let guard = item.get("signature_guard");
        let min_params = guard
            .and_then(|g| g.get("min_params"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_params = guard
            .and_then(|g| g.get("max_params"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        out.push(BodyTemplateEntry {
            concept_name,
            template_kind: kind,
            template,
            min_params,
            max_params,
        });
    }
    out
}

fn parse_sugar_entries(raw: &str) -> Vec<SugarEntry> {
    let root: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let entries = match root
        .get("header")
        .and_then(|h| h.get("content"))
        .and_then(|c| c.get("entries"))
        .and_then(|e| e.as_array())
    {
        Some(a) => a.clone(),
        None => return vec![],
    };

    let mut out = Vec::with_capacity(entries.len());
    for item in &entries {
        // Determine which pattern shape this entry uses and extract the head.
        let (kind, head) = if let Some(head) = item
            .get("predicate_pattern")
            .and_then(|p| p.get("head"))
            .and_then(|v| v.as_str())
        {
            (SugarKind::Predicate, head.to_string())
        } else if let Some(head) = item
            .get("op_pattern")
            .and_then(|p| p.get("head"))
            .and_then(|v| v.as_str())
        {
            (SugarKind::Op, head.to_string())
        } else {
            // Entry has neither predicate_pattern nor op_pattern; skip.
            continue;
        };

        let tmpl_obj = match item.get("emission_template") {
            Some(v) => v,
            None => continue,
        };
        let template = match tmpl_obj.get("template").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let surface_locator = tmpl_obj
            .get("surface_locator")
            .and_then(|v| v.as_str())
            .unwrap_or("annotation:before-method")
            .to_string();
        out.push(SugarEntry {
            kind,
            head,
            template,
            surface_locator,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Type mapping (mirrors cmd_transport.rs map_source_type for TargetStyle::Rust)
// ---------------------------------------------------------------------------

/// Map a Rust source type to the Rust target type.
/// Rust-to-Rust is identity -- the type is already valid Rust.
pub fn map_source_type(src: &str) -> &str {
    src
}

// ---------------------------------------------------------------------------
// Annotation prefix (sugar dict match)
// ---------------------------------------------------------------------------

/// Render the annotation prefix for a concept given the available sugar entries.
/// For Rust this emits `// # requires:` / `// # ensures:` lines above the fn.
///
/// Only consults `Predicate`-kind sugar entries (contract-clause annotations).
/// If no predicate entry matches, returns a default `// concept: <name>` comment.
fn annotation_prefix_for(concept_name: &str, sugar_hint: Option<&str>) -> String {
    // Use sugar_hint if provided (direct predicate head). Otherwise look up
    // entries that match concept_name as a heuristic.
    let head = sugar_hint.unwrap_or(concept_name);
    let entries = sugar_entries();
    for entry in entries {
        if !matches!(entry.kind, SugarKind::Predicate) {
            continue;
        }
        if entry.head == head {
            // surface_locator tells us where to place the annotation.
            // For Rust, "annotation:before-method" maps to a line above the fn.
            // Other locators are reserved for future surface-location dispatch.
            let _ = &entry.surface_locator; // read to prevent dead_code lint
            // Substitute ${concept} placeholder if present.
            let rendered = entry.template.replace("${concept}", concept_name);
            return format!("{}\n", rendered);
        }
    }
    // Default: emit a comment naming the concept (no loss).
    format!("// concept: {concept_name}\n")
}

/// Look up an op-pattern body for a concept (e.g. concept:free → `drop(${val});`).
///
/// `concept_name` is matched against `op_pattern.head`. The `surface_locator`
/// must be `"op:inline"` for the template to apply as a function body.
/// Parameter names are substituted (${val} → first param, etc.) before returning.
///
/// Returns `None` if no op entry matches this concept.
fn op_body_for(concept_name: &str, param_names: &[String]) -> Option<String> {
    let entries = sugar_entries();
    for entry in entries {
        if !matches!(entry.kind, SugarKind::Op) {
            continue;
        }
        if entry.head != concept_name {
            continue;
        }
        if entry.surface_locator != "op:inline" {
            continue;
        }
        let mut rendered = entry.template.clone();
        // Substitute named placeholders from op_pattern.args if needed.
        // The rust-canonical sugar dict uses ${val} as the first arg name.
        // We substitute positionally: ${val} / ${ptr} / ${arg0} etc. →
        // param_names[0], and also generic ${param0}, ${param1} aliases.
        if let Some(first) = param_names.first() {
            // Support common single-arg op names.
            rendered = rendered.replace("${val}", first);
            rendered = rendered.replace("${ptr}", first);
            rendered = rendered.replace("${param0}", first);
        }
        for (i, name) in param_names.iter().enumerate() {
            rendered = rendered.replace(&format!("${{param{i}}}"), name);
        }
        // Reject if unresolved placeholders remain.
        if rendered.contains("${") {
            continue;
        }
        return Some(rendered);
    }
    None
}

// ---------------------------------------------------------------------------
// Body-template lookup (step 2 of fallthrough)
// ---------------------------------------------------------------------------

fn body_template_for(concept_name: &str, param_names: &[String]) -> Option<String> {
    let entries = body_entries();
    for entry in entries {
        if entry.concept_name != concept_name {
            continue;
        }
        if let Some(min) = entry.min_params {
            if param_names.len() < min {
                continue;
            }
        }
        if let Some(max) = entry.max_params {
            if param_names.len() > max {
                continue;
            }
        }
        if entry.template_kind != "verbatim" {
            continue;
        }
        let mut rendered = entry.template.clone();
        for (i, name) in param_names.iter().enumerate() {
            rendered = rendered.replace(&format!("${{param{i}}}"), name);
        }
        rendered = rendered.replace("${param_count}", &param_names.len().to_string());
        // Reject if any unresolved placeholders remain.
        if rendered.contains("${") {
            continue;
        }
        return Some(rendered);
    }
    None
}

// ---------------------------------------------------------------------------
// Language stub (step 3 of fallthrough)
// ---------------------------------------------------------------------------

fn stub_body(concept_name: &str) -> String {
    format!("todo!(\"provekit-bind canonical: {concept_name}\")")
}

// ---------------------------------------------------------------------------
// Core emit function
// ---------------------------------------------------------------------------

/// Emit a Rust function for a single binding.
///
/// Output shape:
/// ```text
/// // concept: <concept_name>
/// pub fn <function>(<param>: <type>, ...) -> <return_type> {
///     <body>
/// }
/// ```
///
/// - `params`: parameter names in order
/// - `param_types`: source-language (Rust) type strings; identity-mapped for Rust
/// - `return_type`: source-language return type; identity-mapped for Rust
/// - `concept_name`: canonical concept name for this binding
pub fn emit(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Realization {
    // Build typed param list.
    let typed_params: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let t = param_types.get(i).map(String::as_str).unwrap_or("i64");
            let mapped = map_source_type(t);
            format!("{name}: {mapped}")
        })
        .collect();
    let typed_param_list = typed_params.join(", ");
    let mapped_return = map_source_type(return_type);

    // Annotation prefix (sugar dict: comment form for Rust).
    let annotation = annotation_prefix_for(concept_name, None);

    // Body fallthrough chain:
    //   Step 1a: op_pattern sugar dict match (e.g. concept:free → drop(${val});)
    //   Step 2:  body-template match
    //   Step 3:  language stub (todo!(...)) -- sets is_stub = true
    let (body_str, is_stub) = if let Some(op_body) = op_body_for(concept_name, params) {
        // Op-pattern match: emit inline body (e.g. drop(val);).
        let indented = op_body
            .lines()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n");
        (format!("{indented}\n"), false)
    } else {
        match body_template_for(concept_name, params) {
            Some(body) => {
                // Indent the body one level.
                let indented = body
                    .lines()
                    .map(|l| format!("    {l}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                (format!("{indented}\n"), false)
            }
            None => {
                // Step 3: language stub.
                (format!("    {}\n", stub_body(concept_name)), true)
            }
        }
    };

    let source = format!(
        "{annotation}pub fn {function}({typed_param_list}) -> {mapped_return} {{\n{body_str}}}\n"
    );

    Realization { source, is_stub }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_body_template_renders() {
        let r = emit("foo", &["x".to_string()], &["i64".to_string()], "i64", "identity");
        assert!(!r.is_stub, "identity should match body-template");
        assert!(r.source.contains("x"), "body should reference param");
        assert!(r.source.contains("pub fn foo(x: i64) -> i64"), "sig shape");
    }

    #[test]
    fn option_body_template_renders() {
        let r = emit("opt", &["v".to_string()], &["Option<i64>".to_string()], "i64", "option");
        assert!(!r.is_stub);
        assert!(r.source.contains("unwrap_or"));
    }

    #[test]
    fn unknown_concept_falls_through_to_stub() {
        let r = emit("foo", &["x".to_string()], &["i64".to_string()], "i64", "concept:unknown-xyz");
        assert!(r.is_stub);
        assert!(r.source.contains("todo!"));
    }

    #[test]
    fn unit_concept_zero_params() {
        let r = emit("noop", &[], &[], "()", "unit");
        assert!(!r.is_stub);
        assert!(r.source.contains("()"));
    }

    #[test]
    fn annotation_prefix_emitted() {
        let r = emit("id", &["x".to_string()], &["i64".to_string()], "i64", "identity");
        assert!(r.source.contains("// concept: identity"), "annotation comment");
    }

    #[test]
    fn free_concept_emits_drop_not_stub() {
        // concept:free must dispatch via op_pattern → drop(${val});
        // It must NOT fall through to the language stub.
        let r = emit(
            "free_resource",
            &["v".to_string()],
            &["Box<i64>".to_string()],
            "()",
            "free",
        );
        assert!(!r.is_stub, "concept:free should not be a stub (op_pattern matches)");
        assert!(
            r.source.contains("drop(v)"),
            "concept:free body should contain drop(v); got:\n{}",
            r.source
        );
    }
}
