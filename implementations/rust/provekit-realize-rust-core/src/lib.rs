// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::realization_tags::tag_sugar_carrier;
use serde_json::{json, Value};

pub mod platform_semantics;

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
        let mut tail_ctx = ShapeLoweringContext::new(
            params, param_types, return_type, operand_bindings,
        );
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
        Some(body) => emit_function(
            function_name,
            params,
            param_types,
            return_type,
            &body,
            false,
        ),
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
            // Tail-expression preference: when this is the LAST child of
            // the function-root seq AND the function returns non-unit,
            // try the EXPRESSION form first (no `;`). This matches rust's
            // tail-expression convention — `Ok(build_ir_document(...))`
            // as the last line, not `Ok(...);` followed by a synthesized
            // tail. Falls through to body form if expression-lift fails.
            let is_function_tail = position.is_empty()
                && index == last_index
                && returns_non_unit;
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
            let expression = lower_term_shape_expression(child, context, &child_position)?;
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
        let value = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
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
        // Omit type annotation when value.type_name is empty — Rust's local type
        // inference covers it (happens for concept:call results, array-repeat, etc.).
        if value.type_name.is_empty() {
            return Some(format!("{} {} = {};", let_kw, target.text, value.text));
        }
        return Some(format!(
            "{} {}: {} = {};",
            let_kw, target.text, value.type_name, value.text
        ));
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
            let call_args: Vec<String> = args[1..]
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    lower_term_shape_expression(arg, context, &append_position(position, i + 1))
                        .map(|e| e.text)
                })
                .collect::<Option<Vec<_>>>()?;
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
        if args.len() >= 2
            && args[1].get("kind").and_then(Value::as_str) == Some("method")
        {
            let receiver = lower_term_shape_expression(
                first,
                context,
                &append_position(position, 0),
            )?;
            let method_text = args[1].get("text").and_then(Value::as_str)?;
            let call_args: Vec<String> = args[2..]
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    lower_term_shape_expression(arg, context, &append_position(position, i + 2))
                        .map(|e| e.text)
                })
                .collect::<Option<Vec<_>>>()?;
            let text = format!("{}.{}({})", receiver.text, method_text, call_args.join(", "));
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
        let body = lower_term_shape_expression(
            body_shape,
            context,
            &append_position(position, 0),
        )?;
        let text = format!("|{}| {}", param_names.join(", "), body.text);
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
                Some(lower_block_or_expr(args[2], context, &append_position(position, 2))?)
            }
        } else {
            None
        };
        let text = match else_text {
            Some(e) => format!("if {} {{ {} }} else {{ {} }}", cond.text, then_text, e),
            None => format!("if {} {{ {} }}", cond.text, then_text),
        };
        return Some(ShapeExpression { text, type_name: String::new() });
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
    if concept_name == "concept:for-each" {
        if args.len() != 3 {
            return None;
        }
        let var = args[0].get("text").and_then(Value::as_str)?.to_string();
        let iter = lower_term_shape_expression(args[1], context, &append_position(position, 1))?;
        let body = lower_block_or_expr(args[2], context, &append_position(position, 2))?;
        return Some(ShapeExpression {
            text: format!("for {} in {} {{ {} }}", var, iter.text, body),
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
            // Match arms are EXPRESSIONS — strip any trailing `;` that
            // lower_block_or_expr left over from statement-form lowering.
            // Without this we emit `body();,` which is invalid rust.
            let body_trimmed = body_text.trim_end().trim_end_matches(';');
            arms_text.push(format!("{} => {},", pattern_text, body_trimmed));
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
        return Some(ShapeExpression {
            text: format!("{}!({})", path, tokens),
            type_name: String::new(),
        });
    }
    if concept_name == "concept:try" {
        // try-body + catches. Rust doesn't natively have try-catch; we emit
        // the body as-is (rust uses Result for error handling, declared as
        // an effect on the binding, not in the body shape).
        if args.is_empty() {
            return None;
        }
        let body = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        return Some(body);
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
        return Some(ShapeExpression { text: String::new(), type_name: "()".to_string() });
    }
    if concept_name == "concept:cast" {
        if args.len() != 2 {
            return None;
        }
        let value = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
        let type_text = args[1].get("text").and_then(Value::as_str)?;
        return Some(ShapeExpression {
            text: format!("{} as {}", value.text, type_text),
            type_name: type_text.to_string(),
        });
    }
    if concept_name == "concept:index" {
        if args.len() != 2 {
            return None;
        }
        let receiver = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
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
        let receiver = lower_term_shape_expression(args[0], context, &append_position(position, 0))?;
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
        return Some(literal_term_with_width(
            shape.get("value").unwrap_or(&Value::Null),
            width,
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
        let left = &args[0].text;
        let right = &args[1].text;
        return match op {
            "add" => Some(format!("({left}) + ({right})")),
            "sub" => Some(format!("({left}) - ({right})")),
            "mul" => Some(format!("({left}) * ({right})")),
            "div" => Some(format!("({left}) / ({right})")),
            "mod" => Some(format!("({left}) % ({right})")),
            "eq" => Some(format!("({left}) == ({right})")),
            "ne" => Some(format!("({left}) != ({right})")),
            "lt" => Some(format!("({left}) < ({right})")),
            "le" => Some(format!("({left}) <= ({right})")),
            "gt" => Some(format!("({left}) > ({right})")),
            "ge" => Some(format!("({left}) >= ({right})")),
            "and" => Some(format!("({left}) && ({right})")),
            "or" => Some(format!("({left}) || ({right})")),
            "bitand" => Some(format!("({left}) & ({right})")),
            "bitor" => Some(format!("({left}) | ({right})")),
            "bitxor" => Some(format!("({left}) ^ ({right})")),
            "shl" => Some(format!("({left}) << ({right})")),
            "shr" => Some(format!("({left}) >> ({right})")),
            _ => None,
        };
    }
    if args.len() == 1 {
        let value = &args[0].text;
        return match op {
            "neg" => Some(format!("-({value})")),
            "not" => Some(format!("!({value})")),
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
            ShapeExpression {
                text: format!("{value}{suffix}"),
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
            let is_cross_lang = !param_sort_cids.iter().all(|c| c.is_empty())
                || !return_sort_cid.is_empty();
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
                        while param_types.len() < i { param_types.push(String::new()); }
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
    let root = operator_root()?;
    let (language, library_tag) = operator_binding_surface(&root, concept_name)?;
    if language != "rust" {
        return None;
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
    let typed_params = params
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let ty = param_types
                .get(index)
                .cloned()
                .unwrap_or_else(|| "i64".to_string());
            format!("{name}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_suffix = if return_type.is_empty() || return_type == "()" || return_type == "void" {
        String::new()
    } else {
        format!(" -> {return_type}")
    };
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
    format!("pub fn {function}({typed_params}){return_suffix} {{\n{indented}\n}}\n")
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
    match src.trim() {
        "" => "()".to_string(),
        "void" | "None" | "()" => "()".to_string(),
        "long" | "int" | "i64" | "u64" => "i64".to_string(),
        "i32" | "u32" => "i32".to_string(),
        "short" | "i16" | "u16" => "i16".to_string(),
        "byte" | "char" | "i8" | "u8" => "i8".to_string(),
        "boolean" | "bool" => "bool".to_string(),
        "double" | "float" | "f64" | "f32" => "f64".to_string(),
        "String" | "str" | "&str" | "&String" => "String".to_string(),
        "list" | "List" | "list[int]" | "list[i64]" => "&[i64]".to_string(),
        other => other.to_string(),
    }
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
fn parse_parametric_expansions(value: Option<&Value>) -> std::collections::HashMap<String, ParametricExpansion> {
    let mut map = std::collections::HashMap::new();
    let Some(arr) = value.and_then(Value::as_array) else {
        return map;
    };
    for item in arr {
        let Some(obj) = item.as_object() else { continue };
        let cid = obj.get("cid").and_then(Value::as_str).unwrap_or("").to_string();
        let ctor = obj.get("constructor_cid").and_then(Value::as_str).unwrap_or("").to_string();
        if cid.is_empty() || ctor.is_empty() { continue; }
        let arg_cids: Vec<String> = obj
            .get("arg_cids")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();
        map.insert(
            cid.clone(),
            ParametricExpansion { cid, constructor_cid: ctor, arg_cids },
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
        assert!(
            source.contains("pub fn compute_sum(a: i64, b: i64) -> i64"),
            "{source}"
        );
        assert!(source.contains("let total: i64 = (a) + (b);"), "{source}");
        assert!(
            source.contains("let scaled: i64 = (total) * (2);"),
            "{source}"
        );
        assert!(
            source.contains("let reduced: i64 = (scaled) - (1);"),
            "{source}"
        );
        assert!(source.contains("return reduced;"), "{source}");
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
        assert_eq!(
            source,
            "#[instrument]\npub fn traced(x: i64) -> i64 {\n    x\n}\n"
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
            "pub fn null() -> Arc < Value > {\n    new(Value::Null)\n}\n"
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
            "pub fn null() -> Arc < Value > {\n    Arc::new(Value::Null)\n}\n"
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
            "pub fn boolean(b: bool) -> Arc < Value > {\n    Arc::new(Value::Bool(b))\n}\n"
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
            "pub fn integer(n: i64) -> Arc < Value > {\n    Arc::new(Value::Integer(n))\n}\n"
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
            "pub fn string(s: String) -> Arc < Value > {\n    Arc::new(Value::String(s))\n}\n"
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
            "pub fn string<S: Into<String>>(s: S) -> Arc < Value > {\n    Arc::new(Value::String(s.into()))\n}\n"
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
        assert_eq!(
            response["result"]["source"],
            "pub fn toggle(flag: bool) -> bool {\n    !flag\n}\n"
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
