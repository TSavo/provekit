// SPDX-License-Identifier: Apache-2.0
//
// LLBC lift: build a FunctionContractMemento from a Charon LLBC
// function. Per #383, this is the MIR-layer companion to lift.rs
// (the surface-AST lift). Both produce the same shape of memento;
// cross-layer content_cid equality on equivalent source proves the
// substrate's predicate edges are layer-agnostic.
//
// Scope of this MVP:
//   - The if-panic pattern: a top-level `Switch::If(discr, then=[], _)`
//     immediately followed by `Abort` at top level. Charon's LLBC
//     reconstructs this from the MIR `if cond { panic!() }` pattern.
//   - Discriminant traced ONE step back: `Assign(local, BinaryOp(op,
//     Move(formal_local), Const(scalar)))`. We map MIR ordering ops
//     (Lt/Le/Gt/Ge/Eq/Ne) to the IR's predicate names and apply the
//     comparison flip on negation (¬(x<10) → x≥10), matching the
//     AST walk's `negate` helper byte-for-byte.
//   - Discriminant trail through one Use(...) hop (rustc often emits
//     `_3 := Copy(_1)` then uses `_3` — we recognize this).
//
// Out of scope (later commits on #383):
//   - Full backward-walk-through-Assigns (the general algorithm; this
//     MVP traces 1-2 hops to demonstrate the cross-layer claim).
//   - SwitchInt match arms.
//   - Conditional contributions (when else-branch isn't Return).
//   - Postcondition derivation from trailing return.
//
// The point of this MVP is NOT exhaustive coverage. The point is one
// fixture, lifted at the LLBC layer, producing byte-equal formulas to
// the AST walk's lift of the same source. The cross-layer cache hit
// is paper 07 §6 across substrate layers.

use std::collections::HashMap;

use serde_json::Value;

use provekit_ir_types::{IrFormula, IrTerm};

use crate::contract::{build_function_contract_with_file, Effect, EffectSet, FunctionContractMemento};
use crate::llbc::{LlbcError, LlbcFunction};
use crate::wp::atomic_true;

/// Lift one LLBC function to a FunctionContractMemento. The
/// `source_path` is annotated into the memento's locus for downstream
/// developer-feedback paths. `type_decls` is the raw JSON value of
/// `translated.type_decls` from the parent `LlbcCrate` — pass
/// `krate.type_decls_raw()` to enable struct field name resolution
/// for `Field(Adt, idx)` projections. Pass `None` when the crate's
/// type table is unavailable (test fixtures without types).
pub fn lift_llbc_function(
    f: LlbcFunction<'_>,
    source_path: Option<&str>,
) -> Result<FunctionContractMemento, LlbcError> {
    lift_llbc_function_with_types(f, source_path, None)
}

/// Like `lift_llbc_function` but accepts the crate's type_decls for
/// struct field name resolution. Callers that have an `LlbcCrate`
/// should pass `krate.type_decls_raw()` so `Field(Adt, idx)`
/// projections resolve to source field names (`.x`) instead of
/// indices (`.0`), achieving byte equality with the AST layer.
pub fn lift_llbc_function_with_types(
    f: LlbcFunction<'_>,
    source_path: Option<&str>,
    type_decls: Option<&Value>,
) -> Result<FunctionContractMemento, LlbcError> {
    lift_llbc_function_with_registry(
        f,
        source_path,
        type_decls,
        None,
        &crate::llbc_calls::empty_registry(),
    )
}

/// Full-power LLBC lift: `lift_llbc_function_with_types` plus
/// callsite composition. When a Call statement is encountered in the
/// body, the lifter looks up the callee in `registry`, substitutes
/// actuals for formals in the callee's pre, and pushes the result
/// into the caller's pre contributions (paper 07 §6 composition).
///
/// `fun_decls` is the raw `translated.fun_decls` JSON array used to
/// resolve FunDeclIds to callee names. Pass `None` to skip call
/// resolution; existing tests that don't need calls are unaffected.
pub fn lift_llbc_function_with_registry(
    f: LlbcFunction<'_>,
    source_path: Option<&str>,
    type_decls: Option<&Value>,
    fun_decls: Option<&Value>,
    registry: &crate::llbc_calls::ContractRegistry,
) -> Result<FunctionContractMemento, LlbcError> {
    let fn_name = f.fn_name().ok_or_else(|| LlbcError::Schema {
        path: "item_meta.name".into(),
        detail: "no Ident in name path".into(),
    })?;

    // Build the formal name table. Locals[1..=arg_count] are the
    // formals; their source names are preserved by Charon and we use
    // them verbatim so the lifted IR matches the AST walk's variable
    // names byte-for-byte.
    let arg_count = f.arg_count().unwrap_or(0);
    let formals: Vec<(u32, String)> = f
        .locals()
        .filter_map(|l| {
            let i = l.index()?;
            if i == 0 || i as usize > arg_count {
                return None;
            }
            l.name().map(|n| (i, n.to_string()))
        })
        .collect();

    // Build a named-locals map for non-formal locals that have Charon-preserved
    // source names (i.e. let bindings). When tracing through Use-hops, we stop
    // at a named local and emit Var(name) rather than continuing the trace, so
    // that `let y = x; if y < 10 { panic!() }` produces Var("y") (matching the
    // AST walk's surface-level name) rather than tracing all the way back to
    // Var("x").
    let named_locals: HashMap<u32, String> = f
        .locals()
        .filter_map(|l| {
            let i = l.index()?;
            // skip return local (0) and formals (1..=arg_count)
            if i == 0 || i as usize <= arg_count {
                return None;
            }
            l.name().map(|n| (i, n.to_string()))
        })
        .collect();

    let stmts: Vec<&Value> = f.statements().map(|s| s.raw()).collect();

    // Pre-contributions: if-panic Switch chains + MIR-inserted Asserts
    // (overflow, bounds, division-by-zero, …) + callsite compositions.
    let mut pre_contribs: Vec<IrFormula> = Vec::new();
    collect_if_panic_contributions(&[], &stmts, &formals, &named_locals, false, &mut pre_contribs);
    collect_assert_contributions(&stmts, &formals, &named_locals, &mut pre_contribs);
    let mut effects = detect_effects_llbc(&stmts, fun_decls, registry);
    // Unsafe detection: Charon records `signature.is_unsafe` on the FunDecl
    // JSON; the statement-level scan cannot see it, so we inject it here
    // where we have the full LlbcFunction.
    if f.is_unsafe() {
        effects.add(Effect::Unsafe);
    }
    // Loop detection: every Loop in the body becomes Effect::OpaqueLoop
    // keyed by its content-addressed body hash. Per paper 07 §11, loops
    // are deferred proof obligations — the substrate refuses to compose
    // a contract carrying OpaqueLoop until a LoopInvariantMemento keyed
    // by the loop_cid is supplied. Honest opacity, not silent assumption.
    for lp in crate::llbc_loops::extract_loops(&stmts) {
        effects.add(Effect::OpaqueLoop {
            loop_cid: lp.loop_cid,
        });
    }
    // Try-branch (`?` operator) detection: each Switch::Match-after-
    // Try::branch shape becomes Effect::EarlyReturn. Same opacity model
    // as loops; substrate refuses composition until a TryBranchMemento
    // supplies the success/failure-path contract pair.
    for tr in crate::llbc_try::extract_try_branches(&stmts, fun_decls) {
        effects.add(Effect::EarlyReturn {
            try_cid: tr.try_cid,
        });
    }
    // Closure-capture detection: each Aggregate(Adt) where the type is
    // a synthetic closure type (path ends in Ident("closure")) becomes
    // Effect::ClosureCapture. The closure body is a separate fun_decl
    // (lifted normally); this effect records the link to the body and
    // the count of captured operands.
    if let Some(td) = type_decls {
        for cap in
            crate::llbc_closures::extract_closure_captures(&stmts, Some(td), fun_decls)
        {
            effects.add(Effect::ClosureCapture {
                body_fn_cid: cap.body_fn_cid,
                n_captures: cap.n_captures,
            });
        }
    }
    if let Some(fd) = fun_decls {
        collect_call_contributions(&stmts, &formals, &named_locals, fd, registry, &mut pre_contribs, &mut effects);
    }
    let pre_formula = simplify_conjunction(pre_contribs.clone());

    // Postcondition: every pre-contribution still holds at the
    // function's return point (matches AST walk behavior). Plus a
    // return-value derivation traced through MIR's `_0 := ...` chain
    // — including the tuple-projection-on-CheckedOp pattern rustc
    // emits for arithmetic. This makes the LLBC walk's post atoms
    // align byte-for-byte with the AST walk's `result = <expr>`
    // derivation, collapsing LayerAgreement::Both into LlbcExtra
    // when MIR is the only layer that contributes.
    let mut post_contribs = pre_contribs;
    if let Some(result_eq) =
        derive_return_equation(&stmts, &formals, type_decls, &named_locals, fun_decls, registry)
    {
        post_contribs.push(result_eq);
    }
    let post_formula = simplify_conjunction(post_contribs);

    // We synthesize a syn::ItemFn shell to reuse build_function_contract's
    // memento-construction machinery — it sets up the locus, sorts,
    // canonical bytes and CID, but uses our LLBC-derived pre/post
    // formulas. The shell carries only the function name and formal
    // sorts; its body is empty (the predicates come from LLBC, not
    // from the surface AST).
    let item_fn = synth_item_fn(&fn_name, &formals);
    let mut contract = build_function_contract_with_file(&item_fn, None, source_path);
    // Set LLBC-derived effects on the contract before override_formulas so
    // build_memento_value sees the correct effect set when recomputing the CID.
    contract.effects = effects;
    // Override the lifted formulas with the LLBC-derived ones, then
    // recompute canonical bytes + CID so result_var_name and the
    // header.cid path are consistent.
    contract = override_formulas(contract, pre_formula, post_formula);
    Ok(contract)
}

/// Trace MIR's `_0 := <rvalue>` chain back to a source-equivalent
/// IrTerm and emit `result = <term>` as a postcondition atom. Returns
/// None when the return is unit (Aggregate of empty Tuple), or when
/// the trace can't be resolved to a recognized shape.
///
/// # Call-dest derivation (Task B, #383)
///
/// When `_0` is set by a Call statement's `dest` (not by an Assign rvalue),
/// the standard Assign scan misses the return-value binding. This function
/// also scans for Call statements whose `dest.kind.Local == 0` and lifts
/// them to `result = Ctor("call:<callee_name>", [arg_terms...])`. This is a
/// symbolic representation of the call's return value; downstream consumers
/// can pattern-match `call:<name>` to compose with the callee's post.
///
/// The AST walk does not handle `Expr::Call` (free function calls) in
/// `lift_expr_to_term_inner`, so this derivation is LLBC-first (LlbcExtra).
fn derive_return_equation(
    stmts: &[&Value],
    formals: &[(u32, String)],
    type_decls: Option<&Value>,
    named_locals: &HashMap<u32, String>,
    fun_decls: Option<&Value>,
    registry: &crate::llbc_calls::ContractRegistry,
) -> Option<IrFormula> {
    // Scan in reverse — both Assign and Call-dest paths.
    for (i, s) in stmts.iter().enumerate().rev() {
        // --- Assign path: _0 := <rvalue> ---
        if let Some(arr) = stmt_kind_payload(s, "Assign").and_then(|v| v.as_array()) {
            if arr.len() == 2 {
                if let Some(local) = place_to_local_id(&arr[0]) {
                    if local == 0 {
                        let prior = &stmts[..i];
                        if let Some(term) = rvalue_to_ir_term_for_post(
                            &arr[1],
                            prior,
                            formals,
                            type_decls,
                            named_locals,
                        ) {
                            return Some(IrFormula::Atomic {
                                name: "=".to_string(),
                                args: vec![IrTerm::Var { name: "result".to_string() }, term],
                            });
                        }
                        return None;
                    }
                }
            }
            continue;
        }

        // --- Call-dest path: Call(callee, args, dest: _0) ---
        // When the return value is set by a Call rather than an Assign,
        // derive `result = Ctor("call:<callee_name>", [arg_terms...])`.
        if let Some(dest_local) = crate::llbc_calls::call_dest_local(s) {
            if dest_local == 0 {
                if let Some(fd) = fun_decls {
                    if let Some((func_id, args)) = crate::llbc_calls::extract_call_target(s) {
                        if let Some(callee_name) =
                            crate::llbc_calls::fundecl_name_by_id(fd, func_id)
                        {
                            let prior = &stmts[..i];
                            let mut arg_terms: Vec<IrTerm> = Vec::with_capacity(args.len());
                            let mut all_lifted = true;
                            for op in args.iter() {
                                match operand_to_ir_term(op, prior, formals, named_locals) {
                                    Some(t) => arg_terms.push(t),
                                    None => {
                                        all_lifted = false;
                                        break;
                                    }
                                }
                            }
                            if all_lifted {
                                // `registry` reserved for future post-composition
                                let _ = registry;
                                return Some(IrFormula::Atomic {
                                    name: "=".to_string(),
                                    args: vec![
                                        IrTerm::Var { name: "result".to_string() },
                                        IrTerm::Ctor {
                                            name: format!("call:{}", callee_name),
                                            args: arg_terms,
                                        },
                                    ],
                                });
                            }
                        }
                    }
                }
                // Call-dest to _0 but couldn't resolve: stop.
                return None;
            }
        }
    }
    None
}

/// Lift an Rvalue to an IrTerm for postcondition derivation. Handles:
///   - `Use(Move/Copy(place))`: traces the place via
///     `place_to_term_for_post`.
///   - `BinaryOp(op, lhs, rhs)`: lifts to `Ctor(<op>, [...])` for
///     arithmetic ops (Add/Sub/Mul/Div/Rem, bitwise ops, and their
///     Checked variants). The BinaryOp tag may be a bare string
///     ("Add", "BitAnd") or an object form ({"Div": "UB"}) depending
///     on how Charon encodes the overflow mode. Both forms are handled.
///     Comparison ops are predicates, not terms; not handled here.
///   - `UnaryOp([Cast{...}, operand])`: transparent cast — the inner
///     operand is lifted directly. Matches AST's `Expr::Cast` arm
///     (lift_expr_to_term_inner unwraps to the inner expression),
///     so `x as u32` produces `Var("x")` at the IR layer.
fn rvalue_to_ir_term_for_post(
    rvalue: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
    type_decls: Option<&Value>,
    named_locals: &HashMap<u32, String>,
) -> Option<IrTerm> {
    if let Some(use_op) = rvalue.get("Use") {
        if let Some(place) = use_op.get("Move").or_else(|| use_op.get("Copy")) {
            return place_to_term_for_post(place, prior, formals, type_decls, named_locals);
        }
        if let Some(constant) = use_op.get("Const") {
            return constant_to_ir_term(constant);
        }
        return None;
    }
    if let Some(arr) = rvalue.get("BinaryOp").and_then(|v| v.as_array()) {
        if arr.len() != 3 {
            return None;
        }
        let op = mir_arith_op_tag(&arr[0])?;
        let ir_op = mir_arith_op_to_ir_ctor(op)?;
        let l = operand_to_ir_term(&arr[1], prior, formals, named_locals)?;
        let r = operand_to_ir_term(&arr[2], prior, formals, named_locals)?;
        return Some(IrTerm::Ctor {
            name: ir_op.to_string(),
            args: vec![l, r],
        });
    }
    // UnaryOp([op_descriptor, operand]): handle Cast transparently.
    // Charon encodes `x as T` as `UnaryOp([{"Cast": ...}, operand])`.
    if let Some(arr) = rvalue.get("UnaryOp").and_then(|v| v.as_array()) {
        if arr.len() == 2 {
            if arr[0].get("Cast").is_some() {
                return operand_to_ir_term(&arr[1], prior, formals, named_locals);
            }
        }
    }
    None
}

/// Lift a Place to an IrTerm for postcondition return-value derivation.
/// Handles:
///   - `Local(N)` where _N is a formal: `Var(<formal name>)`.
///   - `Local(N)` where _N is a temp: traces _N's defining rvalue
///     via `rvalue_to_ir_term_for_post`.
///   - `Projection(base, "Deref")`: transparent; recurses on the
///     base. Required for `&Point` references and for slice indexing
///     through a reference (Charon emits `Deref` before `Index`).
///   - `Projection(base, {"Index": {"offset": op, ...}})`: lifts to
///     `Ctor("index", [base_term, offset_term])` matching AST's
///     `s[i]` encoding byte-for-byte.
///   - `Projection(base, Field(Tuple, 0))` where base is `Local(N)`
///     and _N := `BinaryOp(<CheckedOp>, lhs, rhs)`: lifts to the
///     bare arithmetic ctor (the rustc CheckedMul + tuple-projection
///     pattern for overflow-checked arithmetic).
///   - `Projection(base, Field(Tuple, idx))` in all other cases:
///     emits `Ctor("field", [base_term, Var(".N")])`. Matches AST's
///     `Expr::Field(syn::Member::Unnamed(idx))`.
///   - `Projection(base, Field(Adt(adt_id, variant_id), idx))`: resolves
///     the field's source name from type_decls. When variant_id is null
///     (struct), looks up `kind.Struct[idx].name`. When variant_id is N
///     (enum variant), looks up `kind.Enum[N].fields[idx].name`. Unnamed
///     fields (tuple variants) have null name and fall back to index
///     notation. Emits `Ctor("field", [base_term, Var(".<name>")])`.
///     Falls back to index notation if type_decls is absent or lookup
///     fails.
fn place_to_term_for_post(
    place: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
    type_decls: Option<&Value>,
    named_locals: &HashMap<u32, String>,
) -> Option<IrTerm> {
    let kind = place.get("kind")?;
    if let Some(local) = kind.get("Local").and_then(|v| v.as_u64()) {
        let local = local as u32;
        if let Some((_, name)) = formals.iter().find(|(id, _)| *id == local) {
            return Some(IrTerm::Var { name: name.clone() });
        }
        let rvalue = find_last_assign_rvalue(prior, local)?;
        return rvalue_to_ir_term_for_post(rvalue, prior, formals, type_decls, named_locals);
    }
    if let Some(proj_arr) = kind.get("Projection").and_then(|v| v.as_array()) {
        if proj_arr.len() != 2 {
            return None;
        }
        let base = &proj_arr[0];
        let elem = &proj_arr[1];

        // Deref projection: transparent for predicate purposes. Used
        // before `Field(Adt, _)` on `&Point` references and before
        // `Index { offset }` on slice indexing through `&[T]`.
        if elem.as_str() == Some("Deref") {
            return place_to_term_for_post(base, prior, formals, type_decls, named_locals);
        }

        // Index projection: `base[offset]` — Charon emits
        //   Projection([Projection([Local(s), Deref]), Index{offset: Copy/Move(idx)}])
        // Lift to Ctor("index", [base_term, idx_term]) matching AST's
        // Expr::Index → `Ctor("index", [arr, idx])` byte-for-byte.
        if let Some(idx_obj) = elem.get("Index") {
            let base_term = place_to_term_for_post(base, prior, formals, type_decls, named_locals)?;
            let offset_op = idx_obj.get("offset")?;
            let idx_term = operand_to_ir_term(offset_op, prior, formals, named_locals)?;
            return Some(IrTerm::Ctor {
                name: "index".to_string(),
                args: vec![base_term, idx_term],
            });
        }

        if let Some(field_arr) = elem.get("Field").and_then(|v| v.as_array()) {
            if field_arr.len() == 2 {
                let field_kind = &field_arr[0];
                let field_idx = field_arr[1].as_u64()? as usize;

                // Tuple field projection.
                if field_kind.get("Tuple").is_some() {
                    // CheckedOp shortcut: Field(Tuple, 0) on a local whose
                    // defining rvalue is a BinaryOp with an arithmetic op.
                    // The rustc pattern for checked arithmetic — the result
                    // is a (value, overflow_flag) tuple; we skip the tuple
                    // wrapper and emit the bare arithmetic ctor. Uses
                    // `mir_arith_op_tag` so both bare-string and object-form
                    // BinaryOp encodings (`"Add"` vs `{"Div": "UB"}`) work.
                    if field_idx == 0 {
                        if let Some(base_local) = base
                            .get("kind")
                            .and_then(|k| k.get("Local"))
                            .and_then(|v| v.as_u64())
                        {
                            let base_local = base_local as u32;
                            if let Some(base_rv) = find_last_assign_rvalue(prior, base_local) {
                                if let Some(arr) =
                                    base_rv.get("BinaryOp").and_then(|v| v.as_array())
                                {
                                    if arr.len() == 3 {
                                        if let Some(op) = mir_arith_op_tag(&arr[0]) {
                                            if let Some(ir_op) = mir_arith_op_to_ir_ctor(op) {
                                                let l = operand_to_ir_term(
                                                    &arr[1],
                                                    prior,
                                                    formals,
                                                    named_locals,
                                                )?;
                                                let r = operand_to_ir_term(
                                                    &arr[2],
                                                    prior,
                                                    formals,
                                                    named_locals,
                                                )?;
                                                return Some(IrTerm::Ctor {
                                                    name: ir_op.to_string(),
                                                    args: vec![l, r],
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // General tuple element access (non-CheckedOp base, or
                    // idx > 0). Emit Ctor("field", [base_term, Var(".N")]).
                    let base_term =
                        place_to_term_for_post(base, prior, formals, type_decls, named_locals)?;
                    return Some(IrTerm::Ctor {
                        name: "field".to_string(),
                        args: vec![
                            base_term,
                            IrTerm::Var { name: format!(".{}", field_idx) },
                        ],
                    });
                }

                // Adt (struct or enum variant) field projection. Look up
                // the field's source name from the crate's type_decls so
                // the emitted term uses `.x` rather than `.0`, matching
                // AST byte-for-byte for struct fields.
                //
                // Charon encodes the Adt field kind as a two-element JSON
                // array: [adt_id, variant_id]. For structs the adt_id is a
                // plain integer and variant_id is null (or absent). For
                // enum variant projections (after a Switch::Match arm),
                // variant_id is the variant index as an integer.
                //
                // Layer-divergence note: enum-variant field projection in
                // LLBC only appears inside a Switch::Match arm, which the
                // current lifter does not walk for postcondition derivation
                // (match-arm traversal is out of scope for this MVP). So
                // this branch fires for enum fields that appear in simpler
                // reference contexts. Cross-layer byte equality does NOT
                // hold for enum match patterns because AST does not lift
                // match arms at all; this is an LlbcExtra site per paper
                // 07's layered-agreement taxonomy.
                if let Some(adt_arr) = field_kind.get("Adt").and_then(|v| v.as_array()) {
                    let adt_id = adt_arr.first().and_then(|v| v.as_u64())? as usize;
                    // adt_arr[1] is variant_id: Some(N) for enum variants,
                    // null or absent for structs.
                    let variant_id =
                        adt_arr.get(1).and_then(|v| v.as_u64()).map(|n| n as usize);
                    let field_name = type_decls
                        .and_then(|td| adt_field_name(td, adt_id, variant_id, field_idx))
                        .unwrap_or_else(|| field_idx.to_string());
                    let base_term =
                        place_to_term_for_post(base, prior, formals, type_decls, named_locals)?;
                    return Some(IrTerm::Ctor {
                        name: "field".to_string(),
                        args: vec![
                            base_term,
                            IrTerm::Var { name: format!(".{}", field_name) },
                        ],
                    });
                }
            }
        }
    }
    None
}

/// Look up a struct field's source name from the crate's type_decls.
/// `type_decls` is the raw JSON value of `translated.type_decls` (an
/// array). The Adt id is an index into this array matching `def_id`;
/// fields are in `kind.Struct` in declaration order.
///
/// Returns `None` when:
///   - `type_decls` is not an array (absent or wrong type)
///   - no type_decl has `def_id == adt_id`
///   - the type is not a struct (enum, tuple struct, etc.)
///   - `field_idx` is out of range
fn adt_field_name(
    type_decls: &Value,
    adt_id: usize,
    variant_id: Option<usize>,
    field_idx: usize,
) -> Option<String> {
    let decls = type_decls.as_array()?;
    let decl = decls.iter().find(|d| {
        d.get("def_id").and_then(|v| v.as_u64()).map(|id| id as usize) == Some(adt_id)
    })?;
    let kind = decl.get("kind")?;
    let fields = if let Some(vid) = variant_id {
        let variants = kind.get("Enum")?.as_array()?;
        let variant = variants.get(vid)?;
        variant.get("fields")?.as_array()?
    } else {
        kind.get("Struct")?.as_array()?
    };
    let field = fields.get(field_idx)?;
    // Unnamed fields (tuple variants / tuple structs) have `"name": null`.
    // Return None so the caller emits the numeric index fallback.
    field.get("name")?.as_str().map(|s| s.to_string())
}

/// Trace an Assert's `cond` operand back to a `BinaryOp(Eq, x, 0)`
/// definition and return `x` as an IrTerm. Used by DivisionByZero
/// and RemainderByZero handlers because Charon's check_kind operand
/// for those variants is the dividend, not the divisor — the actual
/// runtime check is in the cond.
fn divisor_from_assert_cond(
    cond: Option<&Value>,
    prior: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
) -> Option<IrTerm> {
    let cond = cond?;
    let cond_local = operand_to_local_id(cond)?;
    let cond_rvalue = find_last_assign_rvalue(prior, cond_local)?;
    let arr = cond_rvalue.get("BinaryOp").and_then(|v| v.as_array())?;
    if arr.len() != 3 {
        return None;
    }
    if arr[0].as_str() != Some("Eq") {
        return None;
    }
    let lhs = &arr[1];
    let rhs = &arr[2];
    let divisor_op = if is_zero_constant(rhs) {
        lhs
    } else if is_zero_constant(lhs) {
        rhs
    } else {
        return None;
    };
    operand_to_ir_term(divisor_op, prior, formals, named_locals)
}

fn is_zero_constant(operand: &Value) -> bool {
    let Some(constant) = operand.get("Const") else {
        return false;
    };
    let Some(scalar) = constant
        .get("kind")
        .and_then(|k| k.get("Literal"))
        .and_then(|l| l.get("Scalar"))
    else {
        return false;
    };
    for variant in ["Unsigned", "Signed"] {
        if let Some(arr) = scalar.get(variant).and_then(|v| v.as_array()) {
            if let Some(s) = arr.get(1).and_then(|v| v.as_str()) {
                return s == "0";
            }
        }
    }
    false
}

/// Extract the operator name from a BinaryOp tag that may be either a
/// bare string ("Add", "Div", "BitAnd", …) or an object with overflow
/// mode ({"Div": "UB"}, {"Mul": "Wrap"}, …). Charon encodes Div/Rem as
/// objects in LLBC when UB semantics apply, but bare strings for
/// checked variants and bitwise ops.
fn mir_arith_op_tag(v: &Value) -> Option<&str> {
    if let Some(s) = v.as_str() {
        return Some(s);
    }
    // Object form: first key is the op name, value is the overflow mode.
    v.as_object()?.keys().next().map(|k| k.as_str())
}

/// Map MIR arithmetic-op tag to IR ctor name. Handles:
///   - Bare arithmetic ops (Add/Sub/Mul/Div/Rem) and their Checked
///     variants (which produce a (value, overflow_bool) tuple in MIR).
///     Both forms lift to the same ctor at the IR layer; the no-overflow
///     precondition that `collect_assert_contributions` emits is what
///     distinguishes "checked arithmetic" semantics in the substrate.
///   - Bitwise ops (BitAnd/BitOr/BitXor/Shl/Shr) — Charon encodes
///     these as bare string tags. They map to the same ctor names the
///     AST walk uses (`&`, `|`, `^`, `<<`, `>>`), ensuring byte-
///     identical IR when both layers lift the same bitwise expression.
fn mir_arith_op_to_ir_ctor(op: &str) -> Option<&'static str> {
    match op {
        "Add" | "AddChecked" | "AddWithOverflow" => Some("+"),
        "Sub" | "SubChecked" | "SubWithOverflow" => Some("-"),
        "Mul" | "MulChecked" | "MulWithOverflow" => Some("*"),
        "Div" => Some("/"),
        "Rem" => Some("%"),
        "BitAnd" => Some("&"),
        "BitOr" => Some("|"),
        "BitXor" => Some("^"),
        "Shl" | "ShlUnchecked" => Some("<<"),
        "Shr" | "ShrUnchecked" => Some(">>"),
        _ => None,
    }
}

/// Detect effects from the LLBC body statements. Scans for:
///   - `Panics`: any `Abort` statement directly in the body, or a
///     Switch::If whose then- or else-branch leads to Abort (the
///     canonical panic pattern emitted by Charon for `if cond { panic!() }`).
///   - `Io`: any Call to a function whose resolved name contains I/O
///     substrings (print, io, fmt, display).
///
/// `UnresolvedCall` effects are populated separately by
/// `collect_call_contributions`, which has registry access.
pub fn detect_effects_llbc(
    stmts: &[&Value],
    fun_decls: Option<&Value>,
    registry: &crate::llbc_calls::ContractRegistry,
) -> EffectSet {
    let _ = registry; // Io detection uses callee name strings, not registry lookup
    let mut set = EffectSet::empty();

    for s in stmts.iter() {
        // Panics: bare Abort statement.
        if stmt_kind_tag(s) == Some("Abort") {
            set.add(Effect::Panics);
            continue;
        }

        // Panics: Switch::If whose then- or else-branch leads to Abort.
        if let Some(switch) = stmt_kind_payload(s, "Switch") {
            if let Some(if_arr) = switch.get("If").and_then(|v| v.as_array()) {
                if if_arr.len() == 3 {
                    let then_block = &if_arr[1];
                    let else_block = &if_arr[2];
                    if block_leads_to_abort(then_block, false)
                        || block_leads_to_abort(else_block, false)
                    {
                        set.add(Effect::Panics);
                    }
                }
            }
        }

        // Io: Call to a function whose resolved name contains I/O substrings.
        if let Some((func_id, _args)) = crate::llbc_calls::extract_call_target(s) {
            if let Some(fd) = fun_decls {
                if let Some(callee_name) = crate::llbc_calls::fundecl_name_by_id(fd, func_id) {
                    if is_io_callee_name(&callee_name) {
                        set.add(Effect::Io);
                    }
                }
            }
        }
    }
    set
}

/// Return true when a callee name indicates I/O. Matches against
/// well-known substrings in std::io / fmt path components.
fn is_io_callee_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(lower.as_str(), "print" | "println" | "eprint" | "eprintln" | "write_fmt" | "fmt")
        || lower.contains("io")
        || lower.contains("print")
        || lower.contains("display")
}

/// Collect predicate atoms from `Call` statements: substitute the
/// callee's pre with actuals supplied at the callsite, push the
/// result into the caller's pre contributions. This is paper 07
/// §6's "compose for free" — every callsite composes the callee's
/// content-addressed precondition into the caller's contract.
///
/// When a callee's name can be resolved from fun_decls but is NOT in the
/// registry, emit `Effect::UnresolvedCall { name }` into `effects`. This
/// advertises that the caller's contract depends on a function whose
/// effects are unknown; substrate must refuse composition until resolved.
fn collect_call_contributions(
    stmts: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
    fun_decls: &Value,
    registry: &crate::llbc_calls::ContractRegistry,
    out: &mut Vec<IrFormula>,
    effects: &mut EffectSet,
) {
    for (i, s) in stmts.iter().enumerate() {
        let Some((func_id, args)) = crate::llbc_calls::extract_call_target(s) else {
            continue;
        };
        let Some(callee_name) = crate::llbc_calls::fundecl_name_by_id(fun_decls, func_id) else {
            continue;
        };
        let Some(callee) = registry.get(&callee_name) else {
            // Task B: callee name resolved but not in registry -> UnresolvedCall.
            effects.add(Effect::UnresolvedCall { name: callee_name });
            continue;
        };
        // Lift each actual argument. The trace runs back through prior
        // statements (everything before this Call).
        let prior = &stmts[..i];
        let mut arg_terms: Vec<IrTerm> = Vec::with_capacity(args.len());
        let mut all_lifted = true;
        for op in args.iter() {
            match operand_to_ir_term(op, prior, formals, named_locals) {
                Some(t) => arg_terms.push(t),
                None => {
                    all_lifted = false;
                    break;
                }
            }
        }
        if !all_lifted {
            continue;
        }
        if arg_terms.len() != callee.formals.len() {
            // Arity mismatch — likely a generic / trait method we're
            // resolving incorrectly. Skip rather than emit garbage.
            continue;
        }
        let composed = crate::llbc_calls::compose_callsite_pre(callee, &arg_terms);
        out.push(composed);
    }
}

/// Collect predicate atoms from `Assert` statements in the body. This
/// catches MIR-inserted overflow / panic-on-failure asserts that the
/// surface AST doesn't see — paper 07's "MIR sees more" lane.
fn collect_assert_contributions(
    stmts: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
    out: &mut Vec<IrFormula>,
) {
    for (i, s) in stmts.iter().enumerate() {
        let Some(assert_payload) = stmt_kind_payload(s, "Assert") else {
            continue;
        };
        let Some(assert_obj) = assert_payload.get("assert") else {
            continue;
        };
        let _expected = assert_obj.get("expected").and_then(|v| v.as_bool());
        let Some(check_kind) = assert_obj.get("check_kind") else {
            continue;
        };
        // Recognized check_kinds (Charon's BuiltinAssertKind enum):
        //   Overflow(BinOp, lhs, rhs)
        //   OverflowNeg(operand)
        //   BoundsCheck { len, index }
        //   DivisionByZero(divisor)
        //   RemainderByZero(divisor)
        //
        // Other variants (MisalignedPointerDereference, NullPointerDereference,
        // InvalidEnumConstruction) are out of scope for the MVP — their
        // semantic predicates require type-system reasoning we don't yet
        // do at the LLBC layer.
        let prior = &stmts[..i];

        // Overflow: [{ "Mul": "Wrap" } | { "Add": ... } | ..., lhs, rhs]
        if let Some(arr) = check_kind.get("Overflow").and_then(|v| v.as_array()) {
            if arr.len() < 3 {
                continue;
            }
            let op_descriptor = &arr[0];
            let Some(op_tag) = overflow_op_tag(op_descriptor) else {
                continue;
            };
            let Some(lhs_term) = operand_to_ir_term(&arr[1], prior, formals, named_locals) else {
                continue;
            };
            let Some(rhs_term) = operand_to_ir_term(&arr[2], prior, formals, named_locals) else {
                continue;
            };
            out.push(IrFormula::Atomic {
                name: format!("no-overflow:{}", op_tag),
                args: vec![lhs_term, rhs_term],
            });
            continue;
        }

        // OverflowNeg(operand) -> Atomic("no-overflow:neg", [term]).
        if let Some(operand) = check_kind.get("OverflowNeg") {
            let Some(t) = operand_to_ir_term(operand, prior, formals, named_locals) else {
                continue;
            };
            out.push(IrFormula::Atomic {
                name: "no-overflow:neg".to_string(),
                args: vec![t],
            });
            continue;
        }

        // BoundsCheck { len, index } -> Atomic("<", [index, len]).
        // The implicit predicate is `index < len`. Charon's Assert
        // expected=true here means "the bounds-check passed."
        if let Some(obj) = check_kind.get("BoundsCheck").and_then(|v| v.as_object()) {
            let Some(len_op) = obj.get("len") else {
                continue;
            };
            let Some(index_op) = obj.get("index") else {
                continue;
            };
            let Some(len_term) = operand_to_ir_term(len_op, prior, formals, named_locals) else {
                continue;
            };
            let Some(index_term) = operand_to_ir_term(index_op, prior, formals, named_locals) else {
                continue;
            };
            out.push(IrFormula::Atomic {
                name: "<".to_string(),
                args: vec![index_term, len_term],
            });
            continue;
        }

        // DivisionByZero / RemainderByZero. Charon's check_kind
        // operand is informational (it's the dividend, not the
        // divisor — likely passed in for error reporting). The
        // actual runtime check is in the assert's `cond`: a Bool
        // local whose definition is `BinaryOp(Eq, divisor, 0)`. We
        // trace the cond, extract the non-zero operand, and emit
        // `Atomic("≠", [divisor, 0])`.
        if check_kind.get("DivisionByZero").is_some()
            || check_kind.get("RemainderByZero").is_some()
        {
            if let Some(divisor_term) =
                divisor_from_assert_cond(assert_obj.get("cond"), prior, formals, named_locals)
            {
                out.push(IrFormula::Atomic {
                    name: "≠".to_string(),
                    args: vec![divisor_term, crate::wp::const_int(0)],
                });
            }
            continue;
        }
    }
}

/// Map Charon's overflow op descriptor to a flat tag for the IR's
/// predicate name. The descriptor shape is e.g. `{ "Mul": "Wrap" }`
/// (Mul with wrap-on-overflow arithmetic). We flatten to
/// `mul-wrap` / `add-wrap` / `sub-wrap` etc. so consumers can
/// pattern-match on the predicate-name suffix.
fn overflow_op_tag(descriptor: &Value) -> Option<String> {
    let obj = descriptor.as_object()?;
    let (op_name, mode_val) = obj.iter().next()?;
    let mode = mode_val.as_str().unwrap_or("unknown");
    Some(format!("{}-{}", op_name.to_lowercase(), mode.to_lowercase()))
}

/// Recursive collector. `prior` is the chain of statements the
/// discriminant traces are allowed to reach back through (the
/// concatenation of every enclosing block's stmts up to the current
/// Switch). `stmts` is the current block being scanned.
/// `parent_falls_through_to_abort` is true when reaching the end of
/// the current block (without a Return/Abort inside it) leads to an
/// Abort in the enclosing scope.
///
/// Handles two Switch variants:
///   - `Switch::If`: the binary if-panic pattern (existing).
///   - `Switch::SwitchInt`: multi-arm `match` where arms leading to
///     Abort contribute `Atomic("≠", [discr, lit])` per literal.
fn collect_if_panic_contributions(
    prior: &[&Value],
    stmts: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
    parent_falls_through_to_abort: bool,
    out: &mut Vec<IrFormula>,
) {
    for (i, s) in stmts.iter().enumerate() {
        let Some(switch) = stmt_kind_payload(s, "Switch") else {
            continue;
        };

        // Where does the path AFTER this Switch end up? If the next
        // non-bookkeeping statement is Abort, the fall-through aborts.
        // If we reach the end of `stmts` without hitting Abort or a
        // Return-equivalent, defer to the parent.
        let post_switch_aborts =
            tail_aborts_or_inherit(&stmts[i + 1..], parent_falls_through_to_abort);

        // Build the local prior for discriminant tracing: parent prior plus
        // pre-switch statements in the current block.
        let mut local_prior: Vec<&Value> = Vec::with_capacity(prior.len() + i);
        local_prior.extend(prior.iter().copied());
        local_prior.extend(stmts[..i].iter().copied());

        // --- Switch::If (binary if-panic) ---
        if let Some(if_arr) = switch.get("If").and_then(|v| v.as_array()) {
            if if_arr.len() == 3 {
                let discr = &if_arr[0];
                let then_block = &if_arr[1];
                let else_block = &if_arr[2];

                let then_aborts = block_leads_to_abort(then_block, post_switch_aborts);
                if then_aborts {
                    if let Some(pred) =
                        discriminant_to_formula(discr, &local_prior, formals, named_locals)
                    {
                        out.push(negate_predicate(pred));
                    }
                    // Recurse into the else-block: short-circuited `||` / `&&`
                    // emits another Switch nested in the else side.
                    let mut new_prior = local_prior.clone();
                    new_prior.push(s);
                    let inner_stmts: Vec<&Value> = block_statements(else_block);
                    collect_if_panic_contributions(
                        &new_prior,
                        &inner_stmts,
                        formals,
                        named_locals,
                        post_switch_aborts,
                        out,
                    );
                }
                // If then doesn't abort: not an if-panic. Skip.
            }
            continue;
        }

        // --- Switch::SwitchInt (multi-arm match) ---
        // JSON shape: {"SwitchInt": [discr, lit_ty, [[[scalar,...], block], ...], otherwise_block]}
        // Arm pattern literals are bare {"Scalar": {"Unsigned": ["U32","0"]}} objects;
        // NOT wrapped in the `kind.Literal.Scalar` envelope that `constant_to_ir_term`
        // expects. `arm_scalar_to_ir_term` handles this shape.
        if let Some(si_arr) = switch.get("SwitchInt").and_then(|v| v.as_array()) {
            if si_arr.len() != 4 {
                continue;
            }
            let discr = &si_arr[0];
            let arms = match si_arr[2].as_array() {
                Some(a) => a,
                None => continue,
            };
            let otherwise_block = &si_arr[3];

            // If the otherwise-block aborts, the precondition is a disjunction
            // of matched literals (an `Or`, not a conjunction of `!=` atoms).
            // Out of scope for MVP. Skip.
            if block_leads_to_abort(otherwise_block, post_switch_aborts) {
                continue;
            }

            // Lift the discriminant to an IR term (e.g. Var("x") for a formal).
            let Some(discr_term) =
                operand_to_ir_term(discr, &local_prior, formals, named_locals)
            else {
                continue;
            };

            // For each arm whose block leads to Abort, emit Atomic("!=", [discr, lit])
            // for every literal in that arm's pattern. Turns `0 | 1 => panic!()`
            // into the conjunction `x != 0 /\ x != 1`.
            for arm in arms {
                let arm_arr = match arm.as_array() {
                    Some(a) => a,
                    None => continue,
                };
                if arm_arr.len() != 2 {
                    continue;
                }
                let patterns = match arm_arr[0].as_array() {
                    Some(p) => p,
                    None => continue,
                };
                let arm_block = &arm_arr[1];
                if !block_leads_to_abort(arm_block, post_switch_aborts) {
                    continue;
                }
                for scalar_val in patterns {
                    if let Some(lit_term) = arm_scalar_to_ir_term(scalar_val) {
                        out.push(IrFormula::Atomic {
                            name: "≠".to_string(),
                            args: vec![discr_term.clone(), lit_term],
                        });
                    }
                }
            }
            continue;
        }
    }
}

/// True if the block's statements contain an Abort directly, OR all
/// statements are bookkeeping and falling through the block reaches
/// an Abort (per the parent-aware tail propagation).
fn block_leads_to_abort(block: &Value, parent_falls_through_to_abort: bool) -> bool {
    let stmts = block_statements(block);
    if stmts.iter().any(|s| stmt_kind_tag(s) == Some("Abort")) {
        return true;
    }
    // All bookkeeping → falling through this block reaches the parent's
    // post-switch position.
    if stmts.iter().all(|s| is_bookkeeping(s)) {
        return parent_falls_through_to_abort;
    }
    false
}

/// True if the tail (statements after the current Switch) leads to an
/// Abort, considering parent fall-through. The first non-bookkeeping
/// statement in the tail decides:
///   - Abort: tail aborts
///   - anything else (Return, Assign, another Switch, ...): doesn't abort
///   - empty (only bookkeeping or no remaining statements): defer to parent
fn tail_aborts_or_inherit(tail: &[&Value], parent_falls_through_to_abort: bool) -> bool {
    match tail.iter().find(|s| !is_bookkeeping(s)) {
        Some(s) => stmt_kind_tag(s) == Some("Abort"),
        None => parent_falls_through_to_abort,
    }
}

fn is_bookkeeping(stmt: &&Value) -> bool {
    matches!(
        stmt_kind_tag(stmt),
        Some("StorageDead") | Some("StorageLive") | Some("Nop")
    )
}

/// `_local := BinaryOp(op, lhs, rhs)` → `Atomic(ir_op, [lhs_term, rhs_term])`.
/// Walks one Use-hop if the discriminant comes via `_local := Use(_other)`.
fn discriminant_to_formula(
    operand: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
) -> Option<IrFormula> {
    let local = operand_to_local_id(operand)?;
    let rvalue = find_last_assign_rvalue(prior, local)?;

    if let Some(arr) = rvalue.get("BinaryOp").and_then(|v| v.as_array()) {
        if arr.len() != 3 {
            return None;
        }
        let mir_op = arr[0].as_str()?;
        let pred_name = mir_binop_to_ir_predicate(mir_op)?;
        let lhs = operand_to_ir_term(&arr[1], prior, formals, named_locals)?;
        let rhs = operand_to_ir_term(&arr[2], prior, formals, named_locals)?;
        return Some(IrFormula::Atomic {
            name: pred_name.to_string(),
            args: vec![lhs, rhs],
        });
    }

    if let Some(use_op) = rvalue.get("Use") {
        return discriminant_to_formula(use_op, prior, formals, named_locals);
    }

    None
}

fn operand_to_ir_term(
    operand: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
) -> Option<IrTerm> {
    if let Some(place) = operand.get("Move").or_else(|| operand.get("Copy")) {
        return operand_place_to_ir_term(place, prior, formals, named_locals);
    }
    if let Some(constant) = operand.get("Const") {
        return constant_to_ir_term(constant);
    }
    None
}

/// Lift a Place to an IrTerm for the discriminant / Assert-operand
/// trace. Handles bare `Local`, `Projection(base, Deref)`,
/// `Projection(base, PtrMetadata)` (the slice-fat-pointer length
/// component, lifted as `Ctor("len", [base])`), and one Use-hop
/// trace through prior assignments. Does NOT trace through BinaryOp
/// rvalues — that's `place_to_term_for_post`'s job (used only for
/// return-value derivation, where deep tracing is expected and
/// matches the AST walk's lift).
///
/// Named-local stop rule (Task 3): if the temp local being traced has
/// a Charon-preserved source name (i.e. it was a `let y = ...`
/// binding), we emit `Var(name)` and stop — we do NOT continue
/// tracing through its assignment. This makes `let y = x; if y < 10
/// { panic!() }` produce `Var("y")` matching the AST walk's
/// surface-level name rather than tracing all the way back to
/// `Var("x")`.
fn operand_place_to_ir_term(
    place: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
    named_locals: &HashMap<u32, String>,
) -> Option<IrTerm> {
    let kind = place.get("kind")?;
    if let Some(local) = kind.get("Local").and_then(|v| v.as_u64()) {
        let local = local as u32;
        if let Some((_, name)) = formals.iter().find(|(id, _)| *id == local) {
            return Some(IrTerm::Var { name: name.clone() });
        }
        // Named-local stop: if this non-formal has a source name, emit
        // Var(name) without tracing further. This keeps the predicate
        // at the source-level name the programmer wrote.
        if let Some(name) = named_locals.get(&local) {
            return Some(IrTerm::Var { name: name.clone() });
        }
        let rvalue = find_last_assign_rvalue(prior, local)?;
        if let Some(use_op) = rvalue.get("Use") {
            return operand_to_ir_term(use_op, prior, formals, named_locals);
        }
        return None;
    }
    if let Some(proj_arr) = kind.get("Projection").and_then(|v| v.as_array()) {
        if proj_arr.len() == 2 {
            let base = &proj_arr[0];
            let elem = &proj_arr[1];
            if let Some(name) = elem.as_str() {
                match name {
                    "Deref" => return operand_place_to_ir_term(base, prior, formals, named_locals),
                    "PtrMetadata" => {
                        let inner = operand_place_to_ir_term(base, prior, formals, named_locals)?;
                        return Some(IrTerm::Ctor {
                            name: "len".to_string(),
                            args: vec![inner],
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn constant_to_ir_term(constant: &Value) -> Option<IrTerm> {
    let kind = constant.get("kind")?;
    let lit = kind.get("Literal")?;
    let scalar = lit.get("Scalar")?;
    if let Some(uns) = scalar.get("Unsigned").and_then(|v| v.as_array()) {
        // ["U32", "10"]
        let s = uns.get(1)?.as_str()?;
        let n: i64 = s.parse().ok()?;
        return Some(crate::wp::const_int(n));
    }
    if let Some(sgn) = scalar.get("Signed").and_then(|v| v.as_array()) {
        let s = sgn.get(1)?.as_str()?;
        let n: i64 = s.parse().ok()?;
        return Some(crate::wp::const_int(n));
    }
    None
}

/// Lift a bare arm-pattern scalar (as appears in `SwitchInt` arm patterns)
/// to an IrTerm. Arm pattern literals in Charon's JSON have the shape
/// `{"Scalar": {"Unsigned": ["U32", "0"]}}` or `{"Scalar": {"Signed": ["I32", "1"]}}` --
/// note the absence of the `kind.Literal` wrapper that `constant_to_ir_term`
/// expects. This helper handles the bare-Scalar encoding directly.
fn arm_scalar_to_ir_term(scalar_val: &Value) -> Option<IrTerm> {
    let scalar = scalar_val.get("Scalar")?;
    if let Some(uns) = scalar.get("Unsigned").and_then(|v| v.as_array()) {
        let s = uns.get(1)?.as_str()?;
        let n: i64 = s.parse().ok()?;
        return Some(crate::wp::const_int(n));
    }
    if let Some(sgn) = scalar.get("Signed").and_then(|v| v.as_array()) {
        let s = sgn.get(1)?.as_str()?;
        let n: i64 = s.parse().ok()?;
        return Some(crate::wp::const_int(n));
    }
    None
}

fn operand_to_local_id(operand: &Value) -> Option<u32> {
    let place = operand.get("Move").or_else(|| operand.get("Copy"))?;
    place_to_local_id(place)
}

fn place_to_local_id(place: &Value) -> Option<u32> {
    place
        .get("kind")?
        .get("Local")?
        .as_u64()
        .map(|n| n as u32)
}

fn find_last_assign_rvalue<'a>(prior: &[&'a Value], local: u32) -> Option<&'a Value> {
    for s in prior.iter().rev() {
        let Some(arr) = stmt_kind_payload(s, "Assign").and_then(|v| v.as_array()) else {
            continue;
        };
        if arr.len() != 2 {
            continue;
        }
        let assigned_local = place_to_local_id(&arr[0])?;
        if assigned_local == local {
            return Some(&arr[1]);
        }
    }
    None
}

fn stmt_kind_tag(stmt: &Value) -> Option<&str> {
    let kind = stmt.get("kind")?;
    if let Some(s) = kind.as_str() {
        return Some(s);
    }
    kind.as_object()?.keys().next().map(|k| k.as_str())
}

fn stmt_kind_payload<'a>(stmt: &'a Value, tag: &str) -> Option<&'a Value> {
    stmt.get("kind")?.get(tag)
}

fn block_statements(block: &Value) -> Vec<&Value> {
    block
        .get("statements")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn mir_binop_to_ir_predicate(op: &str) -> Option<&'static str> {
    match op {
        "Lt" => Some("<"),
        "Le" => Some("≤"),
        "Gt" => Some(">"),
        "Ge" => Some("≥"),
        "Eq" => Some("="),
        "Ne" => Some("≠"),
        _ => None,
    }
}

/// Comparison flip on negation, matching lift.rs's `negate` helper:
/// `¬(x < y) → x ≥ y`, etc. This is what makes the LLBC walk's
/// emitted formula byte-identical to the AST walk's (e.g.
/// `Atomic("≥", [Var("x"), Const(10)])` instead of
/// `Not([Atomic("<", ...)])`).
fn negate_predicate(f: IrFormula) -> IrFormula {
    if let IrFormula::Atomic { name, args } = &f {
        let flipped = match name.as_str() {
            "<" => Some("≥"),
            "≤" => Some(">"),
            ">" => Some("≤"),
            "≥" => Some("<"),
            "=" => Some("≠"),
            "≠" => Some("="),
            _ => None,
        };
        if let Some(new_name) = flipped {
            return IrFormula::Atomic {
                name: new_name.to_string(),
                args: args.clone(),
            };
        }
    }
    IrFormula::Not { operands: vec![f] }
}

fn simplify_conjunction(parts: Vec<IrFormula>) -> IrFormula {
    if parts.is_empty() {
        atomic_true().into_formula()
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        IrFormula::And { operands: parts }
    }
}

// ---- Memento construction ----

/// Synthesize a minimal `syn::ItemFn` carrying just the name and formal
/// idents. We reuse `build_function_contract`'s plumbing for sort
/// extraction and locus, then override pre/post with LLBC-derived
/// formulas.
fn synth_item_fn(name: &str, formals: &[(u32, String)]) -> syn::ItemFn {
    let formal_names: Vec<String> = formals.iter().map(|(_, n)| n.clone()).collect();
    let formal_args: Vec<String> = formal_names.iter().map(|n| format!("{}: i64", n)).collect();
    let src = format!("fn {}({}) {{}}", name, formal_args.join(", "));
    syn::parse_str::<syn::ItemFn>(&src).expect("synth fn parses")
}

/// Replace the pre/post on the memento with LLBC-derived formulas and
/// recompute the canonical bytes + CID. Uses contract.rs's shared
/// `build_memento_value` helper so the bytes match what
/// build_function_contract_with_file would produce given these
/// formulas.
fn override_formulas(
    mut c: FunctionContractMemento,
    pre: IrFormula,
    post: IrFormula,
) -> FunctionContractMemento {
    use crate::canonical::{cid_of_value, jcs_bytes_of_value};

    c.pre = pre;
    c.post = post;
    let value = crate::contract::build_memento_value(&c);
    c.canonical_bytes = jcs_bytes_of_value(&value);
    c.cid = cid_of_value(&value);
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llbc::LlbcCrate;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn lifts_clean_fixture_function_f_to_x_ge_10() {
        let krate = LlbcCrate::from_path(fixture_path("clean.llbc")).unwrap();
        let f = krate.function_by_name("f").unwrap();
        let contract = lift_llbc_function(f, Some("clean.rs")).unwrap();

        assert_eq!(contract.fn_name, "f");
        assert_eq!(contract.formals, vec!["x".to_string()]);

        // The keystone empirical claim: pre = (x ≥ 10), the negation
        // of the if-panic discriminant (x < 10), formula-byte-equal
        // to what the AST walk produces for the same Rust source.
        match &contract.pre {
            IrFormula::Atomic { name, args } => {
                assert_eq!(name, "≥", "must use the comparison-flipped predicate");
                assert_eq!(args.len(), 2);
                match &args[0] {
                    IrTerm::Var { name } => assert_eq!(name, "x"),
                    other => panic!("expected Var(x), got {:?}", other),
                }
                match &args[1] {
                    IrTerm::Const { .. } => {}
                    other => panic!("expected Const, got {:?}", other),
                }
            }
            other => panic!("expected Atomic ≥, got {:?}", other),
        }
    }

    #[test]
    fn lifts_compound_or_to_conjunction_of_negated_atoms() {
        // `if x < 10 || y < 5 panic` short-circuits in MIR into nested
        // Switches: outer aborts when x<10, else inner aborts when
        // y<5. After ¬-flip the contribution is (x≥10) ∧ (y≥5),
        // matching what AST's De Morgan produces from the same source.
        let krate = LlbcCrate::from_path(fixture_path("compound_or.llbc")).unwrap();
        let h = krate.function_by_name("h").unwrap();
        let contract = lift_llbc_function(h, Some("compound_or.rs")).unwrap();

        assert_eq!(contract.fn_name, "h");
        assert_eq!(contract.formals, vec!["x".to_string(), "y".to_string()]);

        match &contract.pre {
            IrFormula::And { operands } => {
                assert_eq!(operands.len(), 2, "two conjuncts: x≥10 ∧ y≥5");
                // First conjunct: x ≥ 10
                match &operands[0] {
                    IrFormula::Atomic { name, args } => {
                        assert_eq!(name, "≥");
                        match &args[0] {
                            IrTerm::Var { name } => assert_eq!(name, "x"),
                            other => panic!("expected Var(x), got {:?}", other),
                        }
                    }
                    other => panic!("expected first conjunct ≥, got {:?}", other),
                }
                // Second conjunct: y ≥ 5
                match &operands[1] {
                    IrFormula::Atomic { name, args } => {
                        assert_eq!(name, "≥");
                        match &args[0] {
                            IrTerm::Var { name } => assert_eq!(name, "y"),
                            other => panic!("expected Var(y), got {:?}", other),
                        }
                    }
                    other => panic!("expected second conjunct ≥, got {:?}", other),
                }
            }
            other => panic!("expected And, got {:?}", other),
        }
    }

    #[test]
    fn cross_layer_compound_or_predicate_equality() {
        // The compound-condition empirical claim: `if x<10 || y<5
        // panic` lifts to byte-identical formulas through both
        // layers. AST applies De Morgan in the predicate lift; LLBC
        // walks the nested Switch::If chain and accumulates ¬-flipped
        // conjuncts. They converge.
        use crate::canonical::{formula_to_canonical, jcs_bytes_of_value};
        use crate::contract::build_function_contract;

        let src = std::fs::read_to_string(fixture_path("compound_or.rs")).unwrap();
        let file: syn::File = syn::parse_str(&src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "h" => Some(f),
                _ => None,
            })
            .unwrap();
        let ast_contract = build_function_contract(&item_fn, None);

        let krate = LlbcCrate::from_path(fixture_path("compound_or.llbc")).unwrap();
        let h = krate.function_by_name("h").unwrap();
        let llbc_contract = lift_llbc_function(h, Some("compound_or.rs")).unwrap();

        let ast_pre = jcs_bytes_of_value(&formula_to_canonical(&ast_contract.pre));
        let llbc_pre = jcs_bytes_of_value(&formula_to_canonical(&llbc_contract.pre));
        assert_eq!(
            ast_pre, llbc_pre,
            "cross-layer compound-OR pre formula bytes must match"
        );
    }

    #[test]
    fn cross_layer_predicate_equality_with_ast_walk() {
        // Paper 07 §6 across substrate layers: the SAME Rust source,
        // lifted through TWO different IRs (surface AST via syn,
        // post-borrow-check MIR via Charon), produces byte-identical
        // predicate formulas. Encoded as JCS, the bytes are the same.
        // That's the empirical proof that the substrate's predicate
        // edges are layer-agnostic.
        use crate::canonical::{formula_to_canonical, jcs_bytes_of_value};
        use crate::contract::build_function_contract;
        use crate::llbc::LlbcCrate;

        // ---- AST layer ----
        let src = std::fs::read_to_string(fixture_path("clean.rs")).unwrap();
        let file: syn::File = syn::parse_str(&src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "f" => Some(f),
                _ => None,
            })
            .unwrap();
        let ast_contract = build_function_contract(&item_fn, None);

        // ---- LLBC layer ----
        let krate = LlbcCrate::from_path(fixture_path("clean.llbc")).unwrap();
        let f = krate.function_by_name("f").unwrap();
        let llbc_contract = lift_llbc_function(f, Some("clean.rs")).unwrap();

        // The structural claim: same fn_name, same formals.
        assert_eq!(ast_contract.fn_name, llbc_contract.fn_name);
        assert_eq!(ast_contract.formals, llbc_contract.formals);

        // The empirical claim: byte-equal pre and post formulas after
        // JCS encoding. Capture-avoiding subst at AST + capture-free
        // locals at LLBC converge on the same predicate bytes.
        let ast_pre_jcs = jcs_bytes_of_value(&formula_to_canonical(&ast_contract.pre));
        let llbc_pre_jcs = jcs_bytes_of_value(&formula_to_canonical(&llbc_contract.pre));
        assert_eq!(
            ast_pre_jcs, llbc_pre_jcs,
            "cross-layer pre formula bytes must match"
        );

        let ast_post_jcs = jcs_bytes_of_value(&formula_to_canonical(&ast_contract.post));
        let llbc_post_jcs = jcs_bytes_of_value(&formula_to_canonical(&llbc_contract.post));
        assert_eq!(
            ast_post_jcs, llbc_post_jcs,
            "cross-layer post formula bytes must match"
        );
    }

    // ---- Task 1: struct field access (Adt projection) ----

    #[test]
    fn llbc_lifts_struct_field_to_named_field_ctor() {
        // `fn p(p: &Point) -> u32 { p.x }` — LLBC emits a nested
        // projection: Deref then Field(Adt(0, null), 0). The lifter
        // must resolve field index 0 of type_decl 0 to name "x" from
        // the crate's type_decls table.
        let krate = LlbcCrate::from_path(fixture_path("struct_field.llbc")).unwrap();
        let f = krate.function_by_name("p").unwrap();
        let contract =
            lift_llbc_function_with_types(f, Some("struct_field.rs"), krate.type_decls_raw())
                .unwrap();

        assert_eq!(contract.fn_name, "p");
        assert_eq!(contract.formals, vec!["p".to_string()]);

        // Post should contain `result = Ctor("field", [Var("p"), Var(".x")])`.
        let post_str = serde_json::to_string(&contract.post).unwrap();
        assert!(
            post_str.contains("\"field\""),
            "post should contain field ctor: {}",
            post_str
        );
        assert!(
            post_str.contains("\".x\""),
            "post should use named field .x (not .0): {}",
            post_str
        );
    }

    #[test]
    fn cross_layer_struct_field_byte_equality() {
        // AST and LLBC agree on `result = Ctor("field", [Var("p"), Var(".x")])`.
        // AST's Expr::Field(Named("x")) and LLBC's Field(Adt(0), 0) with
        // type_decls name lookup both produce the same term bytes.
        use crate::canonical::{formula_to_canonical, jcs_bytes_of_value};
        use crate::contract::build_function_contract;

        let src = std::fs::read_to_string(fixture_path("struct_field.rs")).unwrap();
        let file: syn::File = syn::parse_str(&src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "p" => Some(f),
                _ => None,
            })
            .unwrap();
        let ast_contract = build_function_contract(&item_fn, None);

        let krate = LlbcCrate::from_path(fixture_path("struct_field.llbc")).unwrap();
        let f = krate.function_by_name("p").unwrap();
        let llbc_contract =
            lift_llbc_function_with_types(f, Some("struct_field.rs"), krate.type_decls_raw())
                .unwrap();

        let ast_post = jcs_bytes_of_value(&formula_to_canonical(&ast_contract.post));
        let llbc_post = jcs_bytes_of_value(&formula_to_canonical(&llbc_contract.post));
        assert_eq!(
            ast_post, llbc_post,
            "struct field: AST and LLBC post must be byte-identical"
        );
    }

    // ---- Task 2: general tuple element projection ----

    #[test]
    fn llbc_lifts_tuple_field_to_index_ctor() {
        // `fn t(p: (u32, u32)) -> u32 { p.0 }` — LLBC emits
        // Field(Tuple(2), 0) on a formal Local. The CheckedOp shortcut
        // must NOT fire (base is a formal, not a BinaryOp result);
        // the general tuple path must produce Ctor("field", [Var("p"), Var(".0")]).
        let krate = LlbcCrate::from_path(fixture_path("tuple_field.llbc")).unwrap();
        let f = krate.function_by_name("t").unwrap();
        let contract =
            lift_llbc_function_with_types(f, Some("tuple_field.rs"), krate.type_decls_raw())
                .unwrap();

        assert_eq!(contract.fn_name, "t");
        assert_eq!(contract.formals, vec!["p".to_string()]);

        let post_str = serde_json::to_string(&contract.post).unwrap();
        assert!(
            post_str.contains("\"field\""),
            "post should contain field ctor: {}",
            post_str
        );
        assert!(
            post_str.contains("\".0\""),
            "post should use index .0: {}",
            post_str
        );
    }

    #[test]
    fn cross_layer_tuple_field_byte_equality() {
        // AST emits Ctor("field", [Var("p"), Var(".0")]) for `p.0`
        // via syn::Member::Unnamed. LLBC emits the same via the
        // general Field(Tuple, idx) path. Bytes must match.
        use crate::canonical::{formula_to_canonical, jcs_bytes_of_value};
        use crate::contract::build_function_contract;

        let src = std::fs::read_to_string(fixture_path("tuple_field.rs")).unwrap();
        let file: syn::File = syn::parse_str(&src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "t" => Some(f),
                _ => None,
            })
            .unwrap();
        let ast_contract = build_function_contract(&item_fn, None);

        let krate = LlbcCrate::from_path(fixture_path("tuple_field.llbc")).unwrap();
        let f = krate.function_by_name("t").unwrap();
        let llbc_contract =
            lift_llbc_function_with_types(f, Some("tuple_field.rs"), krate.type_decls_raw())
                .unwrap();

        let ast_post = jcs_bytes_of_value(&formula_to_canonical(&ast_contract.post));
        let llbc_post = jcs_bytes_of_value(&formula_to_canonical(&llbc_contract.post));
        assert_eq!(
            ast_post, llbc_post,
            "tuple field: AST and LLBC post must be byte-identical"
        );
    }
    // ---- Task A: SwitchInt multi-arm match ----

    #[test]
    fn llbc_lifts_match_arms_to_neq_conjunction() {
        // `fn f(x: u32) { match x { 0 | 1 => panic!(), _ => {} } }`
        // Charon emits Switch::SwitchInt with one arm carrying patterns
        // [0, 1] that leads to Abort (via fall-through). The lift should
        // produce pre = (x ≠ 0) /\ (x ≠ 1).
        let krate = LlbcCrate::from_path(fixture_path("match_arms.llbc")).unwrap();
        let f = krate.function_by_name("f").unwrap();
        let contract = lift_llbc_function(f, Some("match_arms.rs")).unwrap();

        assert_eq!(contract.fn_name, "f");
        assert_eq!(contract.formals, vec!["x".to_string()]);

        match &contract.pre {
            IrFormula::And { operands } => {
                assert_eq!(operands.len(), 2, "expected two ≠ conjuncts for 0|1 arm: {:?}", contract.pre);
                for op in operands {
                    match op {
                        IrFormula::Atomic { name, args } => {
                            assert_eq!(name, "≠", "predicate must be ≠: {}", name);
                            assert_eq!(args.len(), 2);
                            match &args[0] {
                                IrTerm::Var { name } => assert_eq!(name, "x"),
                                other => panic!("expected Var(x), got {:?}", other),
                            }
                        }
                        other => panic!("expected Atomic ≠, got {:?}", other),
                    }
                }
            }
            other => panic!("expected And conjunction, got {:?}", other),
        }
    }

    // ---- Task A: LLBC Panics effect detection ----

    #[test]
    fn detect_effects_llbc_emits_panics_for_abort_body() {
        // `fn f(x: u32) { if x < 10 { panic!() } }` — clean.llbc has
        // an Abort statement in the body. detect_effects_llbc must emit
        // Effect::Panics.
        use crate::contract::Effect;
        let krate = LlbcCrate::from_path(fixture_path("clean.llbc")).unwrap();
        let f = krate.function_by_name("f").unwrap();
        let contract = lift_llbc_function(f, Some("clean.rs")).unwrap();
        assert!(
            contract.effects.effects.contains(&Effect::Panics),
            "clean.rs has panic! — LLBC contract must carry Effect::Panics; got {:?}",
            contract.effects.effects
        );
    }

    #[test]
    fn detect_effects_llbc_pure_for_no_abort_body() {
        // `fn t(p: (u32, u32)) -> u32 { p.0 }` — no abort, no calls.
        // Effects should be empty (pure).
        let krate = LlbcCrate::from_path(fixture_path("tuple_field.llbc")).unwrap();
        let f = krate.function_by_name("t").unwrap();
        let contract =
            lift_llbc_function_with_types(f, Some("tuple_field.rs"), krate.type_decls_raw())
                .unwrap();
        assert!(
            contract.effects.is_pure(),
            "tuple_field has no panic/io — LLBC contract should be pure; got {:?}",
            contract.effects.effects
        );
    }

    // ---- Task B: UnresolvedCall effect via empty registry ----

    #[test]
    fn empty_registry_emits_unresolved_call_for_outer() {
        // `outer` calls `inner` (defined in the same crate). When we lift
        // `outer` with an EMPTY registry (no inner contract registered),
        // the lifter cannot resolve `inner` and must emit
        // Effect::UnresolvedCall { name: "inner" } in outer's effect set.
        use crate::contract::Effect;
        use crate::llbc_calls::{empty_registry, fun_decls_array};
        let krate = LlbcCrate::from_path(fixture_path("calls.llbc")).unwrap();
        let outer = krate.function_by_name("outer").unwrap();
        let fun_decls = fun_decls_array(&krate);
        let contract = lift_llbc_function_with_registry(
            outer,
            Some("calls.rs"),
            krate.type_decls_raw(),
            fun_decls,
            &empty_registry(), // intentionally empty — forces unresolved
        )
        .unwrap();

        let has_unresolved = contract.effects.effects.iter().any(|e| {
            matches!(e, Effect::UnresolvedCall { name } if name == "inner")
        });
        assert!(
            has_unresolved,
            "outer calls inner with empty registry — must carry UnresolvedCall(inner); got {:?}",
            contract.effects.effects
        );
    }

    // ---- Task A: enum variant projection (adt_field_name with variant_id) ----
    //
    // These unit tests exercise adt_field_name directly using synthetic
    // serde_json::json! values, matching the JSON shape confirmed from
    // the enum_field.llbc fixture. No fixture file or Charon invocation
    // required. Cross-layer byte equality does NOT hold for enum match
    // patterns because the AST walk does not lift match arms at all;
    // this is an LlbcExtra site per paper 07's layered-agreement taxonomy.

    #[test]
    fn adt_field_name_struct_returns_named_field() {
        // Struct: kind.Struct = [{name: "x", ...}, {name: "y", ...}]
        // variant_id = None.
        let type_decls = serde_json::json!([
            {
                "def_id": 0,
                "kind": {
                    "Struct": [
                        {"name": "x", "ty": {}},
                        {"name": "y", "ty": {}}
                    ]
                }
            }
        ]);
        assert_eq!(
            adt_field_name(&type_decls, 0, None, 0),
            Some("x".to_string()),
            "struct field 0 should be named x"
        );
        assert_eq!(
            adt_field_name(&type_decls, 0, None, 1),
            Some("y".to_string()),
            "struct field 1 should be named y"
        );
    }

    #[test]
    fn adt_field_name_enum_named_variant_returns_field_name() {
        // Enum `E { A(u32), B { x: u32 } }`:
        // kind.Enum[0] = variant A with unnamed field (name: null).
        // kind.Enum[1] = variant B with named field x.
        // Matches the JSON shape from tests/fixtures/enum_field.llbc.
        let type_decls = serde_json::json!([
            {
                "def_id": 0,
                "kind": {
                    "Enum": [
                        {
                            "name": "A",
                            "fields": [{"name": null, "ty": {}}]
                        },
                        {
                            "name": "B",
                            "fields": [{"name": "x", "ty": {}}]
                        }
                    ]
                }
            }
        ]);
        // Variant A (idx 0), field 0: unnamed field returns None so the
        // caller uses the numeric index fallback (.0).
        assert_eq!(
            adt_field_name(&type_decls, 0, Some(0), 0),
            None,
            "unnamed tuple-variant field should return None (caller uses .0)"
        );
        // Variant B (idx 1), field 0: named field "x".
        assert_eq!(
            adt_field_name(&type_decls, 0, Some(1), 0),
            Some("x".to_string()),
            "named enum variant field should return its source name"
        );
    }

    #[test]
    fn adt_field_name_unknown_adt_id_returns_none() {
        let type_decls = serde_json::json!([
            {"def_id": 0, "kind": {"Struct": [{"name": "x", "ty": {}}]}}
        ]);
        assert_eq!(
            adt_field_name(&type_decls, 99, None, 0),
            None,
            "unknown adt_id should return None"
        );
    }

    #[test]
    fn adt_field_name_out_of_range_field_idx_returns_none() {
        let type_decls = serde_json::json!([
            {"def_id": 0, "kind": {"Struct": [{"name": "x", "ty": {}}]}}
        ]);
        assert_eq!(
            adt_field_name(&type_decls, 0, None, 5),
            None,
            "out-of-range field_idx should return None"
        );
    }

    // Task B: closure captures.
    //
    // Skipped. The Aggregate rvalue in Charon's LLBC uses "Adt" as the
    // AggregateKind for closures (there is no distinct "Closure" variant in
    // the JSON). The closure type is an opaque ADT whose def_id points to
    // the closure type in type_decls, but detecting "this ADT is a closure
    // type" requires navigating the type_decls table and checking for a
    // closure-specific marker. That lookup is entangled with generic def_id
    // resolution machinery not yet at the LLBC layer. Landing a partial match
    // arm that fires on ANY Adt aggregate would corrupt rvalue_to_ir_term_for_post
    // for ordinary struct construction. Skipped per the task's explicit
    // "skip if tangled" discipline.
    //
    // The closure_capture.llbc fixture and closure_capture.rs source are
    // retained in tests/fixtures/ as reference material for a future commit.

    // ---- Task A: Unsafe effect via signature.is_unsafe ----

    #[test]
    fn detect_effects_llbc_emits_unsafe_for_unsafe_fn() {
        // `drop_in_place` in closure_capture.llbc is a compiler-generated
        // intrinsic with `signature.is_unsafe: true`. Lifting it must
        // emit Effect::Unsafe in the contract's effect set.
        use crate::contract::Effect;
        let krate =
            LlbcCrate::from_path(fixture_path("closure_capture.llbc")).unwrap();
        let f = krate.function_by_name("drop_in_place").unwrap();
        assert!(
            f.is_unsafe(),
            "drop_in_place must have is_unsafe=true in the fixture"
        );
        let contract = lift_llbc_function(f, Some("closure_capture.rs")).unwrap();
        assert!(
            contract.effects.effects.contains(&Effect::Unsafe),
            "unsafe fn must carry Effect::Unsafe; got {:?}",
            contract.effects.effects
        );
    }
}