// SPDX-License-Identifier: Apache-2.0
//
// provekit-realize-c-core
//
// PEP 1.7.0 realize plugin for C. Lowers ProofIR concept bindings to
// idiomatic C source. Parallel to `implementations/rust/provekit-realize-rust-core/`
// which is the template this mirrors.
//
// Federation principle: ALL C surface emission lives here. Zero C syntax
// knowledge belongs in `provekit-cli` or any other Rust crate.
//
// Fallthrough chain per the body-template-memento spec §1.2 and the
// coordinator amendment:
//   1. sugar dict match (op_pattern entries for concept-level ops, e.g. concept:free)
//   2. body-template match (canonical body for the concept)
//   3. language stub: `/* provekit-bind canonical: <concept> */ abort();`
//
// `is_stub` is true only when fallthrough reaches step 3.
//
// concept:free realization:
//   C has explicit heap management so concept:free lowers to `free(${ptr});`.
//   This is a LOSSLESS realization -- the IR concept maps exactly to the C
//   stdlib call. loss_record = {}.

use serde_json::Value;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// CID constants (minted by `mint-plugin-cid`, pinned in substrate_default_cids.rs)
// ---------------------------------------------------------------------------

/// CID for `menagerie/c-language-signature/specs/sugar/c-canonical.json`
pub const SUGAR_PLUGIN_CID: &str =
    "blake3-512:a67012722271aca3a882bc82fa7b92941453e750a65366534ea37ef8eef593921bb3aa33e6b0051bf64531962346014097ef9a99f14f7c3245de1beea6c076dc";

/// CID for `menagerie/c-language-signature/specs/body-templates/c-canonical-bodies.json`
pub const BODY_TEMPLATE_PLUGIN_CID: &str =
    "blake3-512:44f18ea2725ec26196399f6511bf3887db52db3c0e1356e7e090ff41f893929e177df9786d6639d8015d08641dda54338f83e1bd24a07828b5bf6b33c0d7d329";

// ---------------------------------------------------------------------------
// Realization result
// ---------------------------------------------------------------------------

/// Result of lowering one function binding.
///
/// `is_stub = true` means the body fell through to the language stub;
/// `is_stub = false` means a body-template or op-pattern entry rendered
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
/// code at the op site (e.g. `free(${ptr});` for C).
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
fn load_body_entries() -> Vec<BodyTemplateEntry> {
    const BODIES_JSON: &str =
        include_str!("../../../../menagerie/c-language-signature/specs/body-templates/c-canonical-bodies.json");
    parse_body_entries(BODIES_JSON)
}

fn load_sugar_entries() -> Vec<SugarEntry> {
    const SUGAR_JSON: &str =
        include_str!("../../../../menagerie/c-language-signature/specs/sugar/c-canonical.json");
    parse_sugar_entries(SUGAR_JSON)
}

/// Return the raw JCS-canonical content JSON string, extracted from the embedded JSON.
///
/// Used in `provekit.plugin.describe` responses.
pub fn sugar_content_json_from_embedded() -> String {
    const SUGAR_JSON: &str =
        include_str!("../../../../menagerie/c-language-signature/specs/sugar/c-canonical.json");
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
// Type mapping
// ---------------------------------------------------------------------------

/// Map a source type string to a C type string.
///
/// For C-to-C targeting, the source type is used as-is when it is already
/// a valid C type. We apply a small normalization set for common IR types.
pub fn map_source_type(src: &str) -> &str {
    match src {
        "i64" | "int64" => "int64_t",
        "i32" | "int32" => "int32_t",
        "u64" | "uint64" => "uint64_t",
        "u32" | "uint32" => "uint32_t",
        "bool" => "int",
        "()" | "void" => "void",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Annotation prefix (sugar dict: predicate match)
// ---------------------------------------------------------------------------

/// Render the annotation prefix for a concept given the available sugar entries.
/// For C, contract-clause annotations emit block comments above the function.
///
/// If no predicate entry matches, emits a default `/* concept: <name> */` comment.
fn annotation_prefix_for(concept_name: &str, sugar_hint: Option<&str>) -> String {
    let head = sugar_hint.unwrap_or(concept_name);
    let entries = sugar_entries();
    for entry in entries {
        if !matches!(entry.kind, SugarKind::Predicate) {
            continue;
        }
        if entry.head == head {
            let _ = &entry.surface_locator;
            let rendered = entry.template.replace("${concept}", concept_name);
            return format!("{}\n", rendered);
        }
    }
    format!("/* concept: {concept_name} */\n")
}

/// Look up an op-pattern body for a concept (e.g. concept:free -> `free(${ptr});`).
///
/// `concept_name` is matched against `op_pattern.head`. The `surface_locator`
/// must be `"op:inline"` for the template to apply as a function body.
/// Parameter names are substituted before returning.
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
        if let Some(first) = param_names.first() {
            rendered = rendered.replace("${val}", first);
            rendered = rendered.replace("${ptr}", first);
            rendered = rendered.replace("${param0}", first);
        }
        for (i, name) in param_names.iter().enumerate() {
            rendered = rendered.replace(&format!("${{param{i}}}"), name);
        }
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
    format!("/* provekit-bind canonical: {concept_name} */\n    abort();")
}

// ---------------------------------------------------------------------------
// Core emit function
// ---------------------------------------------------------------------------

/// Emit a C function for a single binding.
///
/// Output shape:
/// ```text
/// /* concept: <concept_name> */
/// <return_type> <function>(<type> <param>, ...) {
///     <body>
/// }
/// ```
///
/// - `params`: parameter names in order
/// - `param_types`: source-language type strings; mapped to C equivalents
/// - `return_type`: source-language return type; mapped to C equivalent
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
            let t = param_types.get(i).map(String::as_str).unwrap_or("int64_t");
            let mapped = map_source_type(t);
            format!("{mapped} {name}")
        })
        .collect();
    let typed_param_list = if typed_params.is_empty() {
        "void".to_string()
    } else {
        typed_params.join(", ")
    };
    let mapped_return = map_source_type(return_type);

    // Annotation prefix (sugar dict: comment form for C).
    let annotation = annotation_prefix_for(concept_name, None);

    // Body fallthrough chain:
    //   Step 1a: op_pattern sugar dict match (e.g. concept:free -> free(${ptr});)
    //   Step 2:  body-template match
    //   Step 3:  language stub (abort()) -- sets is_stub = true
    let (body_str, is_stub) = if let Some(op_body) = op_body_for(concept_name, params) {
        // Op-pattern match: emit inline body.
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
        "{annotation}{mapped_return} {function}({typed_param_list}) {{\n{body_str}}}\n"
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
        assert!(r.source.contains("return x;"), "body should contain return x");
        assert!(r.source.contains("int64_t foo(int64_t x)"), "sig shape");
    }

    #[test]
    fn option_body_template_renders() {
        let r = emit("opt", &["v".to_string()], &["int64_t *".to_string()], "i64", "option");
        assert!(!r.is_stub);
        assert!(r.source.contains("NULL") || r.source.contains("return"));
    }

    #[test]
    fn unknown_concept_falls_through_to_stub() {
        let r = emit("foo", &["x".to_string()], &["i64".to_string()], "i64", "concept:unknown-xyz");
        assert!(r.is_stub);
        assert!(r.source.contains("abort()"));
    }

    #[test]
    fn unit_concept_zero_params() {
        let r = emit("noop", &[], &[], "()", "unit");
        assert!(!r.is_stub);
        // C unit -> void return with no params -> "void noop(void)"
        assert!(r.source.contains("void noop(void)"));
        assert!(r.source.contains("return;"));
    }

    #[test]
    fn annotation_prefix_emitted() {
        let r = emit("id", &["x".to_string()], &["i64".to_string()], "i64", "identity");
        assert!(r.source.contains("/* concept: identity */"), "annotation comment");
    }

    #[test]
    fn free_concept_emits_free_not_stub() {
        // concept:free must dispatch via op_pattern -> free(${ptr});
        // It must NOT fall through to the language stub.
        let r = emit(
            "free_resource",
            &["p".to_string()],
            &["void *".to_string()],
            "()",
            "free",
        );
        assert!(!r.is_stub, "concept:free should not be a stub (op_pattern matches)");
        assert!(
            r.source.contains("free(p)"),
            "concept:free body should contain free(p); got:\n{}",
            r.source
        );
    }

    #[test]
    fn pair_concept_two_params() {
        let r = emit(
            "make_pair",
            &["a".to_string(), "b".to_string()],
            &["i64".to_string(), "i64".to_string()],
            "i64",
            "pair",
        );
        assert!(!r.is_stub);
        assert!(r.source.contains("a + b"));
    }

    #[test]
    fn bool_cell_concept() {
        let r = emit("toggle", &["b".to_string()], &["bool".to_string()], "bool", "bool-cell");
        assert!(!r.is_stub);
        assert!(r.source.contains("!b"));
    }

    #[test]
    fn type_mapping_i64_to_int64_t() {
        assert_eq!(map_source_type("i64"), "int64_t");
        assert_eq!(map_source_type("bool"), "int");
        assert_eq!(map_source_type("()"), "void");
        assert_eq!(map_source_type("void"), "void");
    }
}
