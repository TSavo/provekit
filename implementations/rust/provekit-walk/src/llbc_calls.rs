// SPDX-License-Identifier: Apache-2.0
//
// LLBC callsite composition (#383, Tier 1.1). The keystone unlock for
// non-leaf compositions. Without it, only leaf functions (no calls)
// fully lift; with it, every call site at a function boundary
// substitutes the callee's precondition into the caller's, so paper
// 07 §6's "compose for free" works at the substrate level.
//
// The composition is mechanical:
//   At each `StatementKind::Call(call)` in the caller's body:
//     1. Resolve the FunDeclId in `call.func.Regular.kind.Fun.Regular`
//        to the callee's name (the trailing Ident in its
//        item_meta.name path).
//     2. Look up the callee in the ContractRegistry.
//     3. Lift each actual arg via `operand_to_ir_term`.
//     4. For each (formal_name, actual_term) pair, capture-avoiding
//        substitute formal_name -> actual_term in the callee's pre
//        (using `wp::substitute_in_formula` we already built for the
//        AST walk).
//     5. Push the substituted pre into the caller's pre contributions.
//
// Unresolved callees (cross-crate, dyn dispatch, FFI, callee not in
// registry) are skipped silently. Future work: emit
// `Effect::UnresolvedCall { name }` so the substrate refuses to
// compose against them rather than implicitly assuming `true`.

use std::collections::HashMap;

use serde_json::Value;

use provekit_ir_types::{IrFormula, IrTerm};

use crate::contract::FunctionContractMemento;
use crate::llbc::{LlbcCrate, LlbcError};
use crate::wp;

/// Map from function name (the trailing Ident in the path) to its
/// already-lifted contract memento. Used for callsite composition:
/// when the lifter sees a Call to `f`, it looks up f's contract and
/// substitutes actuals for formals in f's pre.
pub type ContractRegistry = HashMap<String, FunctionContractMemento>;

/// Empty registry — used by entry points that don't need callsite
/// composition (e.g., legacy `lift_llbc_function` callers).
pub fn empty_registry() -> ContractRegistry {
    ContractRegistry::new()
}

/// Lift every function in the crate, building a registry as we go.
/// Calls are composed against the registry as it's built — earlier
/// functions in `fun_decls` order are available to later ones. For
/// recursive functions or forward references, the first lift sees an
/// empty entry; subsequent compositions can pick up the contract on a
/// second pass if needed (not yet implemented).
///
/// `source_path` is annotated into each contract's locus.
pub fn lift_llbc_crate(
    krate: &LlbcCrate,
    source_path: Option<&str>,
) -> Result<ContractRegistry, LlbcError> {
    let mut registry = ContractRegistry::new();
    let type_decls = krate.type_decls_raw();
    let fun_decls_raw = fun_decls_array(krate);

    for f in krate.fun_decls() {
        let Some(name) = f.fn_name() else {
            continue;
        };
        match crate::llbc_lift::lift_llbc_function_with_registry(
            f,
            source_path,
            type_decls,
            fun_decls_raw,
            &registry,
        ) {
            Ok(contract) => {
                registry.insert(name, contract);
            }
            Err(_) => {
                // Skip non-structured bodies (extern, intrinsic, trait method).
            }
        }
    }
    Ok(registry)
}

/// Get the raw `fun_decls` array from a crate. Returned as `Option`
/// because foreign / minimal crates may omit it.
pub fn fun_decls_array(krate: &LlbcCrate) -> Option<&Value> {
    let translated = krate.raw_translated()?;
    translated.get("fun_decls")
}

/// Look up a function declaration by its `def_id` (FunDeclId) in the
/// crate's fun_decls table and return the trailing `Ident` of its
/// item_meta.name path. `None` if not found or path is malformed.
pub fn fundecl_name_by_id(fun_decls: &Value, id: u64) -> Option<String> {
    let arr = fun_decls.as_array()?;
    let decl = arr.iter().find(|d| d.get("def_id").and_then(|v| v.as_u64()) == Some(id))?;
    let elems = decl.get("item_meta")?.get("name")?.as_array()?;
    elems.iter().rev().find_map(|e| {
        let ident = e.get("Ident")?.as_array()?;
        ident.first()?.as_str().map(|s| s.to_string())
    })
}

/// Extract the FunDeclId and the args operands from a Call statement.
/// Returns `None` for non-Call statements or for Call kinds we don't
/// resolve (FnPtr, dyn dispatch, closure invocation).
pub fn extract_call_target(stmt: &Value) -> Option<(u64, Vec<&Value>)> {
    let call = stmt.get("kind")?.get("Call")?;
    let func = call.get("func")?;
    let regular = func.get("Regular")?;
    let kind = regular.get("kind")?;
    let fun = kind.get("Fun")?;
    let func_id = fun.get("Regular")?.as_u64()?;
    let args: Vec<&Value> = call
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default();
    Some((func_id, args))
}

/// Substitute actuals for formals in the callee's pre. The callee's
/// formal names are the keys; the actuals are the corresponding lifted
/// IrTerms in the same positional order. Capture-avoiding substitution
/// is the default semantics of `wp::substitute_in_formula`.
pub fn compose_callsite_pre(
    callee: &FunctionContractMemento,
    arg_terms: &[IrTerm],
) -> IrFormula {
    let mut substituted = callee.pre.clone();
    for (formal_name, actual_term) in callee.formals.iter().zip(arg_terms.iter()) {
        substituted = wp::substitute_in_formula(substituted, formal_name, actual_term);
    }
    substituted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn lift_llbc_crate_builds_registry_for_calls_fixture() {
        // The calls fixture has two functions:
        //   fn inner(y: u32) -> u32 { if y < 5 { panic!(); } y }
        //   fn outer(x: u32) -> u32 { inner(x) }
        // After full-crate lift:
        //   inner.pre = (y ≥ 5)
        //   outer.pre = (x ≥ 5)   -- inner's pre with y → x substituted
        let krate = LlbcCrate::from_path(fixture_path("calls.llbc")).unwrap();
        let registry = lift_llbc_crate(&krate, Some("calls.rs")).unwrap();

        let inner = registry.get("inner").expect("inner lifted");
        assert_eq!(inner.formals, vec!["y".to_string()]);
        let inner_pre_str = serde_json::to_string(&inner.pre).unwrap();
        assert!(
            inner_pre_str.contains("\"y\""),
            "inner's pre references y: {}",
            inner_pre_str
        );

        let outer = registry.get("outer").expect("outer lifted");
        assert_eq!(outer.formals, vec!["x".to_string()]);
        let outer_pre_str = serde_json::to_string(&outer.pre).unwrap();
        // The substrate keystone: outer's pre carries the SUBSTITUTED
        // inner.pre — `y → x`. So outer.pre references x (not y).
        assert!(
            outer_pre_str.contains("\"x\""),
            "outer's pre references x (after composition): {}",
            outer_pre_str
        );
        assert!(
            !outer_pre_str.contains("\"y\""),
            "outer's pre must NOT reference y (formal substituted away): {}",
            outer_pre_str
        );
        // The predicate is ≥ 5 — same atom as inner's pre.
        assert!(
            outer_pre_str.contains("\"\\u2265\"") || outer_pre_str.contains("≥"),
            "outer's pre has the ≥ predicate: {}",
            outer_pre_str
        );
    }

    #[test]
    fn compose_callsite_pre_substitutes_formal_to_actual() {
        // Direct unit test on the composition primitive. Build a
        // synthetic callee with pre = (y ≥ 5), substitute y → Var("x"),
        // assert the result is (x ≥ 5).
        use crate::contract::{EffectSet, FunctionContractMemento};
        use crate::locus::Locus;
        use crate::wp::{atomic_ge, const_int, var};
        use provekit_ir_types::Sort;

        let pre = atomic_ge(var("y"), const_int(5)).into_formula();
        let post = pre.clone();
        let callee = FunctionContractMemento {
            fn_name: "inner".into(),
            formals: vec!["y".into()],
            formal_sorts: vec![Sort::Primitive { name: "Int".into() }],
            return_sort: Sort::Primitive { name: "Int".into() },
            pre,
            post,
            body_cid: None,
            effects: EffectSet::empty(),
            locus: Locus::unknown(),
            canonical_bytes: vec![],
            cid: String::new(),
        };
        let composed = compose_callsite_pre(&callee, &[var("x")]);
        // Should be (x ≥ 5).
        let s = serde_json::to_string(&composed).unwrap();
        assert!(s.contains("\"x\""));
        assert!(!s.contains("\"y\""));
    }
}
