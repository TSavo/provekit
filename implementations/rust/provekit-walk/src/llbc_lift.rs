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

use serde_json::Value;

use provekit_ir_types::{IrFormula, IrTerm, Sort};

use crate::contract::{build_function_contract_with_file, FunctionContractMemento};
use crate::llbc::{LlbcError, LlbcFunction};
use crate::wp::atomic_true;

/// Lift one LLBC function to a FunctionContractMemento. The
/// `source_path` is annotated into the memento's locus for downstream
/// developer-feedback paths.
pub fn lift_llbc_function(
    f: LlbcFunction<'_>,
    source_path: Option<&str>,
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

    let stmts: Vec<&Value> = f.statements().map(|s| s.raw()).collect();
    let pre_formula = derive_precondition(&stmts, &formals);
    // Postcondition mirrors the precondition for the if-panic class:
    // every reachable point on the non-panic side still satisfies the
    // contraposition. Matches the AST walk's `lift_function_postcondition`
    // behavior for fixtures without a trailing return expression.
    let post_formula = pre_formula.clone();

    // We synthesize a syn::ItemFn shell to reuse build_function_contract's
    // memento-construction machinery — it sets up the locus, sorts,
    // canonical bytes and CID, but uses our LLBC-derived pre/post
    // formulas. The shell carries only the function name and formal
    // sorts; its body is empty (the predicates come from LLBC, not
    // from the surface AST).
    let item_fn = synth_item_fn(&fn_name, &formals);
    let mut contract = build_function_contract_with_file(&item_fn, None, source_path);
    // Override the lifted formulas with the LLBC-derived ones, then
    // recompute canonical bytes + CID so result_var_name and the
    // header.cid path are consistent.
    contract = override_formulas(contract, pre_formula, post_formula);
    Ok(contract)
}

/// Walk the body for if-panic patterns. For each `Switch::If(discr,
/// then-leads-to-abort, _)`, record `¬discr` as a precondition
/// contribution and recurse into the else-branch (which may contain
/// nested if-panic Switches from short-circuited compound conditions
/// like `||` and `&&`). Conjoin all contributions in source order so
/// the byte-encoding matches the AST walk's De Morgan output.
fn derive_precondition(stmts: &[&Value], formals: &[(u32, String)]) -> IrFormula {
    let mut contribs: Vec<IrFormula> = Vec::new();
    // Top-level: falling through the entire body returns; doesn't abort.
    collect_if_panic_contributions(&[], stmts, formals, false, &mut contribs);
    simplify_conjunction(contribs)
}

/// Recursive collector. `prior` is the chain of statements the
/// discriminant traces are allowed to reach back through (the
/// concatenation of every enclosing block's stmts up to the current
/// Switch). `stmts` is the current block being scanned.
/// `parent_falls_through_to_abort` is true when reaching the end of
/// the current block (without a Return/Abort inside it) leads to an
/// Abort in the enclosing scope.
fn collect_if_panic_contributions(
    prior: &[&Value],
    stmts: &[&Value],
    formals: &[(u32, String)],
    parent_falls_through_to_abort: bool,
    out: &mut Vec<IrFormula>,
) {
    for (i, s) in stmts.iter().enumerate() {
        let Some(switch) = stmt_kind_payload(s, "Switch") else {
            continue;
        };
        let Some(if_arr) = switch.get("If").and_then(|v| v.as_array()) else {
            continue;
        };
        if if_arr.len() != 3 {
            continue;
        }
        let discr = &if_arr[0];
        let then_block = &if_arr[1];
        let else_block = &if_arr[2];

        // Where does the path AFTER this Switch end up? If the next
        // non-bookkeeping statement is Abort, the fall-through aborts.
        // If we reach the end of `stmts` without hitting Abort or a
        // Return-equivalent, defer to the parent.
        let post_switch_aborts =
            tail_aborts_or_inherit(&stmts[i + 1..], parent_falls_through_to_abort);

        // Is the THEN branch the panic side?
        let then_aborts = block_leads_to_abort(then_block, post_switch_aborts);
        if !then_aborts {
            // Not an if-panic at this level. Skip; recursing into the
            // else-block of a non-panic Switch would conflate
            // narrowing with precondition, which is out of scope for
            // the if-panic class.
            continue;
        }

        // Trace the discriminant. The trace is allowed to walk back
        // through the parent `prior` plus the current block's
        // pre-Switch statements.
        let mut local_prior: Vec<&Value> = Vec::with_capacity(prior.len() + i);
        local_prior.extend(prior.iter().copied());
        local_prior.extend(stmts[..i].iter().copied());
        if let Some(pred) = discriminant_to_formula(discr, &local_prior, formals) {
            out.push(negate_predicate(pred));
        }

        // Recurse into the else-block: short-circuited `||` / `&&`
        // emits another Switch nested in the else side. The inner
        // block's "parent fall-through" property is whatever this
        // Switch's post-tail does.
        let mut new_prior = local_prior.clone();
        new_prior.push(s); // include this Switch in prior for inner discr-trace
        let inner_stmts: Vec<&Value> = block_statements(else_block);
        collect_if_panic_contributions(
            &new_prior,
            &inner_stmts,
            formals,
            post_switch_aborts,
            out,
        );
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
) -> Option<IrFormula> {
    let local = operand_to_local_id(operand)?;
    let rvalue = find_last_assign_rvalue(prior, local)?;

    if let Some(arr) = rvalue.get("BinaryOp").and_then(|v| v.as_array()) {
        if arr.len() != 3 {
            return None;
        }
        let mir_op = arr[0].as_str()?;
        let pred_name = mir_binop_to_ir_predicate(mir_op)?;
        let lhs = operand_to_ir_term(&arr[1], prior, formals)?;
        let rhs = operand_to_ir_term(&arr[2], prior, formals)?;
        return Some(IrFormula::Atomic {
            name: pred_name.to_string(),
            args: vec![lhs, rhs],
        });
    }

    if let Some(use_op) = rvalue.get("Use") {
        return discriminant_to_formula(use_op, prior, formals);
    }

    None
}

fn operand_to_ir_term(
    operand: &Value,
    prior: &[&Value],
    formals: &[(u32, String)],
) -> Option<IrTerm> {
    if let Some(place) = operand.get("Move").or_else(|| operand.get("Copy")) {
        let local = place_to_local_id(place)?;
        if let Some((_, name)) = formals.iter().find(|(id, _)| *id == local) {
            return Some(IrTerm::Var { name: name.clone() });
        }
        // Not a formal — trace one Use-hop back to its definition.
        let rvalue = find_last_assign_rvalue(prior, local)?;
        if let Some(use_op) = rvalue.get("Use") {
            return operand_to_ir_term(use_op, prior, formals);
        }
        return None;
    }
    if let Some(constant) = operand.get("Const") {
        return constant_to_ir_term(constant);
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
/// recompute the canonical bytes + CID.
fn override_formulas(
    mut c: FunctionContractMemento,
    pre: IrFormula,
    post: IrFormula,
) -> FunctionContractMemento {
    use crate::canonical::{cid_of_value, jcs_bytes_of_value};

    c.pre = pre;
    c.post = post;
    // Rebuild the canonical bytes following the same schema
    // build_function_contract_with_file uses, so the CID matches what
    // a from-scratch build would produce given these formulas.
    let value = build_memento_value(&c);
    c.canonical_bytes = jcs_bytes_of_value(&value);
    c.cid = cid_of_value(&value);
    c
}

fn build_memento_value(c: &FunctionContractMemento) -> std::sync::Arc<provekit_canonicalizer::Value> {
    use crate::canonical::formula_to_canonical;
    use provekit_canonicalizer::Value as PValue;
    use std::sync::Arc;

    let mut entries: Vec<(&'static str, Arc<PValue>)> = Vec::new();
    entries.push(("schemaVersion", PValue::string("1")));
    entries.push(("kind", PValue::string("function-contract")));
    entries.push(("fnName", PValue::string(c.fn_name.clone())));
    let formals_arr: Vec<Arc<PValue>> = c
        .formals
        .iter()
        .map(|n| PValue::string(n.clone()))
        .collect();
    entries.push(("formals", PValue::array(formals_arr)));
    let formal_sorts_arr: Vec<Arc<PValue>> = c
        .formal_sorts
        .iter()
        .map(|s| sort_to_canonical(s))
        .collect();
    entries.push(("formalSorts", PValue::array(formal_sorts_arr)));
    entries.push(("returnSort", sort_to_canonical(&c.return_sort)));
    entries.push(("pre", formula_to_canonical(&c.pre)));
    entries.push(("post", formula_to_canonical(&c.post)));
    entries.push((
        "bodyCid",
        c.body_cid
            .as_ref()
            .map(|c| PValue::string(c.clone()))
            .unwrap_or(PValue::null()),
    ));
    let effects_arr: Vec<Arc<PValue>> = vec![]; // pure
    entries.push(("effects", PValue::array(effects_arr)));
    entries.push(("locus", c.locus.to_value()));
    PValue::object(entries)
}

fn sort_to_canonical(sort: &Sort) -> std::sync::Arc<provekit_canonicalizer::Value> {
    use provekit_canonicalizer::Value as PValue;
    match sort {
        Sort::Primitive { name } => PValue::object([
            ("kind", PValue::string("primitive")),
            ("name", PValue::string(name.clone())),
        ]),
    }
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
}
