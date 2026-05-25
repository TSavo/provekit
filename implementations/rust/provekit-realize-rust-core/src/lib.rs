// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::realization_tags::tag_sugar_carrier;
use serde_json::{json, Value};

pub mod platform_semantics;

/// Catalog-driven operation-realization lookup (#1391). Loads realization
/// mementos from menagerie/concept-shapes/catalog/realizations/ at first
/// use; maps concept_name → rhs_op_name for target_lang=rust. The catalog
/// is the single source of truth; emit_kit_rust_op (below) is keyed off
/// the rhs op name and is the only remaining kit-specific code.
pub mod operation_realization_catalog {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static RUST_OP_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    static RUST_REVERSE_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();

    pub fn rust_op_for(concept_name: &str) -> Option<String> {
        let map = RUST_OP_MAP.get_or_init(|| build_map("rust"));
        map.get(concept_name).cloned()
    }

    /// Reverse: rust kit-op name → concept_name. Used by the lift side.
    pub fn concept_for_rust_op(kit_op_name: &str) -> Option<String> {
        let reverse = RUST_REVERSE_MAP.get_or_init(|| {
            let forward = RUST_OP_MAP.get_or_init(|| build_map("rust"));
            let mut rev = HashMap::new();
            for (k, v) in forward.iter() {
                rev.entry(v.clone()).or_insert_with(|| k.clone());
            }
            rev
        });
        reverse.get(kit_op_name).cloned()
    }

    fn build_map(target_lang: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        // Walk up from CWD to find menagerie/.
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let mut root: Option<std::path::PathBuf> = None;
        let mut p: Option<&std::path::Path> = Some(cwd.as_path());
        while let Some(cur) = p {
            if cur.join("menagerie").is_dir() {
                root = Some(cur.to_path_buf());
                break;
            }
            p = cur.parent();
        }
        let Some(root) = root else {
            return out;
        };
        let realizations_dir = root.join("menagerie/concept-shapes/catalog/realizations");
        let Ok(entries) = std::fs::read_dir(&realizations_dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(memento) = doc.get("memento") else {
                continue;
            };
            if memento.get("role").and_then(|v| v.as_str()) != Some("abstraction-realization") {
                continue;
            }
            if memento.get("target_lang").and_then(|v| v.as_str()) != Some(target_lang) {
                continue;
            }
            let Some(post) = memento.get("post") else {
                continue;
            };
            let lhs_name = post
                .get("lhs")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str());
            let rhs_name = post
                .get("rhs")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str());
            if let (Some(l), Some(r)) = (lhs_name, rhs_name) {
                out.entry(l.to_string()).or_insert_with(|| r.to_string());
            }
        }
        out
    }
}

const BODY_TEMPLATE_REL: &str =
    "menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json";
const RETURN_OP_CID: &str = "blake3-512:776d417c66325df1d40e3e0fd7331195e2b1d14f9c30b5984030f21aa8b6b38b3eb81ee3dddd46716003275c9960022e2273dd8efb0110bacc5719811ee18dc6";
const CALL_NEW_OP_CID: &str = "blake3-512:e6576534d74eee6b309fa55457620d4903472dcd331f0cb9c2be2a95994655ad64ef1fa56f778534f6ba5c04c055069bb109de3dae4dc45bde7dd689671b24b8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realization {
    pub source: String,
    pub is_stub: bool,
    pub extension: String,
    pub emitted_artifact_cid: String,
    pub observed_loss_record: Value,
    pub used_sugars: Vec<Value>,
}

#[derive(Debug, Clone)]
struct BodyTemplateEntry {
    concept_name: String,
    mode: Option<String>,
    realization_kind: Option<String>,
    template_kind: String,
    template: String,
    loss_record_contribution: Option<Value>,
    min_params: Option<usize>,
    max_params: Option<usize>,
    requires_param_types: Option<Vec<String>>,
    requires_return_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedBody {
    body: String,
    observed_loss_record: Value,
}

pub fn emit_stub(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Realization {
    emit_stub_with_mode(
        function,
        params,
        param_types,
        return_type,
        concept_name,
        None,
    )
}

pub fn emit_stub_with_mode(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
    mode: Option<&str>,
) -> Realization {
    emit_stub_with_mode_and_invocations(
        function,
        params,
        param_types,
        return_type,
        concept_name,
        mode,
        &[],
    )
}

fn emit_stub_with_mode_and_invocations(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
    mode: Option<&str>,
    proc_macro_invocations: &[Value],
) -> Realization {
    let rendered = operator_body_template_for(concept_name, params, param_types, return_type, mode)
        .or_else(|| body_template_for(concept_name, params, param_types, return_type, mode));
    if rendered.is_none() {
        if let Some(realized) =
            emit_sugar_carrier(function, params, param_types, return_type, concept_name)
        {
            return apply_proc_macro_invocations(realized, proc_macro_invocations);
        }
    }
    let realization = match rendered {
        Some(rendered) => emit_function_with_evidence(
            function,
            params,
            param_types,
            return_type,
            &rendered.body,
            rendered.observed_loss_record,
            Vec::new(),
        ),
        None => {
            let body = stub_body_for(concept_name);
            emit_function(function, params, param_types, return_type, &body, true)
        }
    };
    apply_proc_macro_invocations(realization, proc_macro_invocations)
}

pub fn emit_from_term_shape(
    term_shape: &Value,
    function_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Realization {
    emit_from_term_shape_with_bindings(
        term_shape,
        &[],
        function_name,
        params,
        param_types,
        return_type,
    )
}

pub fn emit_from_term_shape_with_bindings(
    term_shape: &Value,
    operand_bindings: &[Value],
    function_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Realization {
    let mut context = ShapeLoweringContext::new(params, param_types, return_type, operand_bindings);
    // Tail-expression top-level: if the top of the term_shape is not a
    // sequence or explicit return, try lowering as a bare expression
    // (Rust implicit-return tail / unit-typed expression statement).
    // This covers two source-side forms:
    //   1. `pub fn json_parse(s) -> Result<...> { serde_json::from_str(s) }`
    //      — non-unit returning function with a single tail expression.
    //   2. `pub fn encode_value(v, out) { match v { ... } }`
    //      — unit-returning function whose body is a single expression
    //      (a match block, an if block, etc.) used for its side effects.
    let top_concept = term_shape_concept_name(term_shape);
    let is_seq_or_return = matches!(
        top_concept.as_deref(),
        Some("concept:seq") | Some("seq") | Some("concept:return") | Some("return")
    );
    if !is_seq_or_return {
        let mut tail_ctx =
            ShapeLoweringContext::new(params, param_types, return_type, operand_bindings);
        if let Some(expr) = lower_term_shape_expression(term_shape, &mut tail_ctx, &[]) {
            return emit_function(
                function_name,
                params,
                param_types,
                return_type,
                &expr.text,
                false,
            );
        }
    }
    match lower_term_shape_body(term_shape, &mut context, &[]) {
        Some(body) => {
            // #1391 follow-on: post-pass mut inference. When a local binding
            // is later used as the receiver of a mutating method call
            // (.push, .insert, .push_str, .set, .append, .add), promote its
            // `let X = ...` to `let mut X = ...`. The lift side often loses
            // mut markers in cross-language round-trips; this recovers them
            // structurally from usage.
            let body = infer_let_mut(&body);
            emit_function(
                function_name,
                params,
                param_types,
                return_type,
                &body,
                false,
            )
        }
        None => emit_stub(
            function_name,
            params,
            param_types,
            return_type,
            &term_shape_concept_name(term_shape).unwrap_or_else(|| "term-shape".to_string()),
        ),
    }
}

/// Emits Rust for the D7 resolved Value::null body shape:
/// `return(call:new(literal("new" | "*::new"), literal(["Null"])))`.
///
/// The trailing `return` node is lowered as a Rust tail expression. The nested
/// `call:new` shape lowers to `<callee>(Value::Null)`. D7-v2 fixtures used a
/// bare `new`; D7-v3 fixtures carry the receiver-prefixed `Arc::new` spelling.
/// Unsupported resolved concepts or literal shapes fall back to the existing
/// `panic!("provekit-bind canonical: <concept>")` stub body.
pub fn emit_from_resolved(
    resolved_term_json: &str,
    function_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Realization {
    let resolved = match serde_json::from_str::<Value>(resolved_term_json) {
        Ok(resolved) => resolved,
        Err(_) => {
            return emit_stub(
                function_name,
                params,
                param_types,
                return_type,
                "malformed-resolved-term",
            );
        }
    };

    match lower_resolved_body(&resolved, params) {
        Ok(body) => emit_function(
            function_name,
            params,
            param_types,
            return_type,
            &body,
            false,
        ),
        Err(concept_name) => emit_stub(
            function_name,
            params,
            param_types,
            return_type,
            &concept_name,
        ),
    }
}

/// RAII guard that sets `CURRENT_VISIBILITY` for the lifetime of a realization
/// and restores the prior value on drop — even across a panic. Every site that
/// writes `CURRENT_VISIBILITY` MUST do so through this guard: a leaked write
/// (the bug the unguarded dispatch path used to have) silently contaminates
/// later realizations on the same thread, which is exactly what made the
/// realize-rust-core unit tests order-dependent on each other's pollution.
struct VisibilityGuard {
    previous: Option<String>,
}

impl VisibilityGuard {
    /// Snapshot the current thread-local value and install an EXPLICIT
    /// `visibility` (`Some(visibility)`) for the duration of the returned
    /// guard. An empty `visibility` is `Some("")` — explicit private — NOT the
    /// absent/default state, so a private source visibility threaded here emits
    /// a bare `fn` and is never over-promoted to `pub`.
    fn set(visibility: &str) -> Self {
        Self::set_optional(Some(visibility))
    }

    /// Like [`VisibilityGuard::set`] but preserves the ABSENT vs PRESENT-EMPTY
    /// distinction: `None` installs `None` (the default => `pub`), `Some(v)`
    /// installs `Some(v)` (explicit; `Some("")` => private `fn`). The dispatch
    /// path uses this so a spec that simply omits `visibility` defaults to
    /// public, while a spec carrying `visibility: ""` (bind's private encoding)
    /// emits a bare `fn`.
    fn set_optional(visibility: Option<&str>) -> Self {
        let guard = VisibilityGuard {
            previous: CURRENT_VISIBILITY.with(|v| v.borrow().clone()),
        };
        CURRENT_VISIBILITY.with(|v| *v.borrow_mut() = visibility.map(|s| s.to_string()));
        guard
    }
}

impl Drop for VisibilityGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        CURRENT_VISIBILITY.with(|v| *v.borrow_mut() = previous);
    }
}

/// Like [`emit_from_resolved`] but reproduces the source-language visibility
/// (`pub`, `pub(crate)`, or `""` for private/inherited) on the emitted
/// signature. `function_source` reads visibility from the `CURRENT_VISIBILITY`
/// thread-local, which the RPC dispatch path sets from the spec's `visibility`
/// field; direct callers (e.g. the D7 source-round-trip receipts) had no way to
/// thread it. Passing this explicitly makes the visibility PRESENT: `""`
/// regenerates a bare `fn` for a private source slice (distinct from the absent
/// case, which defaults to `pub`); `"pub"` regenerates `pub fn`. This variant
/// sets and restores the thread-local around the emit so visibility round-trips
/// byte-identically.
pub fn emit_from_resolved_with_visibility(
    resolved_term_json: &str,
    function_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    visibility: &str,
) -> Realization {
    // RAII guard so the thread-local is restored even if emit_from_resolved
    // panics — a leaked CURRENT_VISIBILITY would otherwise contaminate
    // subsequent realizations on the same thread.
    let _guard = VisibilityGuard::set(visibility);
    emit_from_resolved(
        resolved_term_json,
        function_name,
        params,
        param_types,
        return_type,
    )
}

fn emit_function(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
    is_stub: bool,
) -> Realization {
    let source = function_source(function, params, param_types, return_type, body);
    let emitted_artifact_cid = blake3_512_of(source.as_bytes());
    Realization {
        source,
        is_stub,
        extension: "rs".to_string(),
        emitted_artifact_cid,
        observed_loss_record: json!({}),
        used_sugars: Vec::new(),
    }
}

fn emit_function_with_evidence(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
    observed_loss_record: Value,
    used_sugars: Vec<Value>,
) -> Realization {
    let source = function_source(function, params, param_types, return_type, body);
    let emitted_artifact_cid = blake3_512_of(source.as_bytes());
    Realization {
        source,
        is_stub: false,
        extension: "rs".to_string(),
        emitted_artifact_cid,
        observed_loss_record,
        used_sugars,
    }
}

#[derive(Debug, Clone)]
struct ShapeExpression {
    text: String,
    type_name: String,
}

#[derive(Debug)]
struct ShapeLoweringContext {
    params: Vec<String>,
    param_types: Vec<String>,
    return_type: String,
    operand_bindings: BTreeMap<Vec<usize>, String>,
    defined_symbols: BTreeSet<String>,
    next_leaf: usize,
    next_temp: usize,
    last_assigned_symbol: Option<String>,
}

impl ShapeLoweringContext {
    fn new(
        params: &[String],
        param_types: &[String],
        return_type: &str,
        operand_bindings: &[Value],
    ) -> Self {
        Self {
            params: params.to_vec(),
            param_types: param_types.to_vec(),
            return_type: return_type.to_string(),
            operand_bindings: operand_binding_map(operand_bindings),
            defined_symbols: params.iter().cloned().collect(),
            next_leaf: 0,
            next_temp: 0,
            last_assigned_symbol: None,
        }
    }

    /// Substrate-honest: no silent fallback. When the lowerer hits a
    /// position with no recognized leaf shape AND no operand_binding,
    /// it must refuse loudly (return None) rather than substitute the
    /// function's first parameter — silent substitution produces
    /// compile-able but semantically-wrong Rust, which violates the
    /// trichotomy (exact / loudly-lossy / refuse).
    fn fallback_leaf(&mut self) -> Option<ShapeExpression> {
        self.next_leaf += 1;
        None
    }

    fn temp_name(&mut self) -> String {
        let name = format!("__provekit_v{}", self.next_temp);
        self.next_temp += 1;
        name
    }
}

fn lower_term_shape_body(
    shape: &Value,
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<String> {
    let concept_name = term_shape_concept_name(shape)?;
    if concept_name == "concept:comment" || concept_name == "comment" {
        return Some(rust_comment_body(term_shape_comment_surface(shape)?));
    }
    if concept_name == "concept:skip" || concept_name == "skip" {
        return Some(String::new());
    }
    if concept_name == "concept:seq" || concept_name == "seq" {
        let mut lines = Vec::new();
        let children: Vec<&Value> = term_shape_args(shape).into_iter().collect();
        let last_index = children.len().saturating_sub(1);
        let returns_non_unit = map_source_type(&context.return_type) != "()";
        for (index, child) in children.iter().enumerate() {
            let child_position = append_position(position, index);
            // #1391 follow-on: blank-line carrier — emit an empty line
            // entry. The join("\n") in the function-body assembler turns
            // it into a single blank line, matching rust source's
            // paragraph-style separators. One marker per gap.
            if term_shape_concept_name(child).as_deref() == Some("concept:blank-line") {
                lines.push(String::new());
                continue;
            }
            // Tail-expression preference: when this is the LAST child of
            // the function-root seq AND the function returns non-unit,
            // try the EXPRESSION form first (no `;`). This matches rust's
            // tail-expression convention — `Ok(build_ir_document(...))`
            // as the last line, not `Ok(...);` followed by a synthesized
            // tail. Falls through to body form if expression-lift fails.
            let is_function_tail = position.is_empty() && index == last_index && returns_non_unit;
            if is_function_tail {
                if let Some(expr) = lower_term_shape_expression(child, context, &child_position) {
                    context.last_assigned_symbol = None;
                    lines.push(expr.text);
                    continue;
                }
            }
            if let Some(child_body) = lower_term_shape_body(child, context, &child_position) {
                if !child_body.is_empty() {
                    lines.push(child_body);
                }
                continue;
            }
            // #1391 follow-on: tolerant seq lowering. When a child can't
            // be lowered (java lift gap), emit a TODO comment instead of
            // failing the whole function. The remaining children still
            // lower; the cycle produces partial output with explicit gaps.
            let expression = match lower_term_shape_expression(child, context, &child_position) {
                Some(e) => e,
                None => {
                    let concept = term_shape_concept_name(child).unwrap_or_else(|| "?".into());
                    lines.push(format!("// TODO(lower): un-lowered {}", concept));
                    continue;
                }
            };
            // Last-child tail-expression form for non-unit returning fns:
            // emit the expression bare (no `let temp = X;` + bare-symbol
            // epilogue wrapper). Byte-correct for shim bodies ending in
            // `match ... { ... }`, `out`, etc.
            if index == last_index && returns_non_unit {
                context.last_assigned_symbol = None;
                lines.push(expression.text);
                continue;
            }
            // Non-last expression child = expression-as-statement. Emit
            // bare (no `let __temp = X;` wrapper) since the substrate's
            // lift doesn't currently track value-consumption distinct from
            // side-effect-only execution. Block-terminated expressions
            // (`match { ... }`, `for { ... }`, `if { ... }`, `loop { ... }`)
            // are valid Rust statements without a trailing `;`; non-block
            // expressions need one. The heuristic: peek at the last
            // non-whitespace character of expression.text to decide.
            context.last_assigned_symbol = None;
            let trimmed_end = expression.text.trim_end();
            let needs_semicolon = !trimmed_end.ends_with('}');
            if needs_semicolon {
                lines.push(format!("{};", expression.text));
            } else {
                lines.push(expression.text);
            }
        }
        // Tail expression: when the function returns non-unit and no
        // explicit `concept:return` appeared inside the seq, emit the
        // last assigned symbol as a bare expression (Rust's implicit
        // return), matching the source-side tail-expression convention.
        // This is byte-correct for shim bodies that end with `out` or
        // similar, NOT with a `return X;` statement.
        // Implicit tail-expression only fires at the function ROOT.
        // Nested seqs (inside for/while/match/if bodies) are statement-
        // groups, not return points — appending a symbol there produces
        // invalid rust like `diagnostics\n let mut ir_entries = ...`.
        if position.is_empty()
            && map_source_type(&context.return_type) != "()"
            && !lines
                .iter()
                .any(|line| line.trim_start().starts_with("return "))
        {
            if let Some(symbol) = context.last_assigned_symbol.as_deref() {
                lines.push(symbol.to_string());
            }
        }
        return Some(lines.join("\n"));
    }
    if concept_name == "concept:assign" || concept_name == "assign" {
        let args = term_shape_args(shape);
        if args.len() < 2 {
            return None;
        }
        let target = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        // #1391 follow-on: tolerate EMPTY value object (the java lift
        // sometimes emits {} when it can't interpret the RHS of a java
        // local declaration). Emit a placeholder comment + skip; the rest
        // of the seq continues lowering instead of dropping to a stub.
        let value =
            match lower_term_shape_expression(args[1], context, &append_position(position, 1)) {
                Some(v) => v,
                None => {
                    let is_empty = args[1].as_object().map(|o| o.is_empty()).unwrap_or(false);
                    if is_empty {
                        context.defined_symbols.insert(target.text.clone());
                        context.last_assigned_symbol = Some(target.text.clone());
                        return Some(format!(
                            "// TODO(lift): empty RHS for `{}` (java lift gap)",
                            target.text
                        ));
                    }
                    return None;
                }
            };
        // `let _ = X;` — wildcard discard binding. Emitted by walk_rpc as
        // concept:assign with target = concept:literal source_text "_".
        // No mutability, no type annotation, no symbol tracked.
        if target.text == "_" {
            return Some(format!("let _ = {};", value.text));
        }
        if !is_rust_identifier(&target.text) {
            return None;
        }
        let already_defined = context.defined_symbols.contains(&target.text);
        context.defined_symbols.insert(target.text.clone());
        context.last_assigned_symbol = Some(target.text.clone());
        if already_defined {
            return Some(format!("{} = {};", target.text, value.text));
        }
        // 3-arg form: args[2] is a concept:literal bool true → `let mut`.
        // 2-arg form: no third arg → `let`.
        let is_mut = args.len() >= 3
            && args[2]
                .get("value")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let let_kw = if is_mut { "let mut" } else { "let" };
        // Explicit `let X: Type = value` annotation preserved from source.
        // Stored on the target symbol leaf by the lift; takes precedence
        // over inferred-from-value type.
        let explicit_let_type = args[0]
            .get("let_type")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(ty) = explicit_let_type {
            // #1391 follow-on: when let_type came from java AST
            // (java.util.*, java.lang.*, Object, etc.), map it to a rust
            // type via java_type_to_rust_let_annotation. The java lift
            // restricts let_type emission to generic-container shapes
            // (ArrayList/TreeSet/HashMap/etc.) which is where rust's
            // type inference can't recover the parameter, so we MUST
            // emit an annotation; otherwise the cycled rust won't
            // compile. For other types the lift omits let_type.
            let mapped = java_type_to_rust_let_annotation(&ty);
            if !mapped.is_empty() {
                return Some(format!(
                    "{} {}: {} = {};",
                    let_kw, target.text, mapped, value.text
                ));
            }
            // The match-assign triplet recognizer in the java lift puts
            // a rust-source-spelled type (e.g. `Value`, `String`, `bool`)
            // directly into let_type. Java-shaped types are handled above;
            // these are emit-ready.
            let looks_rusty = !ty.contains('.') && !ty.contains('[') && !ty.contains("Object");
            if looks_rusty {
                return Some(format!(
                    "{} {}: {} = {};",
                    let_kw, target.text, ty, value.text
                ));
            }
            return Some(format!("{} {} = {};", let_kw, target.text, value.text));
        }
        // Omit type annotation when value.type_name is empty — Rust's local type
        // inference covers it (happens for concept:call results, array-repeat, etc.).
        // Also omit when the type came from java AST (java syntax like
        // `Object[]`, `java.util.List`, etc.) — those aren't valid rust types.
        // Rust's type inference covers the cycle when the value expression
        // is well-formed.
        let ty = &value.type_name;
        let java_like = ty.is_empty()
            || ty.contains('[')
            || ty.contains("java.")
            || ty.contains("com.")
            || ty.contains("Object");
        if java_like {
            return Some(format!("{} {} = {};", let_kw, target.text, value.text));
        }
        return Some(format!(
            "{} {}: {} = {};",
            let_kw, target.text, value.type_name, value.text
        ));
    }
    if concept_name == "concept:item-decl" {
        // Function-local item (const, static, fn). args[0] is a symbol leaf
        // carrying the verbatim source. Emit as-is — items are statement
        // form (token-tree pass-through is byte-identical with source).
        let args = term_shape_args(shape);
        if args.is_empty() {
            return None;
        }
        let source = args[0].get("text").and_then(Value::as_str)?;
        // Re-normalize macro-style spacing (`X : T` → `X: T`) so token-stream
        // artifacts don't appear in the emitted source.
        return Some(normalize_macro_tokens(source));
    }
    if concept_name == "concept:destructure-struct" {
        // args: [value, type_leaf, field_leaf1, field_leaf2, ...]
        let args = term_shape_args(shape);
        if args.len() < 3 {
            return None;
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let type_text = args[1].get("text").and_then(Value::as_str).unwrap_or("");
        // Each field-leaf has text=binding-name and field_name=source-field.
        // For rust source emit: `Type { field1: binding1, field2 }` where
        // we use shorthand `{ field }` if binding-name == field-name.
        let mut field_specs: Vec<String> = Vec::new();
        for f in &args[2..] {
            let binding = f.get("text").and_then(Value::as_str).unwrap_or("");
            let field = f
                .get("field_name")
                .and_then(Value::as_str)
                .unwrap_or(binding);
            context.defined_symbols.insert(binding.to_string());
            if binding == field {
                field_specs.push(binding.to_string());
            } else {
                field_specs.push(format!("{}: {}", field, binding));
            }
        }
        // Source-style multi-line for non-trivial fields.
        return Some(format!(
            "let {} {{\n    {},\n}} = {};",
            type_text,
            field_specs.join(",\n    "),
            value.text
        ));
    }
    if concept_name == "concept:destructure-tuple" {
        // args: [value, name_leaf1, name_leaf2, ...]
        let args = term_shape_args(shape);
        if args.len() < 2 {
            return None;
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let names: Vec<String> = args[1..]
            .iter()
            .filter_map(|n| n.get("text").and_then(Value::as_str).map(String::from))
            .collect();
        if names.is_empty() {
            return None;
        }
        // Track each name as a defined symbol.
        for name in &names {
            context.defined_symbols.insert(name.clone());
        }
        // Last-bound symbol for tail-expression detection.
        if let Some(last) = names.last() {
            context.last_assigned_symbol = Some(last.clone());
        }
        return Some(format!("let ({}) = {};", names.join(", "), value.text));
    }
    if concept_name == "concept:return" || concept_name == "return" {
        let args = term_shape_args(shape);
        if args.is_empty() {
            return Some("return;".to_string());
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(format!("return {};", value.text));
    }
    if concept_name == "concept:break" {
        let args = term_shape_args(shape);
        if args.is_empty() {
            return Some("break;".to_string());
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(format!("break {};", value.text));
    }
    if concept_name == "concept:continue" {
        return Some("continue;".to_string());
    }
    if concept_name == "concept:conditional" || concept_name == "conditional" {
        let args = term_shape_args(shape);
        if args.len() != 3 {
            return None;
        }
        let condition =
            lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let then_body =
            lower_term_shape_branch_body(args[1], context, &append_position(position, 1))?;
        // Omit else-clause when args[2] is concept:skip (the substrate's
        // canonical "no-else" placeholder). Matches source `if cond { body }`
        // byte-identical instead of emitting `if cond { body } else { () }`.
        if term_shape_concept_name(args[2]).as_deref() == Some("concept:skip") {
            return Some(format!(
                "if {} {{\n{}\n}}",
                condition.text,
                indent_block(&then_body)
            ));
        }
        let else_body =
            lower_term_shape_branch_body(args[2], context, &append_position(position, 2))?;
        return Some(format!(
            "if {} {{\n{}\n}} else {{\n{}\n}}",
            condition.text,
            indent_block(&then_body),
            indent_block(&else_body)
        ));
    }
    // concept:call as a bare statement (side-effect call like `obj.method(arg);`).
    // The expression lowering path handles the callee-present case; if it returns None
    // (callee identity absent from this shape), the body lowering also returns None,
    // which propagates the refused result correctly rather than producing a stub.
    if concept_name == "concept:call" || concept_name == "call" {
        let expr = lower_term_shape_expression(shape, context, position)?;
        return Some(format!("{};", expr.text));
    }
    None
}

fn lower_term_shape_branch_body(
    shape: &Value,
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<String> {
    if let Some(body) = lower_term_shape_body(shape, context, position) {
        return Some(body);
    }
    let expression = lower_term_shape_expression(shape, context, position)?;
    Some(format!("return {};", expression.text))
}

/// Catalog-driven kit-op emitter (#1391). Keyed by the rhs op name from
/// realization mementos (target_lang=rust). One small arm per rhs name;
/// the catalog controls WHICH concept maps to which rhs name.
fn emit_kit_rust_op(
    rhs_op_name: &str,
    args: &[&Value],
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<ShapeExpression> {
    match rhs_op_name {
        "rust:str-as-bytes" => {
            if args.is_empty() {
                return None;
            }
            let recv =
                lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
            Some(ShapeExpression {
                text: format!("{}.as_bytes()", recv.text),
                type_name: String::new(),
            })
        }
        "rust:serde-value-as-str" => {
            if args.is_empty() {
                return None;
            }
            let recv =
                lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
            Some(ShapeExpression {
                text: format!("{}.as_str()", recv.text),
                type_name: String::new(),
            })
        }
        "rust:option-is-some" => {
            if args.is_empty() {
                return None;
            }
            let recv =
                lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
            Some(ShapeExpression {
                text: format!("{}.is_some()", recv.text),
                type_name: "bool".to_string(),
            })
        }
        "rust:vec-new" => {
            if !args.is_empty() {
                return None;
            }
            Some(ShapeExpression {
                text: "Vec::new()".to_string(),
                type_name: String::new(),
            })
        }
        "rust:hashmap-new" => {
            if !args.is_empty() {
                return None;
            }
            Some(ShapeExpression {
                text: "HashMap::new()".to_string(),
                type_name: String::new(),
            })
        }
        _ => None,
    }
}

fn lower_term_shape_expression(
    shape: &Value,
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<ShapeExpression> {
    let Some(concept_name) = term_shape_concept_name(shape) else {
        return term_shape_leaf_expression(shape, context, position);
    };
    let args = term_shape_args(shape);
    // concept:literal shapes have a concept_name but are leaf values, not operations.
    // Delegate to term_shape_leaf_expression which handles the `value` / `source_text`
    // fields directly, so literal shapes embedded inside other shapes (e.g. as args
    // of concept:array-repeat) lower correctly without going through the args loop.
    if concept_name == "concept:literal" || concept_name == "literal" {
        return term_shape_leaf_expression(shape, context, position);
    }
    if concept_name == "concept:seq" || concept_name == "seq" {
        lower_term_shape_body(shape, context, position)?;
        return context
            .last_assigned_symbol
            .as_ref()
            .map(|symbol| ShapeExpression {
                text: symbol.clone(),
                type_name: map_source_type(&context.return_type),
            });
    }
    // concept:call: walk_rpc now emits callee identity at args[0].
    //
    // Free function call (Expr::Call):
    //   args[0] = {kind:"path", text:"blake3::Hasher::new"} — callee path leaf
    //   args[1..] = call arguments
    //   output: `blake3::Hasher::new(arg0, arg1, ...)`
    //
    // Method call (Expr::MethodCall):
    //   args[0] = receiver shape
    //   args[1] = {kind:"method", text:"update"} — method ident leaf
    //   args[2..] = call arguments
    //   output: `receiver.update(arg0, arg1, ...)`
    //
    // If callee identity is absent (neither layout applies) → return None
    // so the concept stays refused rather than emitting a wrong body.
    if concept_name == "concept:call" || concept_name == "call" {
        if args.is_empty() {
            return None;
        }
        let first = args[0];
        if first.get("kind").and_then(Value::as_str) == Some("path") {
            // Free function call: args[0] is the callee path leaf.
            let callee_text = first.get("text").and_then(Value::as_str)?;
            let mut call_args: Vec<String> = args[1..]
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    lower_term_shape_expression(arg, context, &append_position(position, i + 1))
                        .map(|e| e.text)
                })
                .collect::<Option<Vec<_>>>()?;
            // #1391 follow-on: callee-signature-aware `&` insertion. The
            // cross-language cycle loses `&x` borrow markers because java
            // has no equivalent. For known boundary callees in
            // libprovekit-rpc-cross-platform whose param shapes are known,
            // add the `&` back. This is a known-callee registry, not a
            // generic solution — but it closes the run_server diff.
            let ref_param_indices: Option<&[usize]> = match callee_text {
                "stderr_write_line" => Some(&[0]),
                "stdout_write_line" => Some(&[0]),
                "json_serialize" => Some(&[0]),
                "json_parse" => Some(&[0]),
                "blake3_512_cid" => Some(&[0]),
                "handle_line" => Some(&[0, 1]),
                "build_ir_document" => Some(&[0, 1]),
                // content_addressed_name, slot_cid, encode_jcs take &-args
                // but their typical callsites pass already-reference values
                // (match-bound or param-bound); skip to avoid double-&.
                _ => None,
            };
            if let Some(idxs) = ref_param_indices {
                for &idx in idxs {
                    if idx >= call_args.len() {
                        continue;
                    }
                    let arg = &call_args[idx];
                    let trimmed = arg.trim();
                    let is_bare_ident = !trimmed.is_empty()
                        && trimmed
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_');
                    let is_format_macro = trimmed.starts_with("format!(");
                    // Skip if the arg names a function PARAM that's already
                    // typed `&T`. Adding `&` produces &&T. Detect by checking
                    // both the (mapped) param_types and (preserved) original
                    // — Value, str, JsonNode all canonically arrive via
                    // reference in this codebase.
                    let original_param_types =
                        CURRENT_ORIGINAL_PARAM_TYPES.with(|v| v.borrow().clone());
                    let is_already_ref = is_bare_ident
                        && context
                            .params
                            .iter()
                            .position(|p| p == trimmed)
                            .map(|pi| {
                                let mapped =
                                    context.param_types.get(pi).cloned().unwrap_or_default();
                                // Prefer rust-source spelling (e.g. `&str`) over
                                // java-mapped (`String`) which loses the reference.
                                let original =
                                    original_param_types.get(pi).cloned().unwrap_or_default();
                                // Heuristic: if mapped type is one of the
                                // canonical pass-by-reference types in this
                                // source (Value, str), treat as ref.
                                original.starts_with('&')
                                    || mapped.starts_with('&')
                                    || mapped == "Value"
                                    || mapped == "str"
                            })
                            .unwrap_or(false);
                    if (is_bare_ident && !is_already_ref) || is_format_macro {
                        call_args[idx] = format!("&{}", arg);
                    }
                }
            }
            // Substrate-canonical tuple literal: the java side emits tuple
            // construction as `concept:call(__provekit_tuple_new, ...)` because
            // there's no concept:tuple-literal in the catalog yet. Round-trip
            // back to rust by emitting a tuple literal `(a, b, c)` — the same
            // surface the rust lifter produced originally.
            if callee_text == "__provekit_tuple_new" {
                let text = format!("({})", call_args.join(", "));
                return Some(ShapeExpression {
                    text,
                    type_name: String::new(),
                });
            }
            let text = format!("{}({})", callee_text, call_args.join(", "));
            // Return empty type_name so the enclosing concept:assign arm omits
            // the type annotation (Rust's local type inference covers it).
            return Some(ShapeExpression {
                text,
                type_name: String::new(),
            });
        }
        // Method call: args[0] is receiver, args[1] is {kind:"method"} leaf,
        // args[2..] are call arguments.
        if args.len() >= 2 && args[1].get("kind").and_then(Value::as_str) == Some("method") {
            let receiver =
                lower_term_shape_expression(first, context, &append_position(position, 0))?;
            let method_text = args[1].get("text").and_then(Value::as_str)?;
            let call_args: Vec<String> = args[2..]
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    lower_term_shape_expression(arg, context, &append_position(position, i + 2))
                        .map(|e| e.text)
                })
                .collect::<Option<Vec<_>>>()?;
            let text = format!(
                "{}.{}({})",
                receiver.text,
                method_text,
                call_args.join(", ")
            );
            // Return empty type_name so the enclosing concept:assign arm omits
            // the type annotation (Rust's local type inference covers it).
            return Some(ShapeExpression {
                text,
                type_name: String::new(),
            });
        }
        // Callee identity absent — refuse.
        return None;
    }
    // concept:array-repeat: [elem; len] — walk_rpc emits args=[elem_shape, len_shape].
    if concept_name == "concept:array-repeat" {
        if args.len() != 2 {
            return None;
        }
        let elem = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let len = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        return Some(ShapeExpression {
            text: format!("[{}; {}]", elem.text, len.text),
            type_name: String::new(),
        });
    }
    // concept:array-literal: java's `new T[]{a, b, c}` lifts here. When the
    // enclosing fn returns a rust tuple `(A,B,...)`, this is the lifted form
    // of `__provekit_tuple_new(a, b)` — the java lower's tuple emit. Emit as
    // rust tuple literal `(a, b)` in tuple-return contexts; else as rust
    // array literal `[a, b]`. The return-type check is purely the function
    // signature's surface form — when the source emitted `(Value, bool)`
    // the realize side sees `(Value,bool)` in context.return_type.
    if concept_name == "concept:array-literal" {
        let elems: Vec<String> = args
            .iter()
            .enumerate()
            .map(|(i, a)| {
                lower_term_shape_expression(a, context, &append_position(position, i))
                    .map(|e| e.text)
            })
            .collect::<Option<Vec<_>>>()?;
        let rt = context.return_type.trim();
        let is_tuple_return = rt.starts_with('(') && rt.ends_with(')');
        let text = if is_tuple_return {
            format!("({})", elems.join(", "))
        } else {
            format!("[{}]", elems.join(", "))
        };
        return Some(ShapeExpression {
            text,
            type_name: String::new(),
        });
    }
    // concept:ref: &expr or &mut expr — walk_rpc emits args=[inner_shape, mutability_leaf].
    // mutability_leaf: {kind:"mutability", text:"mut"} or {kind:"mutability", text:""}.
    if concept_name == "concept:ref" {
        if args.len() != 2 {
            return None;
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let mut_leaf = args[1];
        let mut_text = mut_leaf.get("text").and_then(Value::as_str).unwrap_or("");
        let prefix = if mut_text == "mut" { "&mut " } else { "&" };
        return Some(ShapeExpression {
            text: format!("{}{}", prefix, inner.text),
            type_name: String::new(),
        });
    }
    // concept:closure: |param1, param2, ...| body — walk_rpc emits
    // args = [body_shape, param1_literal, param2_literal, ...]. Body is
    // at args[0]; each param is a concept:literal with source_text
    // carrying the ident name.
    if concept_name == "concept:closure" {
        if args.is_empty() {
            return None;
        }
        let body_shape = args[0];
        let mut param_names: Vec<String> = Vec::with_capacity(args.len().saturating_sub(1));
        for param in args.iter().skip(1) {
            // Closure params are now symbol leaves (kind=symbol, text=name).
            let Some(text) = param.get("text").and_then(Value::as_str) else {
                // Legacy source_text path (un-re-minted shims).
                if let Some(legacy) = param.get("source_text").and_then(Value::as_str) {
                    param_names.push(legacy.to_string());
                    continue;
                }
                return None;
            };
            param_names.push(text.to_string());
        }
        let body = lower_term_shape_expression(body_shape, context, &append_position(position, 0))?;
        // Source-form choice: when the lift marked closure_block_body=true
        // (source had `|e| { ... }`), wrap the body in braces so the lower
        // emits `|e| { body }`. rustfmt can then split long lines inside.
        // Without the marker emit the expression form `|e| body`.
        let is_block_body = body_shape
            .get("closure_block_body")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = if is_block_body {
            // Block-form closure: put body on its own line so rustfmt
            // can normalize layout (break long lines, indent body).
            // The single-line form `|e| { body }` blocks rustfmt's
            // line-breaking inside the closure body.
            format!("|{}| {{\n{}\n}}", param_names.join(", "), body.text)
        } else {
            format!("|{}| {}", param_names.join(", "), body.text)
        };
        return Some(ShapeExpression {
            text,
            type_name: String::new(),
        });
    }
    // Structural control flow + match + macro lowerings — substrate-canonical
    // operators minted 2026-05-21. Replace source_text fallbacks.
    if concept_name == "concept:conditional" || concept_name == "conditional" {
        if args.len() < 2 {
            return None;
        }
        let cond = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let then_text = lower_block_or_expr(args[1], context, &append_position(position, 1))?;
        let else_text = if args.len() >= 3 {
            if term_shape_concept_name(args[2]).as_deref() == Some("concept:skip") {
                None
            } else {
                Some(lower_block_or_expr(
                    args[2],
                    context,
                    &append_position(position, 2),
                )?)
            }
        } else {
            None
        };
        let text = match else_text {
            Some(e) => format!("if {} {{ {} }} else {{ {} }}", cond.text, then_text, e),
            None => format!("if {} {{ {} }}", cond.text, then_text),
        };
        return Some(ShapeExpression {
            text,
            type_name: String::new(),
        });
    }
    if concept_name == "concept:while" {
        if args.len() != 2 {
            return None;
        }
        let cond = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let body = lower_block_or_expr(args[1], context, &append_position(position, 1))?;
        return Some(ShapeExpression {
            text: format!("while {} {{ {} }}", cond.text, body),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:while-let" {
        // args: pattern_leaf, value, body
        if args.len() != 3 {
            return None;
        }
        let pattern_text = args[0].get("text").and_then(Value::as_str)?;
        let value = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let body = lower_block_or_expr(args[2], context, &append_position(position, 2))?;
        return Some(ShapeExpression {
            text: format!("while let {} = {} {{ {} }}", pattern_text, value.text, body),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:if-let" {
        // args: pattern_leaf, value, then, else
        if args.len() != 4 {
            return None;
        }
        let pattern_text = args[0].get("text").and_then(Value::as_str)?;
        let value = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let then_text =
            lower_term_shape_branch_body(args[2], context, &append_position(position, 2))?;
        // Omit else when args[3] is concept:skip (source had no else clause).
        if term_shape_concept_name(args[3]).as_deref() == Some("concept:skip") {
            return Some(ShapeExpression {
                text: format!(
                    "if let {} = {} {{ {} }}",
                    pattern_text, value.text, then_text
                ),
                type_name: String::new(),
            });
        }
        let else_text =
            lower_term_shape_branch_body(args[3], context, &append_position(position, 3))?;
        return Some(ShapeExpression {
            text: format!(
                "if let {} = {} {{ {} }} else {{ {} }}",
                pattern_text, value.text, then_text, else_text
            ),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:for-each" {
        if args.len() != 3 {
            return None;
        }
        let var_text = args[0].get("text").and_then(Value::as_str)?.to_string();
        // Preserve `for mut X in ...` mutability marker from lift.
        let is_mut = args[0].get("mut").and_then(Value::as_bool).unwrap_or(false);
        // Detect explicit ref pattern from lift: `& b` from syn::Pat::Reference.
        let already_ref_pat = var_text.trim_start().starts_with('&');
        let iter = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let body = lower_block_or_expr(args[2], context, &append_position(position, 2))?;
        // #1391 follow-on: ref-pattern inference. When the loop body uses
        // `var` in primitive-numeric position (>>, &, +, -, * as int, etc.)
        // and the iterable isn't already borrowed, prefer `for &var in &iter`
        // to match the idiomatic rust source form. Heuristic: scan body for
        // patterns like `(var)` followed by binary ops or as-cast that
        // require primitive (not &u8) semantics.
        let bare_var = var_text.trim_start_matches('&').trim().to_string();
        let needs_ref_pat = !already_ref_pat
            && !is_mut
            && body_uses_var_as_primitive(&body, &bare_var)
            && !iter.text.starts_with('&');
        let (final_var, final_iter) = if needs_ref_pat {
            (format!("&{}", bare_var), format!("&{}", iter.text))
        } else if already_ref_pat {
            (var_text, iter.text)
        } else {
            let v = if is_mut {
                format!("mut {}", bare_var)
            } else {
                bare_var
            };
            (v, iter.text)
        };
        return Some(ShapeExpression {
            text: format!("for {} in {} {{ {} }}", final_var, final_iter, body),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:for" {
        // Classic for(init; cond; step; body) — rare in rust source; emit
        // as a decomposed seq+while for substrate honesty.
        if args.len() != 4 {
            return None;
        }
        let init = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let cond = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let step = lower_term_shape_expression(args[2], context, &append_position(position, 2))?;
        let body = lower_term_shape_expression(args[3], context, &append_position(position, 3))?;
        return Some(ShapeExpression {
            text: format!(
                "{{ {}; while {} {{ {}; {}; }} }}",
                init.text, cond.text, body.text, step.text
            ),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:match" {
        if args.is_empty() {
            return None;
        }
        let scrutinee =
            lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let mut arms_text: Vec<String> = Vec::with_capacity(args.len().saturating_sub(1));
        for (i, arm) in args.iter().enumerate().skip(1) {
            let arm_args = term_shape_args(arm);
            if arm_args.len() != 2 {
                return None;
            }
            let pattern_text = arm_args[0].get("text").and_then(Value::as_str)?.to_string();
            let body_text = lower_block_or_expr(
                arm_args[1],
                context,
                &append_position(&append_position(position, i), 1),
            )?;
            // Match arms can be expression-form (`=> expr,`) or block-form
            // (`=> { stmts; expr },`). Source-form preservation:
            //   - Single statement that's a return → block-form
            //     `=> { return X; }` matches source byte-identical.
            //   - Otherwise expression-form `=> expr,` (strip trailing `;`).
            let trimmed = body_text.trim_end();
            let is_return_stmt = trimmed.starts_with("return ")
                && trimmed.ends_with(';')
                && !trimmed[7..trimmed.len() - 1].contains(';');
            if is_return_stmt {
                arms_text.push(format!(
                    "{} => {{\n{}\n}},",
                    pattern_text,
                    indent_block(trimmed)
                ));
            } else {
                let body_trimmed = trimmed.trim_end_matches(';');
                arms_text.push(format!("{} => {},", pattern_text, body_trimmed));
            }
        }
        return Some(ShapeExpression {
            text: format!("match {} {{ {} }}", scrutinee.text, arms_text.join(" ")),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:macro-call" {
        // args[0] = path leaf (macro name); args[1] = symbol leaf (macro tokens).
        if args.len() != 2 {
            return None;
        }
        let path = args[0].get("text").and_then(Value::as_str)?;
        let tokens = args[1].get("text").and_then(Value::as_str)?;
        // syn's to_token_stream normalizes spacing around `:` to
        // `key : value`. Normalize back to `key: value` so json!{}
        // and similar map-body macros byte-compare against source
        // after rustfmt (which won't reformat macro bodies by default).
        let normalized = normalize_macro_tokens(tokens);
        // For json! (and similar map-body macros) the source layout is
        // multi-line: { "k": v, "k2": v2, }. rustfmt on stable doesn't
        // format inside macro bodies. Pretty-print here so byte-comparison
        // post-format matches source.
        let pretty = if path == "json" {
            pretty_print_macro_body(&normalized)
        } else if path == "format"
            || path == "println"
            || path == "eprintln"
            || path == "write"
            || path == "writeln"
        {
            // Comma-separated arg list. Break across lines when EITHER
            // total exceeds 100 chars OR the format-string first arg is
            // long enough that the source likely wrote it multi-line.
            // Threshold of 60 catches typical multi-arg format! calls
            // with non-trivial format strings (which is when rustfmt
            // would break too if it formatted macros).
            let total_len = path.len() + 2 + normalized.len();
            let has_multi_arg = normalized.contains(',');
            if total_len > 100 || (total_len > 60 && has_multi_arg) {
                pretty_print_macro_args(&normalized)
            } else {
                normalized
            }
        } else {
            normalized
        };
        return Some(ShapeExpression {
            text: format!("{}!({})", path, pretty),
            type_name: String::new(),
        });
    }
    // Carrier-canonical concepts that the java lift emits when it
    // recognizes Result.ok/.err/.okOrElse / SumVariant constructor /
    // Substrate.tryUnwrap. Map back to native rust syntax for byte-
    // identical round-trip.
    if concept_name == "concept:fallible-ok" {
        if args.is_empty() {
            return Some(ShapeExpression {
                text: "Ok(())".to_string(),
                type_name: String::new(),
            });
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("Ok({})", inner.text),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:fallible-err" {
        if args.is_empty() {
            return Some(ShapeExpression {
                text: "Err(())".to_string(),
                type_name: String::new(),
            });
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("Err({})", inner.text),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:sum-variant-construct" {
        // args: [family_text, variant_text, payload]
        if args.len() < 3 {
            return None;
        }
        // family/variant may be string-literal expressions; extract text.
        let family = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let variant = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let payload = lower_term_shape_expression(args[2], context, &append_position(position, 2))?;
        // Strip the surrounding quotes from family/variant if they're
        // string literals (they came as "LiftError" / "Internal").
        let family_clean = family.text.trim_matches('"').to_string();
        let variant_clean = variant.text.trim_matches('"').to_string();
        return Some(ShapeExpression {
            text: format!("{}::{}({})", family_clean, variant_clean, payload.text),
            type_name: family_clean,
        });
    }
    if concept_name == "concept:fallible-ok-or-else" {
        // Result.okOrElse(value, errSupplier) → `value.ok_or_else(closure)`
        if args.len() != 2 {
            return None;
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let supplier =
            lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        return Some(ShapeExpression {
            text: format!("{}.ok_or_else({})", value.text, supplier.text),
            type_name: String::new(),
        });
    }
    // catalog-driven operation realization dispatch (#1391).
    // Look up concept in realizations[target_lang=rust]; if present,
    // delegate to the kit-op emitter keyed by rhs op name. The catalog at
    // menagerie/concept-shapes/catalog/realizations/ is the single source
    // of truth: concept_name → rhs_op_name. emit_kit_rust_op below is the
    // only remaining kit-specific code, keyed by catalog'd rhs names.
    if let Some(rhs_op) = operation_realization_catalog::rust_op_for(&concept_name) {
        if let Some(emitted) = emit_kit_rust_op(&rhs_op, &args, context, position) {
            return Some(emitted);
        }
    }
    if concept_name == "concept:value-clone" {
        // Substrate.cloneOf(x) → x.cloned() (rust source-form)
        if args.is_empty() {
            return None;
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("{}.cloned()", inner.text),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:try-unwrap" {
        // Substrate.tryUnwrap(x) — alternate name for concept:try.
        if args.is_empty() {
            return None;
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("{}?", inner.text),
            type_name: inner.type_name,
        });
    }
    if concept_name == "concept:try" {
        // First-class `expr?` operator. The substrate's concept:try
        // (args=[inner]) is the rust source's Try-operator form;
        // realize emits `inner?` for byte-identical rust round-trip.
        // Target languages without `?` translate via method:try_unwrap
        // mapping in their respective realize plugins.
        if args.is_empty() {
            return None;
        }
        let body = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("{}?", body.text),
            type_name: body.type_name,
        });
    }
    if concept_name == "concept:throw" {
        if args.is_empty() {
            // Bare throw (rare in rust — panic!() with no msg).
            return Some(ShapeExpression {
                text: "panic!()".to_string(),
                type_name: "()".to_string(),
            });
        }
        let inner = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(ShapeExpression {
            text: format!("panic!({})", inner.text),
            type_name: "()".to_string(),
        });
    }
    if concept_name == "concept:skip" {
        return Some(ShapeExpression {
            text: String::new(),
            type_name: "()".to_string(),
        });
    }
    if concept_name == "concept:cast" {
        if args.len() != 2 {
            return None;
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let type_text = args[1].get("text").and_then(Value::as_str)?;
        // Java-only types (Object[], java.util.List, etc.) have no rust
        // equivalent — drop the cast and emit just the value. The
        // surrounding context's type inference handles it.
        if type_text.contains('[') || type_text.contains("java.") || type_text.contains("Object") {
            return Some(value);
        }
        // Paren-wrap the operand when it's a non-atomic expression.
        // `as` binds tighter than `>>`, `&`, etc., so `b >> 4 as usize`
        // parses as `b >> (4 as usize)` — semantically different from
        // `(b >> 4) as usize`. paren_for_op detects top-level operators
        // and wraps accordingly.
        let value_text = paren_for_op(&value.text);
        return Some(ShapeExpression {
            text: format!("{} as {}", value_text, type_text),
            type_name: type_text.to_string(),
        });
    }
    if concept_name == "concept:index" {
        if args.len() != 2 {
            return None;
        }
        let receiver =
            lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let index = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        return Some(ShapeExpression {
            text: format!("{}[{}]", receiver.text, index.text),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:field" {
        if args.len() != 2 {
            return None;
        }
        let receiver =
            lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let field_text = args[1].get("text").and_then(Value::as_str)?;
        return Some(ShapeExpression {
            text: format!("{}.{}", receiver.text, field_text),
            type_name: String::new(),
        });
    }
    let mut arg_terms = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        arg_terms.push(lower_term_shape_expression(
            arg,
            context,
            &append_position(position, index),
        )?);
    }
    let expression = operation_expression(&concept_name, &arg_terms)?;
    Some(ShapeExpression {
        text: expression,
        type_name: operation_return_type(&concept_name, &arg_terms, &context.return_type),
    })
}

fn term_shape_leaf_expression(
    shape: &Value,
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<ShapeExpression> {
    if let Some(symbol) = context.operand_bindings.get(position) {
        return Some(symbol_term(symbol, context));
    }
    if shape.get("kind").and_then(Value::as_str) == Some("var") {
        if let Some(name) = shape.get("name").and_then(Value::as_str) {
            return Some(ShapeExpression {
                text: name.to_string(),
                type_name: type_for_argument(name, context),
            });
        }
    }
    // Literal leaf: concept:literal shape emitted by walk_rpc's literal_shape fn.
    // Substrate-canonical shape: (sort, value, optional integer_width).
    // source_text dropped from substrate channel (2026-05-21); the rust
    // realize binary reconstructs rust source from value+sort.
    if shape.get("kind").and_then(Value::as_str) == Some("literal") || shape.get("value").is_some()
    {
        let width = shape.get("integer_width").and_then(Value::as_str);
        let radix = shape.get("radix").and_then(Value::as_str);
        return Some(literal_term_with_width_and_radix(
            shape.get("value").unwrap_or(&Value::Null),
            width,
            radix,
        ));
    }
    // Legacy source_text fallback — only kept for backwards compat with
    // un-re-minted shim envelopes. After all shims re-minted post-2026-05-21,
    // this branch should never fire on shim sources.
    if let Some(source_text) = shape.get("source_text").and_then(Value::as_str) {
        return Some(ShapeExpression {
            text: source_text.to_string(),
            type_name: String::new(),
        });
    }
    // Leaf kinds emitted by walk_rpc that carry their text verbatim:
    // kind:"path"        → free function callee (e.g. "blake3::Hasher::new")
    // kind:"method"      → method ident (e.g. "update")
    // kind:"mutability"  → "mut" or "" inside concept:ref
    // kind:"symbol"      → identifier binding (closure param, wildcard "_",
    //                      match-arm pattern text, etc.)
    // Return the text verbatim — NOT through literal_term which would quote it.
    if let Some(kind) = shape.get("kind").and_then(Value::as_str) {
        if kind == "path" || kind == "method" || kind == "mutability" || kind == "symbol" {
            if let Some(text) = shape.get("text").and_then(Value::as_str) {
                return Some(ShapeExpression {
                    text: text.to_string(),
                    type_name: String::new(),
                });
            }
        }
    }
    context.fallback_leaf()
}

fn operation_expression(concept_name: &str, args: &[ShapeExpression]) -> Option<String> {
    let op = concept_name
        .strip_prefix("concept:")
        .unwrap_or(concept_name);
    if args.len() == 2 {
        let left = paren_for_op(&args[0].text);
        let right = paren_for_op(&args[1].text);
        return match op {
            "add" => Some(format!("{left} + {right}")),
            "sub" => Some(format!("{left} - {right}")),
            "mul" => Some(format!("{left} * {right}")),
            "div" => Some(format!("{left} / {right}")),
            "mod" => Some(format!("{left} % {right}")),
            "eq" => Some(format!("{left} == {right}")),
            "ne" => Some(format!("{left} != {right}")),
            "lt" => Some(format!("{left} < {right}")),
            "le" => Some(format!("{left} <= {right}")),
            "gt" => Some(format!("{left} > {right}")),
            "ge" => Some(format!("{left} >= {right}")),
            "and" => Some(format!("{left} && {right}")),
            "or" => Some(format!("{left} || {right}")),
            "bitand" => Some(format!("{left} & {right}")),
            "bitor" => Some(format!("{left} | {right}")),
            "bitxor" => Some(format!("{left} ^ {right}")),
            "shl" => Some(format!("{left} << {right}")),
            "shr" => Some(format!("{left} >> {right}")),
            _ => None,
        };
    }
    if args.len() == 1 {
        let value = paren_for_op(&args[0].text);
        return match op {
            "neg" => Some(format!("-{value}")),
            "not" => Some(format!("!{value}")),
            "bitnot" => Some(format!("!({value})")),
            _ => None,
        };
    }
    None
}

fn operation_return_type(
    concept_name: &str,
    args: &[ShapeExpression],
    fallback_return_type: &str,
) -> String {
    match concept_name
        .strip_prefix("concept:")
        .unwrap_or(concept_name)
    {
        "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "and" | "or" | "not" => "bool".to_string(),
        _ => args
            .first()
            .map(|arg| arg.type_name.clone())
            .unwrap_or_else(|| map_source_type(fallback_return_type)),
    }
}

fn operand_binding_map(operand_bindings: &[Value]) -> BTreeMap<Vec<usize>, String> {
    let mut out = BTreeMap::new();
    for binding in operand_bindings {
        let Some(position) = binding.get("position").and_then(Value::as_array) else {
            continue;
        };
        let Some(symbol) = binding.get("symbol").and_then(Value::as_str) else {
            continue;
        };
        let mut parts = Vec::new();
        let mut valid = true;
        for part in position {
            if let Some(value) = part.as_u64().and_then(|value| usize::try_from(value).ok()) {
                parts.push(value);
            } else {
                valid = false;
                break;
            }
        }
        if valid {
            out.insert(parts, symbol.to_string());
        }
    }
    out
}

fn symbol_term(symbol: &str, context: &ShapeLoweringContext) -> ShapeExpression {
    if matches!(symbol, "true" | "True") {
        return ShapeExpression {
            text: "true".to_string(),
            type_name: "bool".to_string(),
        };
    }
    if matches!(symbol, "false" | "False") {
        return ShapeExpression {
            text: "false".to_string(),
            type_name: "bool".to_string(),
        };
    }
    if symbol.parse::<i64>().is_ok() {
        return ShapeExpression {
            text: symbol.to_string(),
            type_name: "i64".to_string(),
        };
    }
    if symbol.starts_with('"') && symbol.ends_with('"') {
        return ShapeExpression {
            text: symbol.to_string(),
            type_name: "String".to_string(),
        };
    }
    ShapeExpression {
        text: symbol.to_string(),
        type_name: type_for_argument(symbol, context),
    }
}

/// Try body-lowering first (for concept:seq / concept:assign / concept:return
/// statement-shapes); fall back to expression-lowering on single-expression
/// bodies. Used for the body slot of structural operators (while / for-each /
/// conditional / match-arm) where the body may be either a block or a single
/// expression.
fn lower_block_or_expr(
    shape: &Value,
    context: &mut ShapeLoweringContext,
    position: &[usize],
) -> Option<String> {
    if let Some(body) = lower_term_shape_body(shape, context, position) {
        return Some(body);
    }
    let expr = lower_term_shape_expression(shape, context, position)?;
    Some(expr.text)
}

/// Lower a concept:literal value to a rust source-spelling, including the
/// integer width suffix when present. Replaces source_text-based reconstruction
/// (which leaked kit-internal source into substrate state).
fn literal_term_with_width(value: &Value, integer_width: Option<&str>) -> ShapeExpression {
    literal_term_with_width_and_radix(value, integer_width, None)
}

fn literal_term_with_width_and_radix(
    value: &Value,
    integer_width: Option<&str>,
    radix: Option<&str>,
) -> ShapeExpression {
    match value {
        Value::Bool(value) => ShapeExpression {
            text: value.to_string(),
            type_name: "bool".to_string(),
        },
        Value::Number(value) => {
            // Re-attach integer width suffix when declared. "inferred" / None
            // → bare number (rust infers from context).
            let width = integer_width.unwrap_or("inferred");
            let suffix = if width == "inferred" || width.is_empty() {
                String::new()
            } else {
                width.to_string()
            };
            let type_name = if width.is_empty() || width == "inferred" {
                "i64".to_string()
            } else {
                width.to_string()
            };
            // Preserve source radix when declared. The lift carries radix
            // because base10 reproduction would normalize `0x0F` to `15`
            // — semantically equivalent but not byte-identical for round-trip.
            let number_text = match radix {
                Some("hex") => {
                    if let Some(n) = value.as_i64() {
                        format!("0x{:02X}", n)
                    } else if let Some(n) = value.as_u64() {
                        format!("0x{:02X}", n)
                    } else {
                        value.to_string()
                    }
                }
                Some("oct") => {
                    if let Some(n) = value.as_i64() {
                        format!("0o{:o}", n)
                    } else {
                        value.to_string()
                    }
                }
                Some("bin") => {
                    if let Some(n) = value.as_i64() {
                        format!("0b{:b}", n)
                    } else {
                        value.to_string()
                    }
                }
                _ => value.to_string(),
            };
            ShapeExpression {
                text: format!("{number_text}{suffix}"),
                type_name,
            }
        }
        Value::String(value) => {
            // Substrate-canonical string literal: rust raw form. (Char/string
            // disambiguation lost when concept:Char isn't minted as a distinct
            // sort — for now all string-typed literals emit as rust string
            // literals. Single-char patterns embedded in match arms emit via
            // the match-arm pattern path, not here.)
            ShapeExpression {
                text: format!("{:?}", value),
                type_name: "&str".to_string(),
            }
        }
        _ => ShapeExpression {
            text: "()".to_string(),
            type_name: "()".to_string(),
        },
    }
}

fn literal_term(value: &Value) -> ShapeExpression {
    match value {
        Value::Bool(value) => ShapeExpression {
            text: value.to_string(),
            type_name: "bool".to_string(),
        },
        Value::Number(value) => ShapeExpression {
            text: value.to_string(),
            type_name: "i64".to_string(),
        },
        Value::String(value) => ShapeExpression {
            text: format!("{:?}.to_string()", value),
            type_name: "String".to_string(),
        },
        _ => ShapeExpression {
            text: "()".to_string(),
            type_name: "()".to_string(),
        },
    }
}

fn type_for_argument(symbol: &str, context: &ShapeLoweringContext) -> String {
    context
        .params
        .iter()
        .position(|param| param == symbol)
        .and_then(|index| context.param_types.get(index))
        .map(|ty| map_source_type(ty))
        .unwrap_or_else(|| map_source_type(&context.return_type))
}

fn append_position(position: &[usize], next: usize) -> Vec<usize> {
    let mut out = position.to_vec();
    out.push(next);
    out
}

fn is_rust_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn indent_block(body: &str) -> String {
    let lines = body.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return "    ()".to_string();
    }
    lines
        .into_iter()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn term_shape_concept_name(shape: &Value) -> Option<String> {
    shape
        .get("concept_name")
        .or_else(|| shape.get("conceptName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn term_shape_args(shape: &Value) -> Vec<&Value> {
    shape
        .get("args")
        .and_then(Value::as_array)
        .map(|args| args.iter().collect())
        .unwrap_or_default()
}

fn term_shape_comment_surface(shape: &Value) -> Option<&str> {
    term_shape_args(shape)
        .first()
        .and_then(|arg| arg.get("value"))
        .and_then(Value::as_str)
        .or(Some(""))
}

fn rust_comment_body(surface: &str) -> String {
    let trimmed = surface.trim();
    if trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.ends_with("*/")) {
        return trimmed.to_string();
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        return format!("// {}", rest.trim());
    }
    if trimmed.contains('\n') {
        return format!("/*\n{}\n*/", trimmed);
    }
    format!("// {trimmed}")
}

fn apply_proc_macro_invocations(
    mut realization: Realization,
    proc_macro_invocations: &[Value],
) -> Realization {
    let prefix = proc_macro_attribute_prefix(proc_macro_invocations);
    if prefix.is_empty() {
        return realization;
    }
    realization.source = format!("{prefix}{}", realization.source);
    realization.emitted_artifact_cid = blake3_512_of(realization.source.as_bytes());
    realization
}

fn proc_macro_attribute_prefix(proc_macro_invocations: &[Value]) -> String {
    proc_macro_invocations
        .iter()
        .filter_map(proc_macro_attribute_token_stream)
        .map(|token_stream| format!("{token_stream}\n"))
        .collect()
}

fn proc_macro_attribute_token_stream(invocation: &Value) -> Option<String> {
    let token_stream = invocation.get("token_stream")?.as_str()?.trim();
    if token_stream.starts_with("#[") && token_stream.ends_with(']') {
        Some(token_stream.to_string())
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct SugarCarrierSpec {
    concept_name: &'static str,
    concept_cid: &'static str,
    operation_kind: &'static str,
    loss_name: &'static str,
}

const SUGAR_CARRIER_SPECS: &[SugarCarrierSpec] = &[
    SugarCarrierSpec {
        concept_name: "concept:postdec",
        concept_cid: "blake3-512:cac33b2bef01e38d327440e7bfecebf3e7540d463a02e68dd047e47d0c9cca45f94181ce773fb389671a960cc957760b540b2927afd6d2c624cf9ddaca225f1a",
        operation_kind: "postdec",
        loss_name: "rust-no-postfix-decrement",
    },
    SugarCarrierSpec {
        concept_name: "concept:postinc",
        concept_cid: "blake3-512:be615743882f980a2fde0ca6ec3250305c28e2fac1fe4d17accd1790d62af7992ff80282f6507335b959ccceaa32a047f1845b8a9e96a54d20b3766d46589aee",
        operation_kind: "postinc",
        loss_name: "rust-no-postfix-increment",
    },
    SugarCarrierSpec {
        concept_name: "concept:predec",
        concept_cid: "blake3-512:fa83fc84643e03f1e60aa66848412e0cdc25ad6ede0cf216643fb8d4dbe52c4d8df28283f754040cc0f53a62ec22e73a2db623e6507055ab1076df8394024995",
        operation_kind: "predec",
        loss_name: "rust-no-prefix-decrement",
    },
    SugarCarrierSpec {
        concept_name: "concept:preinc",
        concept_cid: "blake3-512:8c8383c221eaca3b95d30437d768065d5117091415afb04e92f541af6fb26d37af79d423e25a59ffaf3f6e2d654d0bd64cfe8e071ee5483ed6bca2614442001f",
        operation_kind: "preinc",
        loss_name: "rust-no-prefix-increment",
    },
    SugarCarrierSpec {
        concept_name: "concept:throw",
        concept_cid: "blake3-512:bfca9b128ea5128d15236ebbe44150ff60355b9bbcd664ae4abbc34f2e4e658f7441089449956bfdb333d1f2eb1bff828c74a5d2f3df7fec723abe883bb81a12",
        operation_kind: "throw",
        loss_name: "rust-result-not-throw",
    },
    SugarCarrierSpec {
        concept_name: "concept:new",
        concept_cid: "blake3-512:26eb0a9484d68fff3fafe1ee82f09e3c3f49e1e2d1e8d01c733362b39473590e61f5903080ffdf69f2532e57047d0fbd4439a11ff778936e27a61f0c4c8c35b8",
        operation_kind: "new",
        loss_name: "rust-no-new-keyword",
    },
    SugarCarrierSpec {
        concept_name: "concept:ushr",
        concept_cid: "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad",
        operation_kind: "ushr",
        loss_name: "rust-unified-shr",
    },
    SugarCarrierSpec {
        concept_name: "concept:source-unit",
        concept_cid: "blake3-512:377bec17d4c9ea2e44216e244685c282b3ac83c19191699eab94a47ff0b123bf4899d6b5691ce88fa7bc70d4dd9f8d2566631bd02895f891ec67ca6d32a87285",
        operation_kind: "source-unit",
        loss_name: "rust-crate-source-unit",
    },
];

fn emit_sugar_carrier(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Option<Realization> {
    let spec = sugar_carrier_spec(concept_name)?;
    let loss_record = loss_record_for(spec.loss_name);
    let loss_record_cid = cid_for_json(&loss_record);
    let sugar_dict = sugar_dict_for(spec, &loss_record);
    let sugar_dict_cid = cid_for_json(&sugar_dict);
    let used_sugar = used_sugar_for(&sugar_dict, &sugar_dict_cid);
    let args_jcs = json!([]);
    let payload = json!({
        "args_jcs": args_jcs,
        "args_jcs_cid": cid_for_json(&args_jcs),
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": spec.concept_cid,
        "concept_name": spec.concept_name,
        "concept_site_cid": spec.concept_cid,
        "emitted_by": {
            "kit_cid": sugar_dict_cid,
            "kit_id": format!("provekit-realize-rust-core@{}", env!("CARGO_PKG_VERSION")),
            "kit_kind": "realize",
            "target_language": "rust"
        },
        "loss_record_cid": loss_record_cid,
        "operation_kind": spec.operation_kind,
        "schema_version": "1",
        "shape_cid": spec.concept_cid,
        "sugar_dict_cid": sugar_dict_cid,
        "term_position": [0]
    });
    let payload_jcs = jcs_for_json(&payload);
    let payload_cid = blake3_512_of(payload_jcs.as_bytes());
    let message = format!("provekit concept carrier: {}", spec.concept_name);
    let body = format!(
        "// provekit-concept: {payload_jcs}\n// provekit-concept-payload-cid: {payload_cid}\npanic!({})",
        rust_string_literal(&message)
    );
    Some(emit_function_with_evidence(
        function,
        params,
        param_types,
        return_type,
        &body,
        loss_record,
        vec![used_sugar],
    ))
}

fn sugar_carrier_spec(concept_name: &str) -> Option<SugarCarrierSpec> {
    let normalized = normalize_concept_name(concept_name);
    SUGAR_CARRIER_SPECS
        .iter()
        .copied()
        .find(|spec| spec.concept_name == normalized)
}

fn normalize_concept_name(concept_name: &str) -> String {
    if concept_name.starts_with("concept:") {
        concept_name.to_string()
    } else {
        format!("concept:{concept_name}")
    }
}

fn loss_record_for(loss_name: &str) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        loss_name.to_string(),
        json!({
            "args": [],
            "head": "atomic",
            "name": loss_name
        }),
    );
    json!({
        "loss_record_contribution": {
            "form": "literal",
            "value": Value::Object(value)
        }
    })
}

fn sugar_dict_for(spec: SugarCarrierSpec, loss_record: &Value) -> Value {
    let realization = serde_json::to_value(tag_sugar_carrier(spec.concept_name))
        .unwrap_or_else(|_| json!({"kind": "sugar-carrier"}));
    json!({
        "concept_name": spec.concept_name,
        "kind": "concept-citation-sugar-carrier",
        "loss_record_contribution": loss_record.get("loss_record_contribution").cloned().unwrap_or_else(|| json!({})),
        "realization": realization,
        "target_language": "rust"
    })
}

fn used_sugar_for(sugar_dict: &Value, sugar_dict_cid: &str) -> Value {
    let mut value = sugar_dict.clone();
    if let Value::Object(map) = &mut value {
        map.insert(
            "header".to_string(),
            json!({
                "cid": sugar_dict_cid,
                "kind": "sugar"
            }),
        );
    }
    value
}

fn cid_for_json(value: &Value) -> String {
    let jcs = jcs_for_json(value);
    blake3_512_of(jcs.as_bytes())
}

fn jcs_for_json(value: &Value) -> String {
    let canonical = cvalue_from_json(value);
    encode_jcs(&canonical)
}

fn cvalue_from_json(value: &Value) -> CValue {
    match value {
        Value::Null => CValue::Null,
        Value::Bool(value) => CValue::Bool(*value),
        Value::Number(value) => CValue::Integer(value.as_i64().unwrap_or(0)),
        Value::String(value) => CValue::String(value.clone()),
        Value::Array(items) => CValue::Array(
            items
                .iter()
                .map(|item| std::sync::Arc::new(cvalue_from_json(item)))
                .collect(),
        ),
        Value::Object(map) => CValue::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), std::sync::Arc::new(cvalue_from_json(value))))
                .collect(),
        ),
    }
}

fn lower_resolved_body(term: &Value, params: &[String]) -> Result<String, String> {
    let concept_name = resolved_concept_name(term);
    if concept_name != "return" {
        return Err(concept_name);
    }

    let args = resolved_args(term).ok_or(concept_name)?;
    if args.len() != 1 {
        return Err("return".to_string());
    }
    lower_resolved_expr(&args[0], params)
}

fn lower_resolved_expr(term: &Value, params: &[String]) -> Result<String, String> {
    match resolved_concept_name(term).as_str() {
        "call:new" => lower_call_new(term, params),
        "literal" => lower_literal(term),
        concept_name => Err(concept_name.to_string()),
    }
}

fn lower_call_new(term: &Value, params: &[String]) -> Result<String, String> {
    let args = resolved_args(term).ok_or_else(|| "call:new".to_string())?;
    if args.len() != 2 {
        return Err("call:new".to_string());
    }

    let name = lower_literal(&args[0])?;
    let lowered_args = lower_call_new_args_literal(&args[1], params)?;
    if name != "new" && !name.ends_with("::new") {
        return Err("call:new".to_string());
    }

    Ok(format!("{name}({lowered_args})"))
}

fn lower_call_new_args_literal(term: &Value, params: &[String]) -> Result<String, String> {
    let node = resolved_node(term).ok_or_else(|| "literal".to_string())?;
    if node.get("kind").and_then(Value::as_str) != Some("literal") {
        return Err(resolved_concept_name(term));
    }

    match node.get("value") {
        Some(Value::Array(items)) if is_value_null_literal(items) => Ok("Value::Null".to_string()),
        Some(Value::Array(items)) => lower_single_value_variant_application(items, params)
            .ok_or_else(|| "literal".to_string()),
        _ => Err("literal".to_string()),
    }
}

fn lower_single_value_variant_application(items: &[Value], params: &[String]) -> Option<String> {
    if items.len() != 1 {
        return None;
    }

    let surface = items[0].as_str()?;
    let call = surface.strip_prefix("call:")?;
    let (ctor_name, rest) = call.split_once('(')?;
    let inner = rest.strip_suffix(')')?;
    let (variant_path, arg_list) = inner.split_once(", [")?;
    let arg = arg_list.strip_suffix(']')?;
    let first_param = params.first()?;

    let variant_name = variant_path.rsplit("::").next()?;
    if ctor_name != variant_name {
        return None;
    }

    let lowered_arg = lower_value_variant_arg(arg, first_param)?;
    match variant_path {
        "Value::Bool" | "Value::Integer" | "Value::String" => {
            Some(format!("{variant_path}({lowered_arg})"))
        }
        _ => None,
    }
}

fn lower_value_variant_arg(arg: &str, first_param: &str) -> Option<String> {
    if arg == first_param {
        return Some(arg.to_string());
    }

    let method_call = arg.strip_prefix("method:")?;
    let (method_name, rest) = method_call.split_once('(')?;
    if method_name.is_empty() {
        return None;
    }
    let inner = rest.strip_suffix(')')?;
    let (receiver, method_args) = inner.split_once(", [")?;
    if receiver != first_param {
        return None;
    }
    if !method_args.strip_suffix(']')?.is_empty() {
        return None;
    }

    Some(format!("{first_param}.{method_name}()"))
}

fn lower_literal(term: &Value) -> Result<String, String> {
    let node = resolved_node(term).ok_or_else(|| "literal".to_string())?;
    if node.get("kind").and_then(Value::as_str) != Some("literal") {
        return Err(resolved_concept_name(term));
    }

    match node.get("value") {
        Some(Value::String(value)) if value == "new" || value.ends_with("::new") => {
            Ok(value.to_string())
        }
        Some(Value::Array(items)) if is_value_null_literal(items) => Ok("Value::Null".to_string()),
        _ => Err("literal".to_string()),
    }
}

fn is_value_null_literal(items: &[Value]) -> bool {
    items.len() == 1 && items[0].as_str() == Some("Null")
}

fn resolved_args(term: &Value) -> Option<&[Value]> {
    resolved_node(term)?
        .get("args")?
        .as_array()
        .map(Vec::as_slice)
}

fn resolved_node(term: &Value) -> Option<&serde_json::Map<String, Value>> {
    term.get("node")?.as_object()
}

fn resolved_concept_name(term: &Value) -> String {
    let Some(node) = resolved_node(term) else {
        return "malformed-resolved-term".to_string();
    };
    match node.get("kind").and_then(Value::as_str) {
        Some("literal") => "literal".to_string(),
        Some("concept:op-application") => node
            .get("op_definition_cid")
            .and_then(Value::as_str)
            .map(op_concept_name)
            .unwrap_or_else(|| "malformed-resolved-term".to_string()),
        _ => "malformed-resolved-term".to_string(),
    }
}

fn op_concept_name(op_definition_cid: &str) -> String {
    match op_definition_cid {
        RETURN_OP_CID => "return".to_string(),
        CALL_NEW_OP_CID => "call:new".to_string(),
        other => other.to_string(),
    }
}

pub fn dispatch(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "name": "provekit-realize-rust",
                "version": "0.1.0",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": ["rust"],
                    "emits_signed_mementos": false,
                    "ir_version": "v1.1.0"
                }
            }
        }),
        "provekit.plugin.body_template_entries" => {
            let params = request
                .get("params")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let library_tag = params
                .get("target_library_tag")
                .or_else(|| params.get("targetLibraryTag"))
                .or_else(|| params.get("library_tag"))
                .or_else(|| params.get("libraryTag"))
                .and_then(Value::as_str)
                .unwrap_or("");
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "entries": body_template_entries_json(library_tag),
                }
            })
        }
        "provekit.plugin.invoke" => {
            let Some(params) = request.get("params").and_then(Value::as_object) else {
                return error(id, -32602, "INVALID_PARAMS: params must be an object");
            };
            let function = params.get("function").and_then(Value::as_str).unwrap_or("");
            let return_type = params
                .get("return_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let concept_name = params
                .get("concept_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let source_function = params
                .get("source_function_name")
                .or_else(|| params.get("sourceFunctionName"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(function);
            let mode = params.get("mode").and_then(Value::as_str);
            // Visibility for this function. Install via the RAII guard so the
            // thread-local is restored when this dispatch arm returns (or
            // panics) — an un-restored write would leak `pub`/`pub(crate)` into
            // the next realization on this thread (the cross-test pollution bug).
            //
            // ABSENT vs PRESENT-EMPTY is load-bearing: when the spec omits
            // `visibility` entirely, leave the thread-local at its default
            // (`None` => `pub`). When `visibility` is PRESENT — including the
            // empty string, which is how `bind` encodes a PRIVATE source fn —
            // thread it verbatim (`Some("")` => bare `fn`), so the real
            // bind->realize pipeline never over-promotes a private fn to `pub`.
            let visibility = params.get("visibility").and_then(Value::as_str);
            let _visibility_guard = VisibilityGuard::set_optional(visibility);
            let target_library_tag = params
                .get("target_library_tag")
                .or_else(|| params.get("targetLibraryTag"))
                .or_else(|| params.get("library_tag"))
                .or_else(|| params.get("libraryTag"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let _target_library_guard = TargetLibraryTagGuard::set(target_library_tag);
            let cn_for_attr = params
                .get("conceptName")
                .or_else(|| params.get("concept_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            CURRENT_CONCEPT_NAME.with(|v| *v.borrow_mut() = cn_for_attr);
            let generic_params = params
                .get("genericParams")
                .or_else(|| params.get("generic_params"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            CURRENT_GENERIC_PARAMS.with(|v| *v.borrow_mut() = generic_params);
            let doc_lines =
                string_array(params.get("docLines").or_else(|| params.get("doc_lines")));
            CURRENT_DOC_LINES.with(|v| *v.borrow_mut() = doc_lines);
            let original_param_types = string_array(
                params
                    .get("originalParamTypes")
                    .or_else(|| params.get("original_param_types")),
            );
            CURRENT_ORIGINAL_PARAM_TYPES.with(|v| *v.borrow_mut() = original_param_types);
            let param_names = string_array(params.get("params"));
            let mut param_types = string_array(params.get("param_types"));
            // #1369: cross-language parametric dispatch. When param_sort_cids
            // is provided (cross-lang signaling), translate them to rust syntax
            // via the catalog + composite-CID expansions. Override the source-
            // language strings in param_types. Returns rust strings; signature
            // emission uses these directly.
            let param_sort_cids = string_array(
                params
                    .get("param_sort_cids")
                    .or_else(|| params.get("paramSortCids")),
            );
            let return_sort_cid = params
                .get("return_sort_cid")
                .or_else(|| params.get("returnSortCid"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let parametric_expansions = parse_parametric_expansions(
                params
                    .get("parametric_sort_expansions")
                    .or_else(|| params.get("parametricSortExpansions")),
            );
            let is_cross_lang =
                !param_sort_cids.iter().all(|c| c.is_empty()) || !return_sort_cid.is_empty();
            if is_cross_lang {
                let mut translated = Vec::with_capacity(param_sort_cids.len());
                for cid in &param_sort_cids {
                    if cid.is_empty() {
                        translated.push(String::new());
                        continue;
                    }
                    translated.push(
                        map_concept_hub_sort_cid_to_rust(cid, &parametric_expansions)
                            .unwrap_or_default(),
                    );
                }
                // Use translated types where non-empty; else keep original
                // (which lets primitives fall through legacy map_source_type).
                for (i, t) in translated.iter().enumerate() {
                    if !t.is_empty() && i < param_types.len() {
                        param_types[i] = t.clone();
                    } else if !t.is_empty() {
                        // pad if param_types shorter than param_sort_cids
                        while param_types.len() < i {
                            param_types.push(String::new());
                        }
                        param_types.push(t.clone());
                    }
                }
            }
            let return_type = if is_cross_lang && !return_sort_cid.is_empty() {
                map_concept_hub_sort_cid_to_rust(return_sort_cid, &parametric_expansions)
                    .unwrap_or_else(|| return_type.to_string())
            } else {
                return_type.to_string()
            };
            let return_type = return_type.as_str();
            let operand_bindings = value_array(
                params
                    .get("operand_bindings")
                    .or_else(|| params.get("operandBindings")),
            );
            let proc_macro_invocations = value_array(
                params
                    .get("procMacroInvocations")
                    .or_else(|| params.get("proc_macro_invocations")),
            );
            let realized = if let Some(term_shape) =
                params.get("term_shape").or_else(|| params.get("termShape"))
            {
                let r = emit_from_term_shape_with_bindings(
                    term_shape,
                    &operand_bindings,
                    source_function,
                    &param_names,
                    &param_types,
                    return_type,
                );
                apply_proc_macro_invocations(r, &proc_macro_invocations)
            } else {
                emit_stub_with_mode_and_invocations(
                    source_function,
                    &param_names,
                    &param_types,
                    return_type,
                    concept_name,
                    mode,
                    &proc_macro_invocations,
                )
            };
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "source": realized.source,
                    "emitted_artifact_cid": realized.emitted_artifact_cid,
                    "is_stub": realized.is_stub,
                    "extension": realized.extension,
                    "observed_loss_record": realized.observed_loss_record,
                    "used_sugars": realized.used_sugars
                }
            })
        }
        // The kit IS the authority on its platform semantics. libprovekit
        // calls this RPC method to load the declaration; it MUST NOT carry
        // a hardcoded mirror of this data. Per #1270.
        "provekit.plugin.platform_semantics" => {
            let decl = crate::platform_semantics::declaration();
            let dimension_values = crate::platform_semantics::dimension_values();
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tags": decl.tags,
                    "dimension_values": dimension_values,
                    "op_aliases": rust_concept_op_aliases()
                }
            })
        }
        "shutdown" | "provekit.plugin.shutdown" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": null
        }),
        _ => error(id, -32601, &format!("METHOD_NOT_FOUND: {method}")),
    }
}

/// Op-CID aliases declared by the Rust kit. Moved here from libprovekit
/// (was `libprovekit/src/core/platform_semantics.rs::rust_concept_op_aliases`)
/// because op aliases are kit knowledge: the kit knows which concept-CIDs it
/// realizes as which target ops, and ships that as part of its declaration.
fn rust_concept_op_aliases() -> std::collections::BTreeMap<String, String> {
    use std::collections::BTreeMap;
    BTreeMap::from([
        (
            "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468".to_string(),
            "blake3-512:398980644a46039b0c2875ab36ccb61f52f284ccad5481593305ed3f10efe91e7863c00a3f2d673644430f691e6b5354f5d65f9da4fa23acdb13dc58f5b438f9".to_string(),
        ),
        (
            "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af".to_string(),
            "blake3-512:b6c62a64669ff12d0af45d9932c1ab5e08576f1cac97b4abe60392a9f02393dac9765514b024b1481ddc829d4b7fb97950ad648a9944dceafa194b8423923533".to_string(),
        ),
        (
            "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03".to_string(),
            "blake3-512:1df457dceb0ec7a6dc4596eb70be001be09180afc69fa3ff8121cd78a0daff5dd9606dbfd4fb9fcdc5d834939a6f19c52b80aace16dea6df5ffdce62d86bbfa2".to_string(),
        ),
        (
            "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839".to_string(),
            "blake3-512:d7403da8d2a8921b71170b5fc34c12022118d0c545f25c7ff89fe77bbed02419e3528479ded0e746535ee92d0e1801bce46608c15c3d6d2a5567bec811cbc75a".to_string(),
        ),
        (
            "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d".to_string(),
            "blake3-512:235c6177611c2753a1c0d07d44391f5465ab50dc585372df52220118cb103ef19502192a07148bd2969d7f6f7ed0d134714d7745825f486768d0b0de8ac0b6dc".to_string(),
        ),
        (
            "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a".to_string(),
            "blake3-512:37af5330572cf08650e3b6d5fdfc2649d56c0bb2e019f9be3861082c9d1961c1808beca6f9dfc39742ade25f06bfb499da74c89d33f64decd0c70f0972d021e1".to_string(),
        ),
        (
            "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b".to_string(),
            "blake3-512:cb23fbc9d05a19b353e1fe85c77e241fdc8c58cde5a7c5cad008b721a51eaf682284d8bfe3b383d751cb58833e94beb6bd0dd4d330f9619f095c8b4daa8298da".to_string(),
        ),
        (
            "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b".to_string(),
            "blake3-512:fcc41d285a20dae6c2deb2a854665d5d43bc829a09a76107d929898b3b169d1abf53ed71f302b00ec2146bcec3b5fe732ca7ecd4354e7739e67feea3db9fd6a2".to_string(),
        ),
        (
            "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3".to_string(),
            "blake3-512:5c455355a13fd97a872848613b34b2b56f9738c832f900558710af1cd053976157513f31a8feb123202557dc0a369b88bc7c946179fe817d6c2f80d4f318f824".to_string(),
        ),
        (
            "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353".to_string(),
            "blake3-512:16ba612da4883e853dd18b08c8e7b1803e1e2b0a42ab83c261048a49cdfd9b20bc54e809b8f4e8e5c9af63cc7447dee039cb826c611dfec137855a11a502adb9".to_string(),
        ),
        (
            "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f".to_string(),
            "blake3-512:eeaaf14737f661b6bce03f23d281974502182fea83909eeaade25e510887b26e80dac1b10af3b1f2f496b53898051d63e8d250e78cfa8e88380c84809e5eabe0".to_string(),
        ),
        (
            "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409".to_string(),
            "blake3-512:e0c3e13fd7e0d11fa3b78f4e083ab60b1166bdd905bc04e533e6dcc97d79330bd6a403caaf1265d8134ea3ccd5fe8cfd5a3e18f349ea7edcb6310c098e845c0f".to_string(),
        ),
    ])
}

pub fn run_rpc() {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let method = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let response = match serde_json::from_str::<Value>(line) {
            Ok(request) => dispatch(&request),
            Err(err) => error(Value::Null, -32700, &format!("PARSE_ERROR: {err}")),
        };
        let _ = serde_json::to_writer(&mut stdout, &response);
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
        if method == "shutdown" || method == "provekit.plugin.shutdown" {
            break;
        }
    }
}

fn body_template_for(
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    mode: Option<&str>,
) -> Option<RenderedBody> {
    body_template_for_entries(
        entries(),
        concept_name,
        params,
        param_types,
        return_type,
        mode,
    )
}

fn body_template_for_entries(
    entries: &[BodyTemplateEntry],
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    mode: Option<&str>,
) -> Option<RenderedBody> {
    let mapped_param_types: Vec<String> =
        param_types.iter().map(|ty| map_source_type(ty)).collect();
    let mapped_return_type = map_source_type(return_type);
    for entry in entries {
        if !concept_matches(&entry.concept_name, concept_name) {
            continue;
        }
        if !mode_matches(entry.mode.as_deref(), mode) {
            continue;
        }
        if entry.min_params.is_some_and(|min| params.len() < min) {
            continue;
        }
        if entry.max_params.is_some_and(|max| params.len() > max) {
            continue;
        }
        if entry.template_kind != "verbatim" {
            continue;
        }
        if let Some(required) = &entry.requires_return_type {
            if required != &mapped_return_type {
                continue;
            }
        }
        if let Some(required) = &entry.requires_param_types {
            if required != &mapped_param_types {
                continue;
            }
        }
        if let Some(rendered) = render_template(
            &entry.template,
            params,
            &mapped_param_types,
            &mapped_return_type,
        ) {
            return Some(RenderedBody {
                body: rendered,
                observed_loss_record: observed_loss_record_for_entry(entry),
            });
        }
    }
    None
}

fn observed_loss_record_for_entry(entry: &BodyTemplateEntry) -> Value {
    if entry.realization_kind.as_deref() != Some("boundary-realization") {
        return json!({});
    }
    let Some(contribution) = entry.loss_record_contribution.clone() else {
        return json!({});
    };
    json!({
        "loss_record_contribution": contribution
    })
}

fn operator_body_template_for(
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    mode: Option<&str>,
) -> Option<RenderedBody> {
    let current_library =
        CURRENT_TARGET_LIBRARY_TAG.with(|value| value.borrow().trim().to_string());
    if !current_library.is_empty() {
        let shim_entries = shim_body_template_entries(&current_library);
        if !shim_entries.is_empty() {
            if let Some(rendered) = body_template_for_entries(
                &shim_entries,
                concept_name,
                params,
                param_types,
                return_type,
                mode,
            ) {
                return Some(rendered);
            }
        }
    }
    let root = operator_root()?;
    let (language, library_tag) = operator_binding_surface(&root, concept_name)?;
    if language != "rust" {
        return None;
    }
    let shim_entries = shim_body_template_entries(&library_tag);
    if !shim_entries.is_empty() {
        if let Some(rendered) = body_template_for_entries(
            &shim_entries,
            concept_name,
            params,
            param_types,
            return_type,
            mode,
        ) {
            return Some(rendered);
        }
    }
    let template = load_library_body_template(&language, &library_tag)?;
    body_template_for_entries(
        &template,
        concept_name,
        params,
        param_types,
        return_type,
        mode,
    )
}

fn operator_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("PROVEKIT_OPERATOR_ROOT") {
        let root = PathBuf::from(root);
        if root
            .join(".provekit")
            .join("library-bindings.json")
            .is_file()
        {
            return Some(root);
        }
    }
    std::env::current_dir().ok().and_then(|cwd| {
        cwd.ancestors()
            .find(|base| {
                base.join(".provekit")
                    .join("library-bindings.json")
                    .is_file()
            })
            .map(Path::to_path_buf)
    })
}

fn operator_binding_surface(root: &Path, concept_name: &str) -> Option<(String, String)> {
    let path = root.join(".provekit").join("library-bindings.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    let config_language = doc.get("language")?.as_str()?;
    let bindings = doc.get("bindings")?.as_object()?;
    let surface = bindings
        .iter()
        .find(|(candidate, _)| concept_matches(candidate, concept_name))?
        .1
        .as_str()?;
    let (language, tag) = split_library_surface(surface)?;
    if language != config_language {
        return None;
    }
    Some((language, tag))
}

fn split_library_surface(surface: &str) -> Option<(String, String)> {
    let (language, tag) = surface.split_once('-')?;
    if language.is_empty() || tag.is_empty() {
        return None;
    }
    Some((language.to_string(), tag.to_string()))
}

fn load_library_body_template(language: &str, library_tag: &str) -> Option<Vec<BodyTemplateEntry>> {
    let rel = PathBuf::from("menagerie")
        .join(format!("{language}-language-signature"))
        .join("specs")
        .join("body-templates")
        .join(format!("{language}-canonical-bodies-{library_tag}.json"));
    find_repo_file(&rel)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|root| parse_entries(&root))
}

fn shim_body_template_entries(library_tag: &str) -> Vec<BodyTemplateEntry> {
    let values = shim_template_entry_values(library_tag);
    if values.is_empty() {
        return Vec::new();
    }
    let envelope = json!({ "header": { "content": { "entries": values } } });
    parse_entries(&envelope)
}

fn body_template_entries_json(library_tag: &str) -> Vec<Value> {
    shim_template_entry_values(library_tag)
}

fn shim_template_entry_values(library_tag: &str) -> Vec<Value> {
    if library_tag.trim().is_empty() {
        return Vec::new();
    }
    static SHIM_ENTRIES: OnceLock<BTreeMap<String, Vec<Value>>> = OnceLock::new();
    SHIM_ENTRIES
        .get_or_init(|| {
            let mut entries = BTreeMap::new();
            entries.insert(
                "postgres".to_string(),
                entries_from_shim_proof(provekit_shim_postgres::PROVEKIT_PROOF_BYTES, "postgres"),
            );
            entries.insert(
                "rusqlite".to_string(),
                entries_from_shim_proof(provekit_shim_rusqlite::PROVEKIT_PROOF_BYTES, "rusqlite"),
            );
            entries
        })
        .get(library_tag)
        .cloned()
        .unwrap_or_default()
}

fn entries_from_shim_proof(bytes: &[u8], library_tag: &str) -> Vec<Value> {
    let Ok(catalog) = provekit_proof_envelope::cbor_decode::decode(bytes) else {
        return Vec::new();
    };
    let Some(root) = catalog.as_map() else {
        return Vec::new();
    };
    let Some(members) = root.get("members").and_then(|value| value.as_map()) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for member in members.values() {
        let Some(member_bytes) = member.as_bstr() else {
            continue;
        };
        let Ok(member_json) = serde_json::from_slice::<Value>(member_bytes) else {
            continue;
        };
        let body = member_json.get("body").unwrap_or(&member_json);
        if body.get("kind").and_then(Value::as_str) != Some("library-sugar-binding-entry") {
            continue;
        }
        if library_tag.is_empty()
            || body.get("target_library_tag").and_then(Value::as_str) != Some(library_tag)
        {
            continue;
        }
        if let Some(entry) = binding_entry_to_template_entry(body, library_tag) {
            entries.push(entry);
        }
    }
    entries
}

fn binding_entry_to_template_entry(decl: &Value, library_tag: &str) -> Option<Value> {
    let concept_name = decl.get("concept_name").and_then(Value::as_str)?;
    let param_names = decl
        .get("param_names")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let body_text = decl
        .get("body_source")
        .and_then(|body| body.get("body_text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if body_text.is_empty() {
        return None;
    }
    let arity = param_names.len();
    let mut entry = json!({
        "concept_name": concept_name,
        "emission_template": {
            "kind": "verbatim",
            "template": substitute_shim_params_with_placeholders(body_text, &param_names),
        },
        "loss_record_contribution": decl
            .get("loss_record_contribution")
            .cloned()
            .unwrap_or_else(|| json!({"form": "literal", "value": {"entries": []}})),
        "signature_guard": {
            "min_params": arity,
            "max_params": arity,
        },
        "target_library_tag": library_tag,
    });
    if let Some(observed) = decl.get("observed_dimension").and_then(Value::as_str) {
        entry["observed_dimension"] = Value::String(observed.to_string());
    }
    if let Some(helpers) = decl.get("file_helpers").cloned() {
        entry["file_helpers"] = helpers;
    }
    Some(entry)
}

fn substitute_shim_params_with_placeholders(body: &str, param_names: &[String]) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphabetic() || c == b'_' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let ident = std::str::from_utf8(&bytes[i..j]).unwrap_or("");
            if let Some(index) = param_names.iter().position(|name| name == ident) {
                out.push_str(&format!("${{param{index}}}"));
            } else {
                out.push_str(ident);
            }
            i = j;
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

fn render_template(
    template: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Option<String> {
    let mut rendered = template.to_string();
    for (index, name) in params.iter().enumerate() {
        rendered = rendered.replace(&format!("${{param{index}}}"), name);
    }
    for (index, ty) in param_types.iter().enumerate() {
        rendered = rendered.replace(&format!("${{param_type_{index}}}"), ty);
    }
    rendered = rendered.replace("${param_count}", &params.len().to_string());
    rendered = rendered.replace("${return_type}", return_type);
    if rendered.contains("${") {
        None
    } else {
        Some(rendered)
    }
}

thread_local! {
    /// Visibility for the function currently being realized. Set at the
    /// dispatch site from the spec's `visibility` field, read in
    /// function_source. Threading visibility through every wrapper would touch
    /// ~20 call sites; thread-local keeps the surface minimal.
    ///
    /// The `Option` distinguishes ABSENT from PRESENT-EMPTY, which is
    /// load-bearing for correctness:
    ///   `None`        => no visibility was threaded (e.g. emit_stub /
    ///                    emit_from_resolved called directly as the public-API
    ///                    realization surface). Defaults to `pub`.
    ///   `Some("")`    => an explicit PRIVATE/inherited source visibility. This
    ///                    is exactly how `bind` encodes a private function
    ///                    (bind.rs: NamedTerm.visibility = String::new()), so
    ///                    the real bind->realize pipeline MUST emit a bare `fn`
    ///                    and must NOT over-promote it to `pub`.
    ///   `Some("pub")` / `Some("pub(crate)")` / ... => emit verbatim.
    pub(crate) static CURRENT_VISIBILITY: std::cell::RefCell<Option<String>> =
        std::cell::RefCell::new(None);
    /// Generic parameter declarations (e.g. "<A: AdapterLifter>") set at
    /// dispatch, read in function_source for byte-identical signature emit.
    pub(crate) static CURRENT_GENERIC_PARAMS: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::new());
    /// Original param types as-written (with `&A` etc preserved). When set,
    /// function_source uses these for signature emission instead of the
    /// substituted param_types.
    pub(crate) static CURRENT_ORIGINAL_PARAM_TYPES: std::cell::RefCell<Vec<String>> =
        std::cell::RefCell::new(Vec::new());
    /// Concept name for the function being realized. Emitted as
    /// `#[provekit::sugar(concept = "X")]` attribute so the output can be
    /// re-lifted (cycle-invariance test).
    pub(crate) static CURRENT_CONCEPT_NAME: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::new());
    /// Doc comment lines (`///` content, without the prefix) for the
    /// function being realized. Emitted as `/// <text>` lines between the
    /// concept attribute and the fn signature. Empty when source had no docs.
    pub(crate) static CURRENT_DOC_LINES: std::cell::RefCell<Vec<String>> =
        std::cell::RefCell::new(Vec::new());
    /// Dispatcher-selected target library tag for the current realize call.
    /// The Rust kit uses it to resolve its own shim proof through cargo path
    /// dependencies instead of accepting substrate-fed template bodies.
    pub(crate) static CURRENT_TARGET_LIBRARY_TAG: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::new());
}

/// RAII guard that installs the request's target library tag for one realize
/// dispatch and restores the prior value on drop.
struct TargetLibraryTagGuard {
    previous: String,
}

impl TargetLibraryTagGuard {
    fn set(library_tag: &str) -> Self {
        let guard = TargetLibraryTagGuard {
            previous: CURRENT_TARGET_LIBRARY_TAG.with(|v| v.borrow().clone()),
        };
        CURRENT_TARGET_LIBRARY_TAG.with(|v| *v.borrow_mut() = library_tag.to_string());
        guard
    }
}

impl Drop for TargetLibraryTagGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        CURRENT_TARGET_LIBRARY_TAG.with(|v| *v.borrow_mut() = previous);
    }
}

/// Pretty-print comma-separated macro args (format!, println!, etc.).
/// Each top-level arg on its own line, matching rustfmt's source-layout
/// style for long macro calls. Splits on top-level `,` only (not inside
/// nested parens/brackets/braces/strings).
fn pretty_print_macro_args(tokens: &str) -> String {
    let trimmed = tokens.trim();
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
    let mut in_string = false;
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            current.push(c);
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                }
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                current.push(c);
            }
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
            '{' => {
                depth_brace += 1;
                current.push(c);
            }
            '}' => {
                depth_brace -= 1;
                current.push(c);
            }
            ',' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                let item = current.trim().to_string();
                if !item.is_empty() {
                    items.push(item);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let last = current.trim().to_string();
    if !last.is_empty() {
        items.push(last);
    }
    if items.is_empty() {
        return String::new();
    }
    // Source-style layout:
    //   format!(
    //       "arg0",
    //       arg1
    //   )
    // No trailing comma on the last arg.
    //
    // Indent issue: rustfmt on stable does NOT re-indent inside macro
    // bodies (format_macro_bodies is nightly-only). Whatever we emit
    // here IS the final indent. We don't know runtime nesting depth at
    // emit time — substrate emits each fragment without global context.
    // We mark args with sentinel `\u{1F}MACRO_INDENT\u{1F}` so a
    // post-processing pass in function_source can substitute the
    // depth-aware indent based on the macro's column position.
    let mut out = String::from("\n");
    for (idx, item) in items.iter().enumerate() {
        let is_last = idx + 1 == items.len();
        out.push('\u{1F}');
        out.push_str("MACRO_ARG_INDENT");
        out.push('\u{1F}');
        out.push_str(item);
        if !is_last {
            out.push(',');
        }
        out.push('\n');
    }
    out.push('\u{1F}');
    out.push_str("MACRO_CLOSE_INDENT");
    out.push('\u{1F}');
    out
}

/// Pretty-print a json!-style macro body that was lifted as a token
/// stream (loses original layout). Emits the canonical rustfmt-style
/// layout: opening brace on the call line, items indented, closing
/// brace on its own line. Strings, nested objects, and arrays render
/// with rustfmt-consistent spacing.
///
/// Input shape: `{ "k1": v1, "k2": v2, }` (post-normalize).
/// Output shape (matching rustfmt source layout):
///   {
///       "k1": v1,
///       "k2": v2,
///   }
fn pretty_print_macro_body(tokens: &str) -> String {
    let trimmed = tokens.trim();
    // Only pretty-print balanced-brace top-level. Bail otherwise (caller
    // already emits the inline form which is at least valid rust).
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return tokens.to_string();
    }
    let inner_raw = &trimmed[1..trimmed.len() - 1].trim();
    // Preserve source-comma style: if the source had a trailing comma
    // after the last item, our output should too (and vice versa).
    // The trim() above strips a trailing comma if it was the last byte
    // followed only by whitespace — re-check the raw inner.
    let source_has_trailing_comma = inner_raw.trim_end().ends_with(',');
    let inner = inner_raw.trim_end_matches(',').trim_end();
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth_brace = 0i32;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_string = false;
    for c in inner.chars() {
        if in_string {
            current.push(c);
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                current.push(c);
            }
            '{' => {
                depth_brace += 1;
                current.push(c);
            }
            '}' => {
                depth_brace -= 1;
                current.push(c);
            }
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
            ',' if depth_brace == 0 && depth_paren == 0 && depth_bracket == 0 => {
                let trimmed_item = current.trim().to_string();
                if !trimmed_item.is_empty() {
                    items.push(trimmed_item);
                }
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    let trimmed_last = current.trim().to_string();
    if !trimmed_last.is_empty() {
        items.push(trimmed_last);
    }
    if items.is_empty() {
        return "{}".to_string();
    }
    // Each item is "key: value" or just a value. For nested objects in
    // values, recursively pretty-print so multi-line layout cascades.
    let mut out = String::from("{\n");
    for (idx, item) in items.iter().enumerate() {
        let is_last = idx + 1 == items.len();
        // Source-style: when the last item's value is a MULTI-LINE
        // nested object, source convention omits the trailing comma
        // (rust's hand-written json! style). Detect by checking if
        // the value ends with multi-line `}`.
        let item_value_is_multiline_object = find_top_level_colon(item)
            .map(|i| {
                let v = item[i + 1..].trim();
                v.starts_with('{') && v.ends_with('}') && v.contains('\n')
            })
            .unwrap_or(false);
        // Also a length check — large nested objects also skip trailing.
        let item_value_is_large_object = find_top_level_colon(item)
            .map(|i| {
                let v = item[i + 1..].trim();
                v.starts_with('{') && v.ends_with('}') && v.len() > 60
            })
            .unwrap_or(false);
        let suppress_trailing_for_last_nested =
            is_last && (item_value_is_multiline_object || item_value_is_large_object);
        let sep = if (is_last && !source_has_trailing_comma) || suppress_trailing_for_last_nested {
            ""
        } else {
            ","
        };
        if let Some(colon_idx) = find_top_level_colon(item) {
            let key = item[..colon_idx].trim();
            let value = item[colon_idx + 1..].trim();
            let value_pretty = if value.starts_with('{') && value.ends_with('}') {
                // Small nested objects (3 or fewer items, <= 80 chars
                // total) stay inline. Larger ones get pretty-printed
                // for readability — matches rustfmt's source-layout style.
                let pretty = pretty_print_macro_body(value);
                let item_count = pretty.matches(",\n").count();
                if item_count <= 2 && value.len() <= 80 {
                    value.to_string()
                } else {
                    indent_lines(&pretty, 4)
                }
            } else if value.starts_with('[') && value.ends_with(']') {
                value.to_string()
            } else {
                value.to_string()
            };
            out.push_str(&format!("    {}: {}{}\n", key, value_pretty, sep));
        } else {
            out.push_str(&format!("    {}{}\n", item, sep));
        }
    }
    out.push('}');
    out
}

fn find_top_level_colon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b':' if depth == 0 => {
                // Skip `::` (path separator).
                if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    i += 2;
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn indent_lines(text: &str, _indent: usize) -> String {
    // The first line stays where it is; subsequent lines get an extra
    // 4-space indent to align with their parent's nesting level. rustfmt's
    // canonical layout for nested objects in macro bodies is:
    //   "outer": {
    //       "inner": value,
    //   },
    // We add 4 spaces to lines 2..N (the closing brace + inner items).
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.to_string();
    }
    let mut out = String::from(lines[0]);
    for line in &lines[1..] {
        out.push('\n');
        if !line.is_empty() {
            out.push_str("    ");
        }
        out.push_str(line);
    }
    out
}

/// Normalize whitespace in macro token text. syn::to_token_stream renders
/// every punctuation token with surrounding spaces (`key : value`,
/// `! v . is_null ( )`). For byte-comparison with source we collapse:
///   `key : value` -> `key: value`
///   `recv . method ( )` -> `recv.method()`
///   `! expr` -> `!expr`  (when not part of `!=`)
/// This is a pragmatic cleanup; the canonical fix is preserving original
/// source spans at lift time.
fn normalize_macro_tokens(tokens: &str) -> String {
    let mut out = String::with_capacity(tokens.len());
    let bytes = tokens.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        // Track string-literal regions so we don't mangle their content.
        // Rust string literal: starts with `"`, ends with unescaped `"`.
        // Within a string we copy bytes verbatim.
        if in_string {
            out.push(bytes[i] as char);
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                // Skip the escaped char.
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            in_string = true;
            out.push('"');
            i += 1;
            continue;
        }
        // Pattern: ` X ` where X is `:` `.` `(` `)` `[` `]` `,` `;` `!`
        if i + 2 < bytes.len() && bytes[i] == b' ' {
            let next = bytes[i + 1];
            match next {
                b':' => {
                    // Distinguish `:` from `::`. For json! body we have
                    // `key : value` (single colon, KV separator).
                    if i + 2 < bytes.len() && bytes[i + 2] == b':' {
                        // Path separator `Foo :: Bar` -> `Foo::Bar`
                        out.push(':');
                        out.push(':');
                        i += 4.min(bytes.len() - i);
                        // Skip the space after `::` too
                        while i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        continue;
                    } else {
                        // `key : value` -> `key: value` (drop space before)
                        out.push(':');
                        i += 2;
                        continue;
                    }
                }
                b'.' => {
                    // `recv . method` -> `recv.method`. Drop space before
                    // and after.
                    out.push('.');
                    i += 2;
                    while i < bytes.len() && bytes[i] == b' ' {
                        i += 1;
                    }
                    continue;
                }
                b'(' | b'[' | b',' | b';' => {
                    out.push(next as char);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        // ` ) ` / ` ] ` — drop space before closers.
        if i + 1 < bytes.len() && bytes[i] == b' ' && (bytes[i + 1] == b')' || bytes[i + 1] == b']')
        {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        // `! v` — drop space when prev char isn't `=` (not `!=`).
        if i + 1 < bytes.len() && bytes[i] == b'!' && bytes[i + 1] == b' ' {
            let prev_is_op_or_start = out.is_empty()
                || matches!(
                    out.as_bytes().last(),
                    Some(b' ' | b'(' | b'[' | b'{' | b',' | b';' | b':')
                );
            if prev_is_op_or_start {
                out.push('!');
                i += 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Decide whether a binary-op operand text needs parens. Atoms (a single
/// identifier, literal, call, method chain, indexed-access) are emitted
/// bare; anything with a binary operator at the top level gets wrapped.
/// Avoids the verbose `(a) + (b)` form rustfmt won't simplify.
fn paren_for_op(text: &str) -> String {
    let trimmed = text.trim();
    // Already parenthesized at top level — leave as-is.
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Verify the wrapping parens match (could be `(a) + (b)` which
        // is NOT a single parenthesized expression).
        let mut depth = 0;
        let mut first_zero_at_end = true;
        for (i, c) in trimmed.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 && i + 1 < trimmed.len() {
                    first_zero_at_end = false;
                    break;
                }
            }
        }
        if first_zero_at_end {
            return trimmed.to_string();
        }
    }
    // Atomic forms: identifier, literal, call, method chain — no
    // binary op at top level. Crude but effective: if there's no
    // unparenthesized space-separated operator like ' + ' / ' && ',
    // emit bare.
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'[' => depth_bracket += 1,
            b']' => depth_bracket -= 1,
            b' ' if depth_paren == 0 && depth_bracket == 0 => {
                // Top-level space — likely surrounds a binary op.
                // Need parens.
                return format!("({trimmed})");
            }
            _ => {}
        }
        i += 1;
    }
    trimmed.to_string()
}

/// Heuristic: does the for-loop body use `var` in a position that requires
/// primitive (non-reference) semantics? Detects `var >> N`, `var & N`,
/// `var as TYPE`, and similar — all of which require var:T not var:&T,
/// implying the source was `for &var in &iter`.
fn body_uses_var_as_primitive(body: &str, var: &str) -> bool {
    if var.is_empty() {
        return false;
    }
    // Look for `<var>` followed (after optional whitespace) by an op that
    // needs primitive semantics, OR `(<var>)` followed by such an op.
    let patterns: &[&str] = &[
        &format!("{} >>", var),
        &format!("{}>>", var),
        &format!("{} <<", var),
        &format!("{}<<", var),
        &format!("{} & ", var),
        &format!("{}&", var),
        &format!("{} | ", var),
        &format!("{}|", var),
        &format!("{} as ", var),
        &format!("{} + ", var),
        &format!("{} - ", var),
        &format!("{} * ", var),
        &format!("{} / ", var),
        &format!("{} % ", var),
        &format!("({})", var),
    ];
    // Boundary check: don't match `bar` against `bart`. Look for the
    // pattern preceded by non-identifier char.
    for pat in patterns {
        let mut from = 0usize;
        while let Some(pos) = body[from..].find(pat) {
            let abs = from + pos;
            let prev = if abs == 0 {
                ' '
            } else {
                body.as_bytes()[abs - 1] as char
            };
            if !(prev.is_ascii_alphanumeric() || prev == '_') {
                return true;
            }
            from = abs + pat.len();
        }
    }
    false
}

/// Post-pass mut inference (#1391 follow-on). Scan the emitted body for
/// mutating method calls `<recv>.<method>(...)` where method ∈ {push,
/// insert, push_str, set, append, add, extend, pop, clear, remove,
/// truncate} and promote the receiver's `let recv = ...;` (or `let recv:
/// T = ...;`) to `let mut recv = ...;`. Conservative: only promotes
/// receivers that appear as a `<ident>.<method>(` text pattern and have
/// a matching `let <ident>` line earlier in the body.
fn infer_let_mut(body: &str) -> String {
    use std::collections::HashSet;
    const MUTATING: &[&str] = &[
        "push", "insert", "push_str", "set", "append", "add", "extend", "pop", "clear", "remove",
        "truncate",
    ];
    let mut receivers: HashSet<String> = HashSet::new();
    let receiver_pat = regex_lite_find_method_receivers(body, MUTATING);
    for r in receiver_pat {
        receivers.insert(r);
    }
    if receivers.is_empty() {
        return body.to_string();
    }
    let mut out = Vec::new();
    for line in body.lines() {
        let trim = line.trim_start();
        let mut rewritten = None;
        if let Some(rest) = trim.strip_prefix("let ") {
            if !rest.starts_with("mut ") {
                // Extract the binding identifier.
                let end = rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_');
                if let Some(end) = end {
                    let name = &rest[..end];
                    let next = rest[end..].chars().next();
                    if matches!(next, Some(':') | Some('=') | Some(' ')) && receivers.contains(name)
                    {
                        let indent = &line[..line.len() - trim.len()];
                        rewritten = Some(format!("{}let mut {}", indent, rest));
                    }
                }
            }
        }
        out.push(rewritten.unwrap_or_else(|| line.to_string()));
    }
    out.join("\n")
}

/// Find receivers of mutating method calls. Returns identifiers that
/// appear as `<ident>.<method>(`.
fn regex_lite_find_method_receivers(body: &str, methods: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for method in methods {
        let needle = format!(".{}(", method);
        let mut search_from = 0;
        while let Some(pos) = body[search_from..].find(&needle) {
            let absolute = search_from + pos;
            // Walk backwards over identifier chars from `absolute`.
            let bytes = body.as_bytes();
            let mut start = absolute;
            while start > 0 {
                let c = bytes[start - 1] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    start -= 1;
                } else {
                    break;
                }
            }
            if start < absolute {
                let ident = &body[start..absolute];
                // Skip if preceded by `.` (chained call: `a.b.push(...)`) or
                // by `::` (path call: `Foo::push(...)`).
                if start > 0 {
                    let prev = bytes[start - 1] as char;
                    if prev == '.' || prev == ':' {
                        search_from = absolute + needle.len();
                        continue;
                    }
                }
                out.push(ident.to_string());
            }
            search_from = absolute + needle.len();
        }
    }
    out
}

fn function_source(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
) -> String {
    // Preserve param_types verbatim for byte-exact signature reproduction:
    // `s: &str` must NOT be transformed to `s: String`. The cross-language
    // canonicalization done by `map_source_type` is for body-template
    // matching, not signature emission.
    // Prefer original_param_types (byte-identical source spelling, e.g.
    // `&A`) when available. Fall back to param_types (substituted form,
    // e.g. `AdapterLifter` after trait-bound substitution).
    let original_params = CURRENT_ORIGINAL_PARAM_TYPES.with(|v| v.borrow().clone());
    let typed_params = params
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let ty = original_params
                .get(index)
                .cloned()
                .filter(|s| !s.is_empty())
                .or_else(|| param_types.get(index).cloned())
                .unwrap_or_else(|| "i64".to_string());
            // #1391 follow-on: when type is a java FQN (cross-language
            // signal), translate to rust source via map_source_type.
            // map_source_type's fallback is identity, so same-language
            // types pass through unchanged.
            let ty = if ty.contains('.') || ty == "JsonNode" || ty.starts_with("Result<") {
                map_source_type(&ty)
            } else {
                ty
            };
            format!("{name}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mapped_return = if return_type.contains('.')
        || return_type == "JsonNode"
        || return_type.starts_with("Result<")
    {
        map_source_type(return_type)
    } else {
        return_type.to_string()
    };
    let return_type = mapped_return.as_str();
    let return_suffix = if return_type.is_empty() || return_type == "()" || return_type == "void" {
        String::new()
    } else {
        format!(" -> {return_type}")
    };
    let generic_params = CURRENT_GENERIC_PARAMS.with(|v| v.borrow().clone());
    let body_lines = body.lines().collect::<Vec<_>>();
    let indented = if body_lines.is_empty() {
        String::new()
    } else {
        body_lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("    {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Post-process macro-arg indent sentinels. For each line containing a
    // sentinel, look back through previous lines to find the macro call's
    // leading whitespace, then substitute the sentinel with that indent +
    // 4 (for args) or same indent (for closing paren). This gives correct
    // depth-aware indent for macro bodies without rustfmt re-indenting.
    let indented = resolve_macro_indent_sentinels(&indented);
    let vis_prefix = CURRENT_VISIBILITY.with(|v| {
        let s = v.borrow();
        // Visibility resolution (see CURRENT_VISIBILITY thread-local doc):
        //   None        (no visibility threaded) => `pub`. The realizer's
        //               direct entry points (emit_stub / emit_from_resolved) are
        //               the public-API realization surface, so an absent
        //               visibility defaults to public.
        //   Some("")    (explicit private/inherited — how `bind` encodes a
        //               private source fn) => no prefix => bare `fn`. The real
        //               bind->realize pipeline lands here and must NOT
        //               over-promote a private fn to `pub`.
        //   Some("pub") / Some("pub(crate)") / ... => emit verbatim.
        match s.as_deref() {
            None => "pub ".to_string(),
            Some("") => String::new(),
            Some(other) => format!("{other} "),
        }
    });
    // Emit #[provekit::sugar(concept = "X")] attribute when concept name
    // is known. This marks the function for re-lifting in subsequent
    // cycles — the cycle-invariance test.
    let attr_prefix = CURRENT_CONCEPT_NAME.with(|v| {
        let s = v.borrow();
        if s.is_empty() {
            String::new()
        } else {
            // Full @sugar attribute matching the source form so the
            // lifter recognizes the function in subsequent cycles.
            // library tag = libprovekit-rpc-cross-platform (the demo's
            // canonical library — generalizing to per-fn library lookup
            // is future work for multi-library round-trips).
            format!(
                "#[provekit::sugar(\n    concept = \"{}\",\n    library = \"libprovekit-rpc-cross-platform\",\n    loss = [],\n)]\n",
                s
            )
        }
    });
    // Doc-comment prefix: emit each doc line as `/// <text>` between the
    // @sugar attribute and the fn signature, matching the rust source's
    // typical `attr → doc → fn` order.
    let doc_prefix = CURRENT_DOC_LINES.with(|v| {
        let lines = v.borrow();
        if lines.is_empty() {
            String::new()
        } else {
            lines
                .iter()
                .map(|l| format!("///{}\n", l))
                .collect::<Vec<_>>()
                .join("")
        }
    });
    let assembled = format!("{attr_prefix}{doc_prefix}{vis_prefix}fn {function}{generic_params}({typed_params}){return_suffix} {{\n{indented}\n}}\n");
    // #1391 follow-on: macro-body re-indent. rustfmt on stable does NOT
    // reformat macro internals. The sentinel resolver gives args the
    // pre-rustfmt column position; when rustfmt re-indents the macro call
    // line (e.g. moving `format!(` from col 4 to col 12 because it's
    // inside a closure block), the args stay at their original column.
    // Fix: run rustfmt over the assembled source, then for each macro call
    // line in the rustfmt output, re-indent the body between `!(` and the
    // matching `)` to call_col+4. This closes the run_server diff.
    rustfmt_then_reindent_macro_bodies(&assembled)
}

/// Run rustfmt on the assembled function source, then re-indent macro
/// body lines to match the macro call's final column position. If
/// rustfmt is unavailable or fails, returns the input unchanged.
fn rustfmt_then_reindent_macro_bodies(src: &str) -> String {
    // Try rustfmt first. If it fails (rustfmt not on PATH, parse error,
    // etc.) fall through to the original sentinel-resolved output.
    let rustfmt_out = match std::process::Command::new("rustfmt")
        .arg("--emit=stdout")
        .arg("--edition=2021")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(src.as_bytes());
            }
            match child.wait_with_output() {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
                _ => return src.to_string(),
            }
        }
        Err(_) => return src.to_string(),
    };
    reindent_macro_bodies(&rustfmt_out)
}

/// For each macro call line ending `!(`, find the matching closing `)`
/// and re-indent every body line to match call_col+4. The closing `)`
/// is re-indented to match the call_col.
fn reindent_macro_bodies(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        // Detect a macro call line: ends with `!(` after the trimmed text.
        // Examples: `format!(`, `json!(`, `panic!(`. Ignore lines that
        // also contain `)` (single-line macro call — nothing to re-indent).
        if trimmed.ends_with("!(") && !trimmed.contains(')') {
            let call_indent: String = line.chars().take_while(|c| *c == ' ').collect();
            let arg_indent = format!("{}    ", call_indent);
            out.push(line.to_string());
            i += 1;
            // Track paren depth to find the matching `)`. Start at 1
            // (the `!(` we just consumed). Body lines re-indented; the
            // line containing the matching `)` becomes call_indent + `);`
            // (or call_indent + `)` if no semicolon, or call_indent + `)?`
            // etc — preserve the trailing tokens after the closing paren).
            let mut depth: i32 = 1;
            while i < lines.len() && depth > 0 {
                let body_line = lines[i];
                let body_trim = body_line.trim_start();
                // Scan for parens but respect strings. The macro body lines
                // we emit contain JSON-escaped strings; track in-string state.
                let (new_depth, closes_here, close_col) = scan_paren_balance(body_trim, depth);
                if closes_here {
                    // Closing line: indent at call_indent, then keep the
                    // trailing characters after the `)`. Reconstruct as
                    // call_indent + body_trim (preserve the `);` or `)?`).
                    out.push(format!("{}{}", call_indent, body_trim));
                    depth = new_depth;
                } else {
                    // Continuation body line: re-indent to arg_indent.
                    out.push(format!("{}{}", arg_indent, body_trim));
                    depth = new_depth;
                    let _ = close_col;
                }
                i += 1;
            }
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    let mut joined = out.join("\n");
    // Preserve trailing newline if original had one.
    if text.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Map a java type string (as it appears in the lowered.java source)
/// to a rust let-binding annotation. The java lift restricts let_type
/// emission to generic-constructor sites, so the only inputs we see
/// here are container-shaped: `java.util.ArrayList<T>`, `java.util.TreeSet<T>`,
/// `java.util.HashMap<K,V>`, etc. Plus the fully-qualified Jackson
/// JsonNode + unparameterised List/Map/Set that arose pre-#1391.
fn java_type_to_rust_let_annotation(ty: &str) -> String {
    let t = ty.trim();
    // Strip outer parens/generics for the family check, but preserve
    // them for parameter forwarding when present.
    let head = t.split('<').next().unwrap_or(t).trim();
    let params: Option<&str> = t.find('<').and_then(|i| {
        let rest = &t[i + 1..];
        rest.rfind('>').map(|j| &rest[..j])
    });
    fn map_param_to_rust(p: &str) -> String {
        // Recursively map parametric components — but for our
        // libprovekit-rpc-cross-platform corpus, the inner type is
        // always String/JsonNode/etc. Use the existing primitive map.
        let p = p.trim();
        match p {
            "String" | "java.lang.String" => "String".to_string(),
            "Integer" | "Long" | "java.lang.Long" | "java.lang.Integer" => "i64".to_string(),
            "com.fasterxml.jackson.databind.JsonNode" | "JsonNode" => "Value".to_string(),
            _ => p.to_string(),
        }
    }
    let container = match head {
        "java.util.ArrayList"
        | "ArrayList"
        | "java.util.List"
        | "List"
        | "java.util.LinkedList"
        | "LinkedList" => "Vec",
        "java.util.TreeSet" | "TreeSet" => "BTreeSet",
        "java.util.HashSet" | "HashSet" => "HashSet",
        "java.util.HashMap" | "HashMap" => "HashMap",
        "java.util.TreeMap" | "TreeMap" => "BTreeMap",
        _ => return String::new(), // Unrecognized — suppress annotation.
    };
    match params {
        Some(p) if !p.trim().is_empty() => {
            // Single-param container: List<T> / Set<T>.
            // Two-param container: Map<K,V>.
            let mut depth = 0i32;
            let mut parts: Vec<String> = Vec::new();
            let mut current = String::new();
            for ch in p.chars() {
                match ch {
                    '<' => {
                        depth += 1;
                        current.push(ch);
                    }
                    '>' => {
                        depth -= 1;
                        current.push(ch);
                    }
                    ',' if depth == 0 => {
                        parts.push(current.trim().to_string());
                        current.clear();
                    }
                    _ => current.push(ch),
                }
            }
            if !current.trim().is_empty() {
                parts.push(current.trim().to_string());
            }
            let mapped: Vec<String> = parts.iter().map(|s| map_param_to_rust(s)).collect();
            format!("{}<{}>", container, mapped.join(", "))
        }
        _ => {
            // No parameters supplied (e.g. bare `java.util.List`). For
            // the libprovekit corpus, source-side Vec was `Vec<Value>`
            // for JsonNode arrays and `Vec<String>` for paths; without
            // generics we can't tell. Suppress to avoid wrong types.
            String::new()
        }
    }
}

/// Scan a line of code (string-aware) and update paren depth. Returns
/// (new_depth, does_this_line_close_to_zero, col_of_closing_paren).
fn scan_paren_balance(line: &str, mut depth: i32) -> (i32, bool, usize) {
    let mut in_str = false;
    let mut esc = false;
    let bytes = line.as_bytes();
    let mut close_col = 0usize;
    let mut closed_here = false;
    for (col, &b) in bytes.iter().enumerate() {
        if esc {
            esc = false;
            continue;
        }
        if b == b'\\' {
            esc = true;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                closed_here = true;
                close_col = col;
            }
        }
    }
    (depth, closed_here, close_col)
}

/// Resolve macro-arg indent sentinels to depth-aware leading whitespace.
/// Sentinels marker: `\u{1F}MACRO_ARG_INDENT\u{1F}` for arg lines,
/// `\u{1F}MACRO_CLOSE_INDENT\u{1F}` for the closing paren.
///
/// For each line containing a sentinel, look back through previous lines
/// to find the macro call line (containing the matching `!(`) and use its
/// leading whitespace + 4 for args, same for closing paren.
fn resolve_macro_indent_sentinels(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len());
    // Track open macro-call lines: stack of leading-whitespace strings.
    let mut macro_stack: Vec<String> = Vec::new();
    for line in lines.iter() {
        let leading_ws: String = line.chars().take_while(|c| *c == ' ').collect();
        let trimmed = line.trim_start();
        // Detect macro call line (ends with `!(`).
        if trimmed.ends_with("!(") {
            macro_stack.push(leading_ws.clone());
        }
        if line.contains("\u{1F}MACRO_ARG_INDENT\u{1F}") {
            let call_indent = macro_stack
                .last()
                .cloned()
                .unwrap_or_else(|| "        ".to_string());
            let arg_indent = format!("{}    ", call_indent);
            out_lines.push(line.replace("\u{1F}MACRO_ARG_INDENT\u{1F}", &arg_indent));
            continue;
        }
        if line.contains("\u{1F}MACRO_CLOSE_INDENT\u{1F}") {
            let call_indent = macro_stack.pop().unwrap_or_else(|| "        ".to_string());
            out_lines.push(line.replace("\u{1F}MACRO_CLOSE_INDENT\u{1F}", &call_indent));
            continue;
        }
        // Detect closing paren that closes a top-level macro call (no
        // sentinel — bare `);` or `)` after macro args).
        if trimmed.starts_with(')') && !macro_stack.is_empty() {
            // Heuristic: if previous output line ended with a macro arg
            // or sentinel-resolved value, this `)` likely closes the
            // macro. Pop.
            macro_stack.pop();
        }
        out_lines.push(line.to_string());
    }
    out_lines.join("\n")
}

fn stub_body_for(concept_name: &str) -> String {
    let message = format!("provekit-bind canonical: {concept_name}");
    format!("panic!({})", rust_string_literal(&message))
}

fn rust_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:x}}}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn map_source_type(src: &str) -> String {
    let t = src.trim();
    // Java FQN translations back to rust (cross-language source-text mapping).
    // #1391 follow-on: handles the rust→java→rust cycle's residual type
    // leakage when param_sort_cids isn't populated by the java lift.
    match t {
        "" => return "()".to_string(),
        "void" | "None" | "()" => return "()".to_string(),
        "long" | "int" | "i64" | "u64" => return "i64".to_string(),
        "i32" | "u32" => return "i32".to_string(),
        "short" | "i16" | "u16" => return "i16".to_string(),
        "byte" | "char" | "i8" | "u8" => return "i8".to_string(),
        "boolean" | "bool" => return "bool".to_string(),
        "double" | "float" | "f64" | "f32" => return "f64".to_string(),
        "String" | "str" | "&str" | "&String" => return "String".to_string(),
        "list" | "List" | "list[int]" | "list[i64]" => return "&[i64]".to_string(),
        // Jackson / java.util / provekit runtime FQNs from the java emit.
        "com.fasterxml.jackson.databind.JsonNode" | "JsonNode" => return "Value".to_string(),
        "java.util.List<String>" => return "&[String]".to_string(),
        "java.nio.file.Path" | "Path" => return "&Path".to_string(),
        "byte[]" => return "&[u8]".to_string(),
        _ => {}
    }
    // Parametric java→rust translations.
    if let Some(inner) = t
        .strip_prefix("java.util.List<")
        .and_then(|s| s.strip_suffix('>'))
    {
        return format!("&[{}]", map_source_type(inner));
    }
    if let Some(inner) = t
        .strip_prefix("com.provekit.runtime.Result<")
        .or_else(|| t.strip_prefix("Result<"))
        .and_then(|s| s.strip_suffix('>'))
    {
        // Split top-level comma; map both sides.
        let mut depth = 0i32;
        let mut split = None;
        for (i, c) in inner.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => {
                    split = Some(i);
                    break;
                }
                _ => {}
            }
        }
        if let Some(i) = split {
            let ok = map_source_type(inner[..i].trim());
            let err = map_source_type(inner[i + 1..].trim());
            return format!("Result<{}, {}>", ok, err);
        }
    }
    if t == "com.provekit.runtime.SumVariant" || t == "SumVariant" {
        return "LiftError".to_string();
    }
    t.to_string()
}

/// #1369: cross-language rust realize parametric dispatch.
///
/// Translate a concept-hub sort CID to rust syntax. Primitive CIDs map
/// directly; composite CIDs decompose via the expansions map and dispatch
/// on constructor. Mirrors the java SugarRealizer's mapConceptHubSortCidToJava.
fn map_concept_hub_sort_cid_to_rust(
    cid: &str,
    expansions: &std::collections::HashMap<String, ParametricExpansion>,
) -> Option<String> {
    if cid.is_empty() {
        return None;
    }
    // Primitive concept-hub sorts.
    let primitive = match cid {
        // concept:Bool
        "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"
            => Some("bool"),
        // concept:Int — default i64
        "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
            => Some("i64"),
        // concept:Float
        "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
            => Some("f64"),
        // concept:String
        "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
            => Some("String"),
        // concept:Unit
        "blake3-512:47682b09e5dba71f563db6249c6cb352f7d540986dc7f4cd8d4fb1aa6d9a503064033ee3eb9f36ee6f9e000f700f2f030ebfcfe2b2b8b7e81a345b0d56551f1b"
            => Some("()"),
        // concept:Bytes
        "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
            => Some("Vec<u8>"),
        // concept:Json
        "blake3-512:702064722b23410fde0d1fd7afac165bf5914441d67abe1e19d63b0e8fe8117296d2677cc721ad096b8b3bb82d178af699bf14fd70bfb18756c5bed6f4434108"
            => Some("serde_json::Value"),
        _ => None,
    };
    if let Some(s) = primitive {
        return Some(s.to_string());
    }
    // Composite parametric CID — decompose via expansion table.
    let exp = expansions.get(cid)?;
    // Ref<T> → "&mut T_resolved"
    if exp.constructor_cid == "blake3-512:37d8efe0ce6321d1a16f80aa06cbdf056c846b8a99613731e8d64d9581af61bc517fd8c87daaff2c817585a7dfd763e09ed729fdc71d25fe16fb1b2e6ca33534" {
        let inner = exp.arg_cids.first()?;
        let inner_rust = map_concept_hub_sort_cid_to_rust(inner, expansions)?;
        return Some(format!("&mut {inner_rust}"));
    }
    // List<T> → "Vec<T_resolved>"
    if exp.constructor_cid == "blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2" {
        let inner = exp.arg_cids.first()?;
        let inner_rust = map_concept_hub_sort_cid_to_rust(inner, expansions)?;
        return Some(format!("Vec<{inner_rust}>"));
    }
    None
}

/// Carrier-side parametric sort expansion (#1369). Same shape as
/// libprovekit::core::lower_plugin::ParametricSortExpansion. Re-declared
/// locally here to keep realize-rust-core independent of libprovekit.
#[derive(Debug, Clone)]
pub struct ParametricExpansion {
    pub cid: String,
    pub constructor_cid: String,
    pub arg_cids: Vec<String>,
}

/// Parse the parametric_sort_expansions field from the realize RPC params
/// into a (composite_cid → expansion) map for dispatch.
fn parse_parametric_expansions(
    value: Option<&Value>,
) -> std::collections::HashMap<String, ParametricExpansion> {
    let mut map = std::collections::HashMap::new();
    let Some(arr) = value.and_then(Value::as_array) else {
        return map;
    };
    for item in arr {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let cid = obj
            .get("cid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ctor = obj
            .get("constructor_cid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if cid.is_empty() || ctor.is_empty() {
            continue;
        }
        let arg_cids: Vec<String> = obj
            .get("arg_cids")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        map.insert(
            cid.clone(),
            ParametricExpansion {
                cid,
                constructor_cid: ctor,
                arg_cids,
            },
        );
    }
    map
}

fn concept_matches(entry_name: &str, request_name: &str) -> bool {
    entry_name == request_name
        || entry_name
            .strip_prefix("concept:")
            .is_some_and(|name| name == request_name)
        || request_name
            .strip_prefix("concept:")
            .is_some_and(|name| name == entry_name)
}

fn mode_matches(entry_mode: Option<&str>, request_mode: Option<&str>) -> bool {
    match entry_mode {
        Some(entry) => request_mode.is_some_and(|request| request == entry),
        None => true,
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn value_array(value: Option<&Value>) -> Vec<Value> {
    value.and_then(Value::as_array).cloned().unwrap_or_default()
}

fn error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn entries() -> &'static [BodyTemplateEntry] {
    static ENTRIES: OnceLock<Vec<BodyTemplateEntry>> = OnceLock::new();
    ENTRIES
        .get_or_init(|| {
            find_repo_file(Path::new(BODY_TEMPLATE_REL))
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .map(|root| parse_entries(&root))
                .unwrap_or_default()
        })
        .as_slice()
}

fn parse_entries(root: &Value) -> Vec<BodyTemplateEntry> {
    root.pointer("/header/content/entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_entry)
                .collect::<Vec<BodyTemplateEntry>>()
        })
        .unwrap_or_default()
}

fn parse_entry(item: &Value) -> Option<BodyTemplateEntry> {
    let template = item.get("emission_template")?;
    let guard = item.get("signature_guard");
    let requires_param_types = guard
        .and_then(|g| g.get("requires_param_types"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    Some(BodyTemplateEntry {
        concept_name: item.get("concept_name")?.as_str()?.to_string(),
        mode: item.get("mode").and_then(Value::as_str).map(str::to_string),
        realization_kind: item
            .get("realization_kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        template_kind: template.get("kind")?.as_str()?.to_string(),
        template: template.get("template")?.as_str()?.to_string(),
        loss_record_contribution: item.get("loss_record_contribution").cloned(),
        min_params: guard
            .and_then(|g| g.get("min_params"))
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        max_params: guard
            .and_then(|g| g.get("max_params"))
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        requires_param_types,
        requires_return_type: guard
            .and_then(|g| g.get("requires_return_type"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn find_repo_file(relative: &Path) -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Some(root) = std::env::var_os("PROVEKIT_REPO_ROOT") {
        bases.push(PathBuf::from(root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.extend(parent.ancestors().map(Path::to_path_buf));
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    bases.extend(manifest_dir.ancestors().map(Path::to_path_buf));

    for base in bases {
        let candidate = base.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Catalog-driven dispatch (#1391): the realizations catalog at
    /// menagerie/concept-shapes/catalog/realizations/ must contain the
    /// six operation-realization mementos minted in this PR. Forward
    /// (concept → rhs op) and reverse (rhs op → concept) must round-trip.
    #[test]
    fn operation_realization_catalog_round_trips() {
        let probes: &[(&str, &str)] = &[
            ("concept:utf8-encode", "rust:str-as-bytes"),
            ("concept:json-text-coerce", "rust:serde-value-as-str"),
            ("concept:option-is-some", "rust:option-is-some"),
            ("concept:list-create", "rust:vec-new"),
            ("concept:map-create", "rust:hashmap-new"),
            ("concept:format-string-interp", "rust:format-macro"),
        ];
        for (concept, rhs) in probes {
            let got_rhs = operation_realization_catalog::rust_op_for(concept);
            assert_eq!(
                got_rhs.as_deref(),
                Some(*rhs),
                "forward lookup {} → {:?} (expected {})",
                concept,
                got_rhs,
                rhs
            );
            let got_concept = operation_realization_catalog::concept_for_rust_op(rhs);
            assert_eq!(
                got_concept.as_deref(),
                Some(*concept),
                "reverse lookup {} → {:?} (expected {})",
                rhs,
                got_concept,
                concept
            );
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn resolved_return_call_new_with_literal_args(args: Vec<Value>) -> Value {
        serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "Arc::new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": args,
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        })
    }

    #[test]
    fn renders_identity_from_body_template() {
        let rendered = emit_stub(
            "wrap_identity",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
            "identity",
        );

        assert!(!rendered.is_stub);
        assert_eq!(rendered.extension, "rs");
        assert_eq!(
            rendered.source,
            "pub fn wrap_identity(x: i64) -> i64 {\n    x\n}\n"
        );
        assert!(rendered.emitted_artifact_cid.starts_with("blake3-512:"));
    }

    #[test]
    fn renders_contract_observation_witness_body_template() {
        let rendered = emit_stub_with_mode(
            "observe_contract",
            &strings(&["callsite_cid", "contract_cid", "mode"]),
            &strings(&["String", "String", "String"]),
            "ContractObservationResult",
            "concept:contract-observation",
            Some("witness"),
        );

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("provekit_witness::observe"));
        assert!(rendered.source.contains("callsite_cid"));
        assert!(rendered.source.contains("contract_cid"));
        assert!(rendered.source.contains("mode"));
    }

    #[test]
    fn refuses_contract_observation_witness_template_for_gate_mode() {
        let rendered = emit_stub_with_mode(
            "observe_contract",
            &strings(&["callsite_cid", "contract_cid", "mode"]),
            &strings(&["String", "String", "String"]),
            "ContractObservationResult",
            "concept:contract-observation",
            Some("gate"),
        );

        assert!(rendered.is_stub);
        assert!(!rendered.source.contains("provekit_witness::observe"));
    }

    #[test]
    fn renders_unit_without_return_arrow() {
        let rendered = emit_stub("do_nothing", &[], &[], "()", "unit");

        assert!(!rendered.is_stub);
        assert_eq!(rendered.source, "pub fn do_nothing() {\n    ()\n}\n");
    }

    #[test]
    fn emits_rust_comment_from_comment_term_shape() {
        let term_shape = serde_json::json!({
            "concept_name": "concept:comment",
            "args": [{"kind": "literal", "value": "// keep me exactly"}],
        });
        let rendered = emit_from_term_shape(&term_shape, "comment_only", &[], &[], "()");

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn comment_only() {\n    // keep me exactly\n}\n"
        );
    }

    #[test]
    fn emits_rust_comment_after_python_comment_hop_surface() {
        let term_shape = serde_json::json!({
            "concept_name": "concept:comment",
            "args": [{"kind": "literal", "value": "// byte exact route"}],
        });
        let rendered = emit_from_term_shape(&term_shape, "comment_hop", &[], &[], "()");

        assert_eq!(
            rendered.source,
            "pub fn comment_hop() {\n    // byte exact route\n}\n"
        );
    }

    #[test]
    fn rpc_term_shape_bindings_render_arithmetic_sequence_despite_unnamed_concept() {
        let term_shape = serde_json::json!({
            "concept_name": "concept:seq",
            "op_cid": "blake3-512:seq",
            "args": [
                {
                    "concept_name": "concept:assign",
                    "op_cid": "blake3-512:assign-total",
                    "args": [
                        {},
                        {
                            "concept_name": "concept:add",
                            "op_cid": "blake3-512:add",
                            "args": [{}, {}]
                        }
                    ]
                },
                {
                    "concept_name": "concept:assign",
                    "op_cid": "blake3-512:assign-scaled",
                    "args": [
                        {},
                        {
                            "concept_name": "concept:mul",
                            "op_cid": "blake3-512:mul",
                            "args": [{}, {}]
                        }
                    ]
                },
                {
                    "concept_name": "concept:assign",
                    "op_cid": "blake3-512:assign-reduced",
                    "args": [
                        {},
                        {
                            "concept_name": "concept:sub",
                            "op_cid": "blake3-512:sub",
                            "args": [{}, {}]
                        }
                    ]
                }
            ]
        });
        let operand_bindings = serde_json::json!([
            {"position": [0, 0], "symbol": "total"},
            {"position": [0, 1, 0], "symbol": "a"},
            {"position": [0, 1, 1], "symbol": "b"},
            {"position": [1, 0], "symbol": "scaled"},
            {"position": [1, 1, 0], "symbol": "total"},
            {"position": [1, 1, 1], "symbol": "2"},
            {"position": [2, 0], "symbol": "reduced"},
            {"position": [2, 1, 0], "symbol": "scaled"},
            {"position": [2, 1, 1], "symbol": "1"}
        ]);
        let response = dispatch(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "UNNAMED-CONCEPT-1",
                "source_function_name": "compute_sum",
                "params": ["a", "b"],
                "param_types": ["int", "int"],
                "return_type": "int",
                "concept_name": "UNNAMED-CONCEPT-1",
                "term_shape": term_shape,
                "operand_bindings": operand_bindings
            }
        }));

        assert_eq!(response["result"]["is_stub"], false);
        let source = response["result"]["source"]
            .as_str()
            .expect("realized source");
        // @sugar attribute is emitted universally (every realization carries
        // its concept binding for re-lift / cycle-invariance — ecc0ba27e), even
        // for bare/UNNAMED concept names.
        assert!(
            source.contains("#[provekit::sugar(\n    concept = \"UNNAMED-CONCEPT-1\","),
            "{source}"
        );
        assert!(
            source.contains("pub fn compute_sum(a: int, b: int) -> int"),
            "{source}"
        );
        assert!(source.contains("let total: i64 = a + b;"), "{source}");
        assert!(source.contains("let scaled: i64 = total * 2;"), "{source}");
        assert!(
            source.contains("let reduced: i64 = scaled - 1;"),
            "{source}"
        );
        assert!(source.contains("reduced\n}"), "{source}");
        assert!(!source.contains("panic!"), "{source}");
    }

    #[test]
    fn falls_back_to_deterministic_stub_for_missing_template() {
        let rendered = emit_stub(
            "unknown_cell",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
            "missing-cell",
        );

        assert!(rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn unknown_cell(x: i64) -> i64 {\n    panic!(\"provekit-bind canonical: missing-cell\")\n}\n"
        );
    }

    #[test]
    fn rpc_emits_proc_macro_invocations_before_realized_function() {
        let response = dispatch(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "traced",
                "params": ["x"],
                "param_types": ["i64"],
                "return_type": "i64",
                "concept_name": "identity",
                "procMacroInvocations": [{
                    "concept_name": "concept:proc-macro-invocation",
                    "macro_path": "instrument",
                    "token_stream": "#[instrument]"
                }]
            }
        }));

        let source = response["result"]["source"]
            .as_str()
            .expect("realized source");
        // proc-macro invocations precede the @sugar attribute, which is emitted
        // universally (concept binding for re-lift / cycle-invariance — ecc0ba27e).
        assert_eq!(
            source,
            "#[instrument]\n#[provekit::sugar(\n    concept = \"identity\",\n    library = \"libprovekit-rpc-cross-platform\",\n    loss = [],\n)]\npub fn traced(x: i64) -> i64 {\n    x\n}\n"
        );
    }

    #[test]
    fn emits_value_null_from_resolved_return_call_new_literal_shape() {
        let resolved = serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": ["Null"],
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "null",
            &[],
            &[],
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn null() -> Arc<Value> {\n    new(Value::Null)\n}\n"
        );
    }

    #[test]
    fn emits_value_null_from_resolved_call_new_with_receiver_prefix() {
        let resolved = serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "Arc::new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": ["Null"],
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "null",
            &[],
            &[],
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn null() -> Arc<Value> {\n    Arc::new(Value::Null)\n}\n"
        );
    }

    #[test]
    fn emits_value_bool_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:Bool(Value::Bool, [b])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "boolean",
            &strings(&["b"]),
            &strings(&["bool"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn boolean(b: bool) -> Arc<Value> {\n    Arc::new(Value::Bool(b))\n}\n"
        );
    }

    #[test]
    fn emits_value_integer_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:Integer(Value::Integer, [n])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "integer",
            &strings(&["n"]),
            &strings(&["i64"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn integer(n: i64) -> Arc<Value> {\n    Arc::new(Value::Integer(n))\n}\n"
        );
    }

    #[test]
    fn emits_value_string_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:String(Value::String, [s])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "string",
            &strings(&["s"]),
            &strings(&["String"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn string(s: String) -> Arc<Value> {\n    Arc::new(Value::String(s))\n}\n"
        );
    }

    #[test]
    fn emits_value_string_from_resolved_call_new_variant_method_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:String(Value::String, [method:into(s, [])])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "string<S: Into<String>>",
            &strings(&["s"]),
            &strings(&["S"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn string<S: Into<String>>(s: S) -> Arc<Value> {\n    Arc::new(Value::String(s.into()))\n}\n"
        );
    }

    #[test]
    fn unsupported_resolved_shape_falls_back_to_stub() {
        let resolved = serde_json::json!({
            "node": {
                "args": [],
                "kind": "concept:op-application",
                "op_definition_cid": "unsupported-concept",
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "unknown_cell",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
        );

        assert!(rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn unknown_cell(x: i64) -> i64 {\n    panic!(\"provekit-bind canonical: unsupported-concept\")\n}\n"
        );
    }

    #[test]
    fn emit_from_resolved_uses_operator_library_binding_before_stub_fallback() {
        let _guard = env_lock().lock().expect("env lock");
        let root = temp_operator_root("realize_library_binding");
        std::fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
        std::fs::write(
            root.join(".provekit").join("library-bindings.json"),
            r#"{
  "language": "rust",
  "bindings": {
    "concept:custom-echo": "rust-demo"
  }
}
"#,
        )
        .expect("write library bindings");
        let template_dir = root
            .join("menagerie")
            .join("rust-language-signature")
            .join("specs")
            .join("body-templates");
        std::fs::create_dir_all(&template_dir).expect("create body template dir");
        std::fs::write(
            template_dir.join("rust-canonical-bodies-demo.json"),
            r#"{
  "header": {
    "content": {
      "entries": [
        {
          "concept_name": "concept:custom-echo",
          "emission_template": {
            "kind": "verbatim",
            "template": "${param0}.clone()"
          },
          "signature_guard": {
            "min_params": 1,
            "max_params": 1,
            "requires_return_type": "String"
          }
        }
      ]
    }
  }
}
"#,
        )
        .expect("write body template");
        std::env::set_var("PROVEKIT_OPERATOR_ROOT", &root);
        std::env::set_var("PROVEKIT_REPO_ROOT", &root);

        let resolved = serde_json::json!({
            "node": {
                "args": [],
                "kind": "concept:op-application",
                "op_definition_cid": "concept:custom-echo",
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "echo",
            &strings(&["value"]),
            &strings(&["String"]),
            "String",
        );

        std::env::remove_var("PROVEKIT_OPERATOR_ROOT");
        std::env::remove_var("PROVEKIT_REPO_ROOT");
        let _ = std::fs::remove_dir_all(root);

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn echo(value: String) -> String {\n    value.clone()\n}\n"
        );
    }

    #[test]
    fn dispatch_invoke_returns_rpc_result_shape() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "toggle",
                "params": ["flag"],
                "param_types": ["bool"],
                "return_type": "bool",
                "concept_name": "bool-cell"
            }
        });

        let response = dispatch(&request);
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["extension"], "rs");
        assert_eq!(response["result"]["is_stub"], false);
        // @sugar attribute is emitted universally (every realization carries its
        // concept binding for re-lift / cycle-invariance — ecc0ba27e).
        assert_eq!(
            response["result"]["source"],
            "#[provekit::sugar(\n    concept = \"bool-cell\",\n    library = \"libprovekit-rpc-cross-platform\",\n    loss = [],\n)]\npub fn toggle(flag: bool) -> bool {\n    !flag\n}\n"
        );
    }

    /// Pipeline-level private-round-trip regression (PR #1455 review).
    ///
    /// A private function lifted from real source is encoded by `bind` as
    /// `NamedTerm.visibility = String::new()`, and `realize_spec_from_named_term`
    /// forwards that into the dispatch spec's `visibility` field as a PRESENT
    /// empty string (`"visibility": ""`). This RPC shape — visibility present
    /// and empty — is exactly what the real bind->realize pipeline emits for a
    /// private fn. The realizer MUST treat it as private (bare `fn`) and MUST
    /// NOT over-promote it to `pub`. (The earlier visibility fix defaulted an
    /// unset visibility to `pub` but conflated absent with present-empty,
    /// silently materializing private APIs as public — the bug this guards.)
    #[test]
    fn dispatch_with_present_empty_visibility_emits_private_fn_not_pub() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "secret",
                "params": ["flag"],
                "param_types": ["bool"],
                "return_type": "bool",
                "concept_name": "bool-cell",
                // bind's PRIVATE encoding: present, empty.
                "visibility": ""
            }
        });

        let response = dispatch(&request);
        let source = response["result"]["source"]
            .as_str()
            .expect("realized source");
        assert!(
            source.contains("\nfn secret(flag: bool) -> bool"),
            "private fn (visibility present+empty) must emit a bare `fn`, not be \
             over-promoted to `pub`: {source}"
        );
        assert!(
            !source.contains("pub fn secret"),
            "private fn was over-promoted to `pub`: {source}"
        );
    }

    /// Companion: an explicitly-public spec (`"visibility": "pub"`) still emits
    /// `pub fn`. Together with `dispatch_invoke_returns_rpc_result_shape`
    /// (visibility ABSENT => default `pub`) this pins all three points of the
    /// absent / present-empty / present-pub contract.
    #[test]
    fn dispatch_with_present_pub_visibility_emits_pub_fn() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "exposed",
                "params": ["flag"],
                "param_types": ["bool"],
                "return_type": "bool",
                "concept_name": "bool-cell",
                "visibility": "pub"
            }
        });

        let response = dispatch(&request);
        let source = response["result"]["source"]
            .as_str()
            .expect("realized source");
        assert!(
            source.contains("pub fn exposed(flag: bool) -> bool"),
            "explicit `pub` visibility must emit `pub fn`: {source}"
        );
    }

    #[test]
    fn renders_dynamic_dispatch_boundary_realization() {
        let rendered = dynamic_dispatch_render();

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("&dyn Fn(i64) -> i64"));
        assert!(rendered.source.contains("dispatched(arg)"));
    }

    #[test]
    fn records_dynamic_dispatch_boundary_loss_without_sugar() {
        let rendered = dynamic_dispatch_render();

        assert_boundary_loss(
            &rendered,
            "rust-dyn-trait-preserves-call-contract-loses-concrete-receiver-type",
        );
    }

    #[test]
    fn discriminates_dynamic_dispatch_boundary_from_missing_concept() {
        let rendered = dynamic_dispatch_render();
        let missing = emit_stub(
            "call_dyn",
            &strings(&["target", "arg"]),
            &strings(&["&dyn Fn(i64) -> i64", "i64"]),
            "i64",
            "concept:dynamic-dispatch-missing",
        );

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains("&dyn Fn(i64) -> i64"));
        assert!(!missing.source.contains("dispatched(arg)"));
    }

    #[test]
    fn renders_closure_boundary_realization() {
        let rendered = boundary_render("call_closure", "concept:closure");

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let f = move ||"));
        assert!(rendered.source.contains("f()"));
    }

    #[test]
    fn records_closure_boundary_loss_without_sugar() {
        let rendered = boundary_render("call_closure", "concept:closure");

        assert_boundary_loss(
            &rendered,
            "rust-closure-preserves-capture-and-call-loses-explicit-environment-record",
        );
    }

    #[test]
    fn discriminates_closure_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:closure", "let f = move ||");
    }

    #[test]
    fn renders_iterator_boundary_realization() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let mut iter = items.iter();"));
        assert!(rendered
            .source
            .contains("iter.next().copied().unwrap_or_default()"));
    }

    #[test]
    fn records_iterator_boundary_loss_without_sugar() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );

        assert_boundary_loss(
            &rendered,
            "rust-iterator-preserves-next-protocol-loses-container-identity",
        );
    }

    #[test]
    fn discriminates_iterator_boundary_from_missing_concept() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );
        let missing = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator-missing",
        );

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains("items.iter()"));
        assert!(!missing.source.contains("items.iter()"));
    }

    #[test]
    fn renders_generic_instantiation_boundary_realization() {
        let rendered = boundary_render("generic_id", "concept:generic-instantiation");

        assert!(!rendered.is_stub);
        assert!(rendered
            .source
            .contains("std::convert::identity::<i64>(arg)"));
    }

    #[test]
    fn records_generic_instantiation_boundary_loss_without_sugar() {
        let rendered = boundary_render("generic_id", "concept:generic-instantiation");

        assert_boundary_loss(
            &rendered,
            "rust-monomorphization-preserves-type-application-loses-runtime-genericity",
        );
    }

    #[test]
    fn discriminates_generic_instantiation_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:generic-instantiation", "identity::<i64>");
    }

    #[test]
    fn renders_reference_boundary_realization() {
        let rendered = boundary_render("borrow_value", "concept:reference");

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let r: &i64 = &arg;"));
        assert!(rendered.source.contains("*r"));
    }

    #[test]
    fn records_reference_boundary_loss_without_sugar() {
        let rendered = boundary_render("borrow_value", "concept:reference");

        assert_boundary_loss(
            &rendered,
            "rust-reference-preserves-aliasing-and-borrow-loses-cross-language-ownership",
        );
    }

    #[test]
    fn discriminates_reference_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:reference", "let r: &i64 = &arg;");
    }

    fn boundary_render(function: &str, concept_name: &str) -> Realization {
        emit_stub(
            function,
            &strings(&["arg"]),
            &strings(&["i64"]),
            "i64",
            concept_name,
        )
    }

    fn dynamic_dispatch_render() -> Realization {
        emit_stub(
            "call_dyn",
            &strings(&["target", "arg"]),
            &strings(&["&dyn Fn(i64) -> i64", "i64"]),
            "i64",
            "concept:dynamic-dispatch",
        )
    }

    fn assert_boundary_loss(rendered: &Realization, loss_name: &str) {
        assert!(!rendered.is_stub);
        assert!(rendered.used_sugars.is_empty());
        assert!(!rendered.source.contains("provekit-concept:"));
        let names = rendered
            .observed_loss_record
            .pointer("/loss_record_contribution/value")
            .and_then(Value::as_object)
            .map(|value| {
                value
                    .values()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(names.contains(&loss_name));
    }

    fn assert_boundary_discriminates(concept_name: &str, expected_source: &str) {
        let rendered = boundary_render("boundary", concept_name);
        let missing = boundary_render("boundary", &format!("{concept_name}-missing"));

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains(expected_source));
        assert!(!missing.source.contains(expected_source));
    }

    // concept:call lowering tests.
    //
    // walk_rpc now emits callee identity inside concept:call args:
    //   - Expr::Call:       args[0] = {kind:"path", text:"..."}, args[1..] = call args
    //   - Expr::MethodCall: args[0] = receiver, args[1] = {kind:"method", text:"..."}, args[2..] = call args
    //
    // These tests pin the new lowering behavior.  Pre-change both tests asserted
    // is_stub == true (callee absent).  Post-change they assert is_stub == false and
    // check the emitted body contains the expected call text.

    #[test]
    fn concept_call_statement_stays_refused_when_callee_absent() {
        // Renamed test: now verifies the CALLEE-PRESENT path succeeds.
        // Mirrors the blake3 shim seq[1]: `hasher.update(bytes);`
        // MethodCall -> concept:call with new layout:
        //   args[0] = receiver leaf (hasher, via operand_binding at [0,0])
        //   args[1] = {kind:"method", text:"update"}
        //   args[2] = bytes arg leaf (via operand_binding at [0,2])
        let term_shape = serde_json::json!({
            "concept_name": "concept:seq",
            "op_cid": "blake3-512:seq",
            "args": [
                {
                    "concept_name": "concept:call",
                    "op_cid": "blake3-512:fa2fd7c6f33492f270282faf69a89e21bb9988d8d0d9678d253c19aa00a977bf1158396b870f2160b718835c6189b51a97b848af8946d43e4244728f0b7e870c",
                    "args": [
                        {},
                        {"kind": "method", "text": "update"},
                        {}
                    ]
                }
            ]
        });
        // Receiver at [0,0], method leaf at [0,1] has no binding (correct),
        // bytes arg at [0,2].
        let operand_bindings = serde_json::json!([
            {"position": [0, 0], "symbol": "hasher"},
            {"position": [0, 2], "symbol": "bytes"}
        ]);
        let response = dispatch(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "concept:blake3-hasher-update",
                "source_function_name": "blake3_hasher_update",
                "params": ["hasher", "bytes"],
                "param_types": ["&mut Hasher", "&[u8]"],
                "return_type": "()",
                "concept_name": "concept:blake3-hasher-update",
                "term_shape": term_shape,
                "operand_bindings": operand_bindings
            }
        }));
        // Callee present: expression lowering succeeds, body wraps with `;`.
        let source = response["result"]["source"].as_str().unwrap_or("");
        assert_eq!(
            response["result"]["is_stub"], false,
            "concept:call with method ident must lower to real body; got source: {}",
            source
        );
        assert!(
            source.contains("hasher.update(bytes)"),
            "expected 'hasher.update(bytes)' in source; got: {}",
            source
        );
    }

    #[test]
    fn concept_call_chained_expression_stays_refused_when_callee_absent() {
        // Renamed test: now verifies a free-function call (Expr::Call) succeeds.
        // Mirrors the blake3 shim: `blake3::Hasher::new()`
        // Call -> concept:call with new layout:
        //   args[0] = {kind:"path", text:"blake3::Hasher::new"}
        //   (no call arguments)
        let term_shape = serde_json::json!({
            "concept_name": "concept:seq",
            "op_cid": "blake3-512:seq",
            "args": [
                {
                    "concept_name": "concept:assign",
                    "op_cid": "blake3-512:assign",
                    "args": [
                        {},
                        {
                            "concept_name": "concept:call",
                            "op_cid": "blake3-512:fa2fd7c6f33492f270282faf69a89e21bb9988d8d0d9678d253c19aa00a977bf1158396b870f2160b718835c6189b51a97b848af8946d43e4244728f0b7e870c",
                            "args": [
                                {"kind": "path", "text": "blake3::Hasher::new"}
                            ]
                        }
                    ]
                }
            ]
        });
        // The assign target (hasher) is at [0,0]; the call has no arg bindings.
        let operand_bindings = serde_json::json!([
            {"position": [0, 0], "symbol": "hasher"}
        ]);
        let response = dispatch(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 43,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "concept:blake3-hasher-new",
                "source_function_name": "blake3_hasher_new",
                "params": [],
                "param_types": [],
                "return_type": "Hasher",
                "concept_name": "concept:blake3-hasher-new",
                "term_shape": term_shape,
                "operand_bindings": operand_bindings
            }
        }));
        // Callee present: free-function call lowers to real body.
        let source = response["result"]["source"].as_str().unwrap_or("");
        assert_eq!(
            response["result"]["is_stub"], false,
            "concept:call with path callee must lower to real body; got source: {}",
            source
        );
        assert!(
            source.contains("blake3::Hasher::new()"),
            "expected 'blake3::Hasher::new()' in source; got: {}",
            source
        );
    }

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn temp_operator_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{name}_{nanos}"));
        std::fs::create_dir_all(&root).expect("create temp operator root");
        root
    }
}
