// SPDX-License-Identifier: Apache-2.0
//
// Walk-side AST builders and effect detector for FunctionContractMemento.
//
// The composition algebra (compose_chain_contracts, compose_function_contracts,
// compose_with_composed, compose_function_contracts_checked, the supporting
// data types FunctionContractMemento / ComposedFunctionContract / EffectSet /
// Effect / OpacityError / OpacityMementoLookup / ChainStep / etc.) lives at
// the workspace level in `libprovekit::compose` per the Contract Composition
// Protocol (CCP) spec, sections 2 / 5 / 9
// (protocol/specs/2026-05-09-contract-composition-protocol.md).
//
// This module re-exports those types and the canonical-encoding glue (so
// existing `use crate::contract::*` paths in walk's other modules continue
// to resolve) and adds the syn-driven helpers that BUILD a FunctionContract
// Memento from a `syn::ItemFn`. AST traversal stays in walk; the algebra
// lives once, in libprovekit.

use syn::{Expr, ExprUnsafe, FnArg, ItemFn, Pat, Stmt};

// ---- Re-export the canonical algebra and supporting types ----

pub use libprovekit::compose::{
    build_memento_value, build_value, cid_of_value, compose_chain_contracts,
    compose_function_contracts, compose_function_contracts_checked, compose_with_composed,
    formula_to_canonical, jcs_bytes_of_value, sort_to_value, substitute_in_formula,
    AliasingMemento, AliasingStatus, AtomicKind, ChainStep, ComposedFunctionContract, Effect,
    EffectSet, EmptyOpacityPool, FunctionContractMemento, Locus, OpacityError,
    OpacityMementoLookup, PinInvariantMementoView,
};

// ---- AST builders ----

/// Build a FunctionContractMemento for an `ItemFn`. The body_cid is
/// optional; pass None when the body's shadow source isn't computed
/// (e.g., during a lift-only pass). The locus carries source-position
/// metadata for downstream developer feedback; pass None to use an
/// unknown/empty locus.
pub fn build_function_contract(
    item_fn: &ItemFn,
    body_cid: Option<String>,
) -> FunctionContractMemento {
    build_function_contract_with_file(item_fn, body_cid, None)
}

/// Build a FunctionContractMemento with an explicit source file path
/// for locus annotation.
pub fn build_function_contract_with_file(
    item_fn: &ItemFn,
    body_cid: Option<String>,
    file_path: Option<&str>,
) -> FunctionContractMemento {
    let fn_name = item_fn.sig.ident.to_string();
    let (formals, formal_sorts) = extract_formals(item_fn);
    let return_sort = extract_return_sort(item_fn);
    let pre = crate::lift::lift_function_precondition(item_fn).into_formula();
    let post = crate::lift::lift_function_postcondition(item_fn).into_formula();
    let effects = detect_effects(item_fn);
    let locus = crate::locus::from_span(item_fn.sig.ident.span(), file_path);

    let value = build_value(
        &fn_name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        body_cid.as_deref(),
        &effects,
        &locus,
        &[],
    );
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    FunctionContractMemento {
        fn_name,
        formals,
        formal_sorts,
        formal_regions: vec![],
        return_sort,
        return_region: None,
        pre,
        post,
        body_cid,
        effects,
        locus,
        canonical_bytes,
        cid,
        auto_minted_mementos: vec![],
    }
}

// ---- Effect detection ----

fn detect_effects(item_fn: &ItemFn) -> EffectSet {
    let mut set = EffectSet::empty();
    // 1. &mut params
    for input in &item_fn.sig.inputs {
        if let FnArg::Typed(pt) = input {
            if let syn::Type::Reference(r) = &*pt.ty {
                if r.mutability.is_some() {
                    if let Pat::Ident(ident) = &*pt.pat {
                        set.add(Effect::Writes {
                            target: ident.ident.to_string(),
                        });
                    } else {
                        set.add(Effect::Writes {
                            target: "<param>".to_string(),
                        });
                    }
                }
            }
        }
    }
    // 2. Walk the body for unsafe, IO, panics, unknown calls.
    for stmt in &item_fn.block.stmts {
        scan_stmt_for_effects(stmt, &mut set);
    }
    set
}

fn scan_stmt_for_effects(stmt: &Stmt, set: &mut EffectSet) {
    match stmt {
        Stmt::Local(local) => {
            if let Some(init) = &local.init {
                scan_expr_for_effects(&init.expr, set);
            }
        }
        Stmt::Expr(e, _) => scan_expr_for_effects(e, set),
        Stmt::Macro(m) => scan_macro_for_effects(&m.mac, set),
        Stmt::Item(_) => {}
    }
}

fn scan_expr_for_effects(expr: &Expr, set: &mut EffectSet) {
    match expr {
        Expr::Unsafe(ExprUnsafe { block, .. }) => {
            set.add(Effect::Unsafe);
            for s in &block.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Macro(m) => scan_macro_for_effects(&m.mac, set),
        Expr::Call(c) => {
            if let Expr::Path(p) = c.func.as_ref() {
                if let Some(seg) = p.path.segments.last() {
                    let name = seg.ident.to_string();
                    if !is_known_pure_call(&name) {
                        set.add(Effect::UnresolvedCall { name });
                    }
                }
            }
            for a in &c.args {
                scan_expr_for_effects(a, set);
            }
        }
        Expr::MethodCall(m) => {
            let method = m.method.to_string();
            if is_io_method(&method) {
                set.add(Effect::Io);
            } else if !is_known_pure_method(&method) {
                set.add(Effect::UnresolvedCall {
                    name: format!(".{}", method),
                });
            }
            scan_expr_for_effects(&m.receiver, set);
            for a in &m.args {
                scan_expr_for_effects(a, set);
            }
        }
        Expr::If(i) => {
            scan_expr_for_effects(&i.cond, set);
            for s in &i.then_branch.stmts {
                scan_stmt_for_effects(s, set);
            }
            if let Some((_, e)) = &i.else_branch {
                scan_expr_for_effects(e, set);
            }
        }
        Expr::Block(b) => {
            for s in &b.block.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::While(w) => {
            scan_expr_for_effects(&w.cond, set);
            for s in &w.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::ForLoop(f) => {
            scan_expr_for_effects(&f.expr, set);
            for s in &f.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Loop(l) => {
            for s in &l.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Match(m) => {
            scan_expr_for_effects(&m.expr, set);
            for arm in &m.arms {
                scan_expr_for_effects(&arm.body, set);
            }
        }
        Expr::Assign(a) => {
            if let Expr::Path(p) = a.left.as_ref() {
                if let Some(seg) = p.path.segments.last() {
                    set.add(Effect::Writes {
                        target: seg.ident.to_string(),
                    });
                }
            }
            scan_expr_for_effects(&a.right, set);
        }
        Expr::Return(r) => {
            if let Some(inner) = &r.expr {
                scan_expr_for_effects(inner, set);
            }
        }
        Expr::Try(t) => scan_expr_for_effects(&t.expr, set),
        Expr::Binary(b) => {
            scan_expr_for_effects(&b.left, set);
            scan_expr_for_effects(&b.right, set);
        }
        Expr::Unary(u) => scan_expr_for_effects(&u.expr, set),
        Expr::Paren(p) => scan_expr_for_effects(&p.expr, set),
        Expr::Reference(r) => scan_expr_for_effects(&r.expr, set),
        Expr::Field(f) => scan_expr_for_effects(&f.base, set),
        Expr::Index(i) => {
            scan_expr_for_effects(&i.expr, set);
            scan_expr_for_effects(&i.index, set);
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                scan_expr_for_effects(e, set);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                scan_expr_for_effects(e, set);
            }
        }
        // Pure expression shapes; no effects to add.
        Expr::Lit(_) | Expr::Path(_) | Expr::Closure(_) => {}
        _ => {}
    }
}

fn scan_macro_for_effects(mac: &syn::Macro, set: &mut EffectSet) {
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    match name.as_str() {
        "panic" | "unreachable" | "todo" | "unimplemented" => set.add(Effect::Panics),
        "println" | "print" | "eprintln" | "eprint" | "dbg" => set.add(Effect::Io),
        "assert" | "debug_assert" | "assert_eq" | "assert_ne" => {}
        "vec" | "format" | "concat" | "stringify" => {}
        _ => set.add(Effect::UnresolvedCall {
            name: format!("{}!", name),
        }),
    }
}

fn is_io_method(name: &str) -> bool {
    matches!(
        name,
        "write"
            | "write_all"
            | "read"
            | "read_to_string"
            | "read_to_end"
            | "send"
            | "recv"
            | "lock"
            | "unlock"
            | "flush"
            | "open"
            | "close"
    )
}

fn is_known_pure_method(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "is_empty"
            | "iter"
            | "into_iter"
            | "map"
            | "filter"
            | "fold"
            | "sum"
            | "product"
            | "count"
            | "min"
            | "max"
            | "clone"
            | "as_str"
            | "as_ref"
            | "as_slice"
            | "to_string"
            | "to_owned"
            | "abs"
            | "saturating_add"
            | "saturating_sub"
            | "checked_add"
            | "checked_sub"
            | "wrapping_add"
            | "wrapping_sub"
            | "trailing_zeros"
            | "leading_zeros"
    )
}

fn is_known_pure_call(name: &str) -> bool {
    matches!(name, "min" | "max" | "abs")
}

// ---- helpers ----

fn extract_formals(item_fn: &ItemFn) -> (Vec<String>, Vec<provekit_ir_types::Sort>) {
    let mut names = Vec::new();
    let mut sorts = Vec::new();
    for input in &item_fn.sig.inputs {
        if let FnArg::Typed(pt) = input {
            let name = match &*pt.pat {
                Pat::Ident(p) => p.ident.to_string(),
                _ => "<arg>".to_string(),
            };
            names.push(name);
            sorts.push(infer_sort(&pt.ty));
        }
    }
    (names, sorts)
}

fn extract_return_sort(item_fn: &ItemFn) -> provekit_ir_types::Sort {
    match &item_fn.sig.output {
        syn::ReturnType::Default => provekit_ir_types::Sort::Primitive {
            name: "Unit".to_string(),
        },
        syn::ReturnType::Type(_, ty) => infer_sort(ty),
    }
}

fn infer_sort(ty: &syn::Type) -> provekit_ir_types::Sort {
    crate::sort_translate::syn_type_to_sort(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, const_int, var};

    fn parse_fn(src: &str) -> ItemFn {
        let file: syn::File = syn::parse_str(src).unwrap();
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) => Some(f),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn pure_function_has_empty_effects() {
        let item_fn = parse_fn(
            r#"
            fn double(x: u32) -> u32 {
                x * 2
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(
            contract.is_pure(),
            "double should be pure: {:?}",
            contract.effects
        );
        assert_eq!(contract.fn_name, "double");
        assert_eq!(contract.formals, vec!["x".to_string()]);
    }

    #[test]
    fn function_with_panic_marks_panics_effect() {
        let item_fn = parse_fn(
            r#"
            fn must_be_ten(x: u32) -> u32 {
                if x != 10 { panic!(); }
                x
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Panics));
    }

    #[test]
    fn function_with_println_marks_io_effect() {
        let item_fn = parse_fn(
            r#"
            fn loud(x: u32) -> u32 {
                println!("got {}", x);
                x + 1
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Io));
        assert!(!contract.is_pure());
    }

    #[test]
    fn function_with_unsafe_marks_unsafe_effect() {
        let item_fn = parse_fn(
            r#"
            fn raw_add(x: *const u32, y: u32) -> u32 {
                unsafe { *x + y }
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Unsafe));
    }

    #[test]
    fn function_with_mut_ref_param_marks_writes_effect() {
        let item_fn = parse_fn(
            r#"
            fn increment(buf: &mut u32) {
                *buf = *buf + 1;
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        let writes = contract
            .effects
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Writes { .. }));
        assert!(writes);
    }

    #[test]
    fn contract_cid_is_deterministic_across_runs() {
        let item_fn = parse_fn(
            r#"
            fn double(x: u32) -> u32 {
                x * 2
            }
        "#,
        );
        let a = build_function_contract(&item_fn, None);
        let b = build_function_contract(&item_fn, None);
        assert_eq!(a.cid, b.cid);
        assert_eq!(a.canonical_bytes, b.canonical_bytes);
        assert!(a.cid.starts_with("blake3-512:"));
    }

    #[test]
    fn distinct_functions_have_distinct_cids() {
        let f1 = parse_fn(r#"fn double(x: u32) -> u32 { x * 2 }"#);
        let f2 = parse_fn(r#"fn triple(x: u32) -> u32 { x * 3 }"#);
        let c1 = build_function_contract(&f1, None);
        let c2 = build_function_contract(&f2, None);
        assert_ne!(c1.cid, c2.cid);
    }

    #[test]
    fn compose_two_pure_contracts_succeeds() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y + 1 }"#), None);
        let composed = compose_function_contracts(&f, &g, 0).expect("compose succeeds");
        assert!(composed.cid.starts_with("blake3-512:"));
        let composed2 = compose_function_contracts(&f, &g, 0).unwrap();
        assert_eq!(composed.cid, composed2.cid);
    }

    #[test]
    fn compose_refuses_impure_contract() {
        let pure_f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let impure_g = build_function_contract(
            &parse_fn(
                r#"
                fn g(y: u32) -> u32 {
                    println!("{}", y);
                    y + 1
                }
            "#,
            ),
            None,
        );
        assert!(compose_function_contracts(&pure_f, &impure_g, 0).is_none());
        assert!(compose_function_contracts(&impure_g, &pure_f, 0).is_none());
    }

    #[test]
    fn compose_refuses_out_of_bounds_formal_idx() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y + 1 }"#), None);
        assert!(compose_function_contracts(&f, &g, 5).is_none());
    }

    #[test]
    fn result_var_name_is_cid_namespaced() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let name = f.result_var_name();
        assert!(name.starts_with("result__"));
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y * 3 }"#), None);
        assert_ne!(f.result_var_name(), g.result_var_name());
    }

    #[test]
    fn effects_stable_across_runs() {
        let f = parse_fn(
            r#"
            fn loud(x: u32) -> u32 {
                println!("hi");
                if x < 10 { panic!(); }
                x + 1
            }
        "#,
        );
        let a = build_function_contract(&f, None);
        let b = build_function_contract(&f, None);
        assert_eq!(a.effects, b.effects);
        assert_eq!(a.cid, b.cid);
    }

    #[test]
    fn _unused_helpers() {
        let _ = atomic_ge(var("x"), const_int(1));
    }

    // ---- Bug #384 A.1: sort-collapse regression tests ----

    #[test]
    fn u32_and_bool_formals_produce_distinct_cids() {
        let f_u32 = build_function_contract(&parse_fn(r#"fn f(x: u32) {}"#), None);
        let f_bool = build_function_contract(&parse_fn(r#"fn f(x: bool) {}"#), None);
        assert_ne!(
            f_u32.cid, f_bool.cid,
            "u32 and bool formals must produce distinct contract CIDs"
        );
        assert_ne!(f_u32.formal_sorts, f_bool.formal_sorts);
    }

    #[test]
    fn ref_lifetime_annotation_does_not_change_cid() {
        let with_lt = build_function_contract(&parse_fn(r#"fn f<'a>(s: &'a str) {}"#), None);
        let without_lt = build_function_contract(&parse_fn(r#"fn f(s: &str) {}"#), None);
        assert_eq!(
            with_lt.formal_sorts, without_lt.formal_sorts,
            "&'a str and &str must produce the same formal sort"
        );
    }

    #[test]
    fn vec_and_user_struct_formals_are_distinct() {
        let f_vec = build_function_contract(&parse_fn(r#"fn f(x: Vec<u32>) {}"#), None);
        let f_struct = build_function_contract(&parse_fn(r#"fn f(x: SomeStruct) {}"#), None);
        assert_ne!(
            f_vec.formal_sorts, f_struct.formal_sorts,
            "Vec<u32> and SomeStruct formals must produce distinct sorts"
        );
        assert_ne!(f_vec.cid, f_struct.cid);
    }

    // ---- Issue #384 B.5: opacity discharge tests ----

    struct MockPool {
        loop_cids: Vec<String>,
        try_cids: Vec<String>,
        body_fn_cids: Vec<String>,
        pin_invariant_targets: std::collections::HashMap<String, String>,
    }

    impl MockPool {
        fn empty() -> Self {
            Self {
                loop_cids: vec![],
                try_cids: vec![],
                body_fn_cids: vec![],
                pin_invariant_targets: std::collections::HashMap::new(),
            }
        }
        fn with_loop(mut self, cid: &str) -> Self {
            self.loop_cids.push(cid.to_string());
            self
        }
        fn with_try(mut self, cid: &str) -> Self {
            self.try_cids.push(cid.to_string());
            self
        }
        fn with_closure(mut self, cid: &str) -> Self {
            self.body_fn_cids.push(cid.to_string());
            self
        }
        fn with_pin_invariant(mut self, target: &str) -> Self {
            self.pin_invariant_targets
                .insert(target.to_string(), "true".to_string());
            self
        }
        fn with_pin_invariant_empty(mut self, target: &str) -> Self {
            self.pin_invariant_targets
                .insert(target.to_string(), "".to_string());
            self
        }
    }

    impl OpacityMementoLookup for MockPool {
        fn has_loop_invariant(&self, loop_cid: &str) -> bool {
            self.loop_cids.iter().any(|c| c == loop_cid)
        }
        fn has_try_branch(&self, try_cid: &str) -> bool {
            self.try_cids.iter().any(|c| c == try_cid)
        }
        fn has_closure_binding(&self, body_fn_cid: &str) -> bool {
            self.body_fn_cids.iter().any(|c| c == body_fn_cid)
        }
        fn has_drop_contract(&self, _: &str) -> bool {
            false
        }
        fn has_aliasing_memento(&self, _a: &str, _b: &str) -> bool {
            false
        }
        fn lookup_pin_invariant(
            &self,
            _function_cid: &str,
            target: &str,
        ) -> Option<PinInvariantMementoView> {
            if let Some(invariant) = self.pin_invariant_targets.get(target).cloned() {
                Some(PinInvariantMementoView {
                    function_cid: _function_cid.to_string(),
                    pinned_target: target.to_string(),
                    invariant,
                })
            } else {
                None
            }
        }
    }

    fn contract_with_effects(name: &str, effects: Vec<Effect>) -> FunctionContractMemento {
        let mut c = build_function_contract(
            &parse_fn(&format!("fn {}(x: u32) -> u32 {{ x }}", name)),
            None,
        );
        for e in effects {
            c.effects.add(e);
        }
        let val = build_memento_value(&c);
        c.canonical_bytes = jcs_bytes_of_value(&val);
        c.cid = cid_of_value(&val);
        c
    }

    #[test]
    fn opaque_loop_without_memento_blocks_composition() {
        let loop_cid = "blake3-512:aabb".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::OpaqueLoop {
                loop_cid: loop_cid.clone(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::LoopNotDischarged { .. })),
            "expected LoopNotDischarged, got {:?}",
            result
        );
    }

    #[test]
    fn opaque_loop_with_memento_allows_composition() {
        let loop_cid = "blake3-512:aabb".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::OpaqueLoop {
                loop_cid: loop_cid.clone(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty().with_loop(&loop_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}",
            result
        );
    }

    #[test]
    fn early_return_without_memento_blocks_composition() {
        let try_cid = "blake3-512:ccdd".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::EarlyReturn {
                try_cid: try_cid.clone(),
            }],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::EarlyReturnNotDischarged { .. })),
            "expected EarlyReturnNotDischarged, got {:?}",
            result
        );
    }

    #[test]
    fn early_return_with_memento_allows_composition() {
        let try_cid = "blake3-512:ccdd".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::EarlyReturn {
                try_cid: try_cid.clone(),
            }],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_try(&try_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}",
            result
        );
    }

    #[test]
    fn closure_capture_without_memento_blocks_composition() {
        let body_fn_cid = "blake3-512:eeff".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::ClosureCapture {
                body_fn_cid: body_fn_cid.clone(),
                n_captures: 1,
            }],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(
                result,
                Err(OpacityError::ClosureCaptureNotDischarged { .. })
            ),
            "expected ClosureCaptureNotDischarged, got {:?}",
            result
        );
    }

    #[test]
    fn closure_capture_with_memento_allows_composition() {
        let body_fn_cid = "blake3-512:eeff".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![Effect::ClosureCapture {
                body_fn_cid: body_fn_cid.clone(),
                n_captures: 1,
            }],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_closure(&body_fn_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}",
            result
        );
    }

    #[test]
    fn unresolved_call_always_blocks_composition() {
        let outer = contract_with_effects(
            "outer",
            vec![Effect::UnresolvedCall {
                name: "some_fn".to_string(),
            }],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(
                result,
                Err(OpacityError::UnresolvedCallNotDischarged { .. })
            ),
            "expected UnresolvedCallNotDischarged, got {:?}",
            result
        );
    }

    #[test]
    fn non_opacity_effects_still_block_after_opacity_discharge() {
        let loop_cid = "blake3-512:1122".repeat(8);
        let outer = contract_with_effects(
            "outer",
            vec![
                Effect::OpaqueLoop {
                    loop_cid: loop_cid.clone(),
                },
                Effect::Io,
            ],
        );
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_loop(&loop_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(None)),
            "expected Ok(None) when non-opacity effect blocks, got {:?}",
            result
        );
    }

    #[test]
    fn check_opacity_pure_contract_is_ok() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let pool = MockPool::empty();
        assert!(f.effects.check_opacity_effects(&pool, Some(&f.cid)).is_ok());
    }

    // ---- Issue #395: PinInvariantMemento discharge tests ----

    #[test]
    fn pinned_reference_without_memento_blocks_composition() {
        let outer = contract_with_effects(
            "outer",
            vec![Effect::PinnedReference {
                target: "pin".to_string(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::PinInvariantNotDischarged { .. })),
            "expected PinInvariantNotDischarged, got {:?}",
            result
        );
    }

    #[test]
    fn pinned_reference_with_memento_succeeds() {
        let outer = contract_with_effects(
            "outer",
            vec![Effect::PinnedReference {
                target: "pin".to_string(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty().with_pin_invariant("pin");
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        match result {
            Ok(Some(_)) => { /* success */ }
            other => panic!(
                "expected Ok(Some(_)) after PinInvariantMemento discharge, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn pinned_reference_with_wrong_target_memento_blocks() {
        let outer = contract_with_effects(
            "outer",
            vec![Effect::PinnedReference {
                target: "pin".to_string(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty().with_pin_invariant("other");
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::PinInvariantNotDischarged { .. })),
            "expected PinInvariantNotDischarged for wrong target, got {:?}",
            result
        );
    }

    #[test]
    fn pinned_reference_with_empty_invariant_blocks() {
        let outer = contract_with_effects(
            "outer",
            vec![Effect::PinnedReference {
                target: "pin".to_string(),
            }],
        );
        let inner =
            build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty().with_pin_invariant_empty("pin");
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::PinInvariantNotDischarged { .. })),
            "expected PinInvariantNotDischarged for empty invariant, got {:?}",
            result
        );
    }
}
