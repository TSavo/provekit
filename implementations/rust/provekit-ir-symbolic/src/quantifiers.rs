//! Quantifier builders + macro forms.
//!
//! Two surfaces:
//! - Function form: `forall_with(sort, |x| { body(x) })`. Direct, no macro.
//! - Macro form: `forall!(x: Int => body)` — sugar that expands to the
//!   function form with the user's identifier bound inside the body
//!   expression.
//!
//! In both forms the IR's `varName` is auto-generated as `_x0`, `_x1`, ...
//! via a thread-local counter mirroring TS `_resetCounter` in
//! `src/ir/quantifiers.ts`. The user-supplied identifier is *not* the
//! IR var name; it's only the binder in the user's source. This preserves
//! cross-language byte-equivalence with TS-emitted JSON, which uses the
//! same auto-generated names.

use std::cell::Cell;

use crate::types::{IrFormula, IrFormulaLambda, IrTerm, LambdaKind, Sort};

thread_local! {
    static COUNTER: Cell<u64> = const { Cell::new(0) };
}

/// Reset the quantifier counter. Test-only and lifter-only — call before
/// re-running the same invariant code so successive runs produce identical
/// IR (and identical CIDs).
pub fn _reset_counter() {
    COUNTER.with(|c| c.set(0));
}

fn fresh_var(sort: Sort) -> IrTerm {
    let n = COUNTER.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    IrTerm::Var {
        name: format!("_x{}", n),
        sort,
    }
}

/// Universal quantifier. `body(x)` is invoked immediately at construction
/// time with the freshly-generated bound variable; the resulting formula
/// is stored as plain data (no closures in the IR tree).
pub fn forall_with<F>(sort: Sort, body: F) -> IrFormula
where
    F: FnOnce(IrTerm) -> IrFormula,
{
    let v = fresh_var(sort.clone());
    let var_name = match &v {
        IrTerm::Var { name, .. } => name.clone(),
        _ => unreachable!(),
    };
    let inner = body(v);
    IrFormula::Forall {
        sort: sort.clone(),
        predicate: IrFormulaLambda {
            kind: LambdaKind::Lambda,
            var_name,
            sort,
            body: Box::new(inner),
        },
    }
}

/// Existential quantifier. See `forall_with` for the binding semantics.
pub fn exists_with<F>(sort: Sort, body: F) -> IrFormula
where
    F: FnOnce(IrTerm) -> IrFormula,
{
    let v = fresh_var(sort.clone());
    let var_name = match &v {
        IrTerm::Var { name, .. } => name.clone(),
        _ => unreachable!(),
    };
    let inner = body(v);
    IrFormula::Exists {
        sort: sort.clone(),
        predicate: IrFormulaLambda {
            kind: LambdaKind::Lambda,
            var_name,
            sort,
            body: Box::new(inner),
        },
    }
}

/// Bounded existential — sugar for `exists` filtered by `member`.
pub fn for_some<F>(domain: IrTerm, element_sort: Sort, body: F) -> IrFormula
where
    F: FnOnce(IrTerm) -> IrFormula,
{
    exists_with(element_sort, |v| {
        let member = IrFormula::Atomic {
            predicate: "member".to_string(),
            args: vec![v.clone(), domain],
        };
        IrFormula::And {
            conjuncts: vec![member, body(v)],
        }
    })
}

// ---------------------------------------------------------------------------
// Macro forms
// ---------------------------------------------------------------------------

/// `forall!(x: SortExpr => body)` — sweetened call to `forall_with`.
///
/// The IR var name is auto-generated (`_xN`); the user's identifier `x`
/// is bound to the IrTerm only inside `body`.
#[macro_export]
macro_rules! forall {
    ($var:ident : $sort:expr => $body:expr) => {
        $crate::quantifiers::forall_with($sort, |$var: $crate::types::IrTerm| $body)
    };
}

/// `exists!(x: SortExpr => body)` — sweetened call to `exists_with`.
#[macro_export]
macro_rules! exists {
    ($var:ident : $sort:expr => $body:expr) => {
        $crate::quantifiers::exists_with($sort, |$var: $crate::types::IrTerm| $body)
    };
}
