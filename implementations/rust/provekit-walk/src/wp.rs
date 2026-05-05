// SPDX-License-Identifier: Apache-2.0
//
// WP construction and substitution. Built on top of `provekit_ir_types::IrFormula`
// so the output is directly compatible with the v1.5.0 substrate's mementos.
//
// This is the formula-side of the walk. The walk module owns the AST traversal
// and calls into here for transformations.

use std::collections::HashSet;

use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde_json::Value;

/// Convenience newtype for the accumulated WP at an arrival.
/// Wraps `IrFormula` to make WP-specific operations explicit at call sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wp(pub IrFormula);

impl Wp {
    pub fn into_formula(self) -> IrFormula {
        self.0
    }

    pub fn as_formula(&self) -> &IrFormula {
        &self.0
    }
}

// ----- Term constructors -----

/// `IrTerm::Var { name }`.
pub fn var(name: impl Into<String>) -> IrTerm {
    IrTerm::Var { name: name.into() }
}

/// `IrTerm::Const { value: <integer>, sort: Int }`.
/// We lean on the canonicalizer's JCS to get byte-stable serialization.
pub fn const_int(n: i64) -> IrTerm {
    IrTerm::Const {
        value: Value::Number(n.into()),
        sort: int_sort(),
    }
}

fn int_sort() -> Sort {
    Sort::Primitive {
        name: "Int".to_string(),
    }
}

// ----- Atomic predicate constructors -----

/// The trivial `true` predicate. Used as the WP at allocation sites
/// where the value is constant and the precondition trivially holds.
pub fn atomic_true() -> Wp {
    Wp(IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    })
}

/// `lhs < rhs`.
pub fn atomic_lt(lhs: IrTerm, rhs: IrTerm) -> Wp {
    Wp(IrFormula::Atomic {
        name: "<".to_string(),
        args: vec![lhs, rhs],
    })
}

/// `lhs >= rhs`. Encodes the IR vocabulary's `≥`.
pub fn atomic_ge(lhs: IrTerm, rhs: IrTerm) -> Wp {
    Wp(IrFormula::Atomic {
        name: "≥".to_string(),
        args: vec![lhs, rhs],
    })
}

// ----- Free-variable computation -----

/// Free variables of an `IrTerm` (those not bound by an enclosing
/// Lambda or Let binder). Sequential `Let` semantics: each binding's
/// bound term sees only the bindings strictly to its left.
pub fn free_vars_term(t: &IrTerm) -> HashSet<String> {
    let mut acc = HashSet::new();
    free_vars_term_into(t, &mut acc);
    acc
}

fn free_vars_term_into(t: &IrTerm, acc: &mut HashSet<String>) {
    match t {
        IrTerm::Var { name } => {
            acc.insert(name.clone());
        }
        IrTerm::Const { .. } => {}
        IrTerm::Ctor { args, .. } => {
            for a in args {
                free_vars_term_into(a, acc);
            }
        }
        IrTerm::Lambda {
            param_name, body, ..
        } => {
            let mut inner = HashSet::new();
            free_vars_term_into(body, &mut inner);
            inner.remove(param_name);
            acc.extend(inner);
        }
        IrTerm::Let { bindings, body } => {
            // Sequential semantics: bindings[i] sees names bound by bindings[0..i].
            let mut bound_so_far: HashSet<String> = HashSet::new();
            for b in bindings {
                let mut bf = HashSet::new();
                free_vars_term_into(&b.bound_term, &mut bf);
                for name in &bound_so_far {
                    bf.remove(name);
                }
                acc.extend(bf);
                bound_so_far.insert(b.name.clone());
            }
            let mut bf = HashSet::new();
            free_vars_term_into(body, &mut bf);
            for name in &bound_so_far {
                bf.remove(name);
            }
            acc.extend(bf);
        }
    }
}

/// Free variables of an `IrFormula`.
pub fn free_vars_formula(f: &IrFormula) -> HashSet<String> {
    let mut acc = HashSet::new();
    free_vars_formula_into(f, &mut acc);
    acc
}

fn free_vars_formula_into(f: &IrFormula, acc: &mut HashSet<String>) {
    match f {
        IrFormula::Atomic { args, .. } => {
            for a in args {
                free_vars_term_into(a, acc);
            }
        }
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Not { operands }
        | IrFormula::Implies { operands } => {
            for o in operands {
                free_vars_formula_into(o, acc);
            }
        }
        IrFormula::Forall { name, body, .. } | IrFormula::Exists { name, body, .. } => {
            let mut inner = HashSet::new();
            free_vars_formula_into(body, &mut inner);
            inner.remove(name);
            acc.extend(inner);
        }
        IrFormula::Choice {
            var_name, body, ..
        } => {
            let mut inner = HashSet::new();
            free_vars_formula_into(body, &mut inner);
            inner.remove(var_name);
            acc.extend(inner);
        }
    }
}

/// Pick a name not in `taken`, biased toward `base` when available.
/// Append `_1`, `_2`, … to disambiguate.
fn fresh_name(taken: &HashSet<String>, base: &str) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut n: u32 = 1;
    loop {
        let candidate = format!("{}_{}", base, n);
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

// ----- Substitution: wp(let x = e, P) = P[e/x] -----

/// Substitute `replacement` for every occurrence of `Var { name == var_name }`
/// in `formula`. This is the WP transformation rule for assignment:
/// `wp(x := e, P) = P[e/x]`.
///
/// Capture-avoiding: when entering a binder whose bound name appears free
/// in `replacement`, the binder is alpha-renamed to a fresh name before
/// the substitution proceeds. Shadowing binders (whose bound name equals
/// `var_name`) are respected — substitution stops at the binder.
pub fn substitute_in_formula(
    formula: IrFormula,
    var_name: &str,
    replacement: &IrTerm,
) -> IrFormula {
    match formula {
        IrFormula::Atomic { name, args } => IrFormula::Atomic {
            name,
            args: args
                .into_iter()
                .map(|t| substitute_in_term(t, var_name, replacement))
                .collect(),
        },
        IrFormula::And { operands } => IrFormula::And {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Or { operands } => IrFormula::Or {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Not { operands } => IrFormula::Not {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Implies { operands } => IrFormula::Implies {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Forall { name, sort, body } => {
            let (name, body) = handle_formula_binder(name, body, var_name, replacement);
            IrFormula::Forall { name, sort, body }
        }
        IrFormula::Exists { name, sort, body } => {
            let (name, body) = handle_formula_binder(name, body, var_name, replacement);
            IrFormula::Exists { name, sort, body }
        }
        IrFormula::Choice {
            var_name: bound,
            sort,
            body,
        } => {
            let (bound, body) = handle_formula_binder(bound, body, var_name, replacement);
            IrFormula::Choice {
                var_name: bound,
                sort,
                body,
            }
        }
    }
}

/// Common alpha-rename + substitute logic for `Forall`/`Exists`/`Choice`.
/// Returns the (possibly-renamed) bound name and the body with substitution
/// applied.
fn handle_formula_binder(
    bound: String,
    body: Box<IrFormula>,
    var_name: &str,
    replacement: &IrTerm,
) -> (String, Box<IrFormula>) {
    if bound == var_name {
        // Shadowing: the binder rebinds `var_name`; do not substitute under it.
        return (bound, body);
    }
    let replacement_free = free_vars_term(replacement);
    if replacement_free.contains(&bound) {
        // Capture risk: alpha-rename `bound` to a fresh name first.
        let mut taken = replacement_free;
        taken.extend(free_vars_formula(&body));
        taken.insert(var_name.to_string());
        taken.insert(bound.clone());
        let fresh = fresh_name(&taken, &bound);
        let renamed = substitute_in_formula(*body, &bound, &IrTerm::Var { name: fresh.clone() });
        let substituted = substitute_in_formula(renamed, var_name, replacement);
        (fresh, Box::new(substituted))
    } else {
        let body_subst = substitute_in_formula(*body, var_name, replacement);
        (bound, Box::new(body_subst))
    }
}

pub fn substitute_in_term(term: IrTerm, var_name: &str, replacement: &IrTerm) -> IrTerm {
    match term {
        IrTerm::Var { name } => {
            if name == var_name {
                replacement.clone()
            } else {
                IrTerm::Var { name }
            }
        }
        IrTerm::Const { value, sort } => IrTerm::Const { value, sort },
        IrTerm::Ctor { name, args } => IrTerm::Ctor {
            name,
            args: args
                .into_iter()
                .map(|t| substitute_in_term(t, var_name, replacement))
                .collect(),
        },
        IrTerm::Lambda {
            param_name,
            param_sort,
            body,
        } => {
            if param_name == var_name {
                // Shadowing: stop substitution at this binder.
                IrTerm::Lambda {
                    param_name,
                    param_sort,
                    body,
                }
            } else {
                let replacement_free = free_vars_term(replacement);
                if replacement_free.contains(&param_name) {
                    // Capture risk: alpha-rename `param_name` to fresh first.
                    let mut taken = replacement_free;
                    taken.extend(free_vars_term(&body));
                    taken.insert(var_name.to_string());
                    taken.insert(param_name.clone());
                    let fresh = fresh_name(&taken, &param_name);
                    let renamed = substitute_in_term(
                        *body,
                        &param_name,
                        &IrTerm::Var {
                            name: fresh.clone(),
                        },
                    );
                    let substituted = substitute_in_term(renamed, var_name, replacement);
                    IrTerm::Lambda {
                        param_name: fresh,
                        param_sort,
                        body: Box::new(substituted),
                    }
                } else {
                    let body = Box::new(substitute_in_term(*body, var_name, replacement));
                    IrTerm::Lambda {
                        param_name,
                        param_sort,
                        body,
                    }
                }
            }
        }
        IrTerm::Let { bindings, body } => {
            // Sequential let with capture-avoidance: each binding's bound_term
            // sees prior bindings (and the var_name → replacement substitution
            // until shadowed). When a binding's name appears free in
            // `replacement`, alpha-rename that binding's name to fresh and
            // propagate the rename to subsequent bound_terms and the body.
            let replacement_free = free_vars_term(replacement);
            let mut new_bindings: Vec<provekit_ir_types::LetBinding> =
                Vec::with_capacity(bindings.len());
            let mut shadowed = false;
            let mut prior_renames: Vec<(String, IrTerm)> = Vec::new();

            for b in bindings.into_iter() {
                // Apply prior alpha-renames first; these are pure renames and
                // capture-free by construction (fresh names were chosen to
                // avoid every name in scope).
                let mut bound_term = b.bound_term;
                for (old, new) in &prior_renames {
                    bound_term = substitute_in_term(bound_term, old, new);
                }
                let bound_term = if shadowed {
                    bound_term
                } else {
                    substitute_in_term(bound_term, var_name, replacement)
                };

                let new_name = if b.name == var_name {
                    // Shadowing: this binder rebinds `var_name`; stop downstream subst.
                    shadowed = true;
                    b.name
                } else if !shadowed && replacement_free.contains(&b.name) {
                    // Capture risk on this binder: alpha-rename to fresh.
                    let mut taken = replacement_free.clone();
                    taken.insert(var_name.to_string());
                    taken.insert(b.name.clone());
                    for existing in &new_bindings {
                        taken.insert(existing.name.clone());
                    }
                    let fresh = fresh_name(&taken, &b.name);
                    prior_renames.push((
                        b.name.clone(),
                        IrTerm::Var {
                            name: fresh.clone(),
                        },
                    ));
                    fresh
                } else {
                    b.name
                };
                new_bindings.push(provekit_ir_types::LetBinding {
                    name: new_name,
                    bound_term,
                });
            }

            let mut body = *body;
            for (old, new) in &prior_renames {
                body = substitute_in_term(body, old, new);
            }
            let body = if shadowed {
                body
            } else {
                substitute_in_term(body, var_name, replacement)
            };

            IrTerm::Let {
                bindings: new_bindings,
                body: Box::new(body),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_var_in_atomic_formula() {
        // y >= 10  with y ↦ 42  should become  42 >= 10
        let initial = atomic_ge(var("y"), const_int(10)).into_formula();
        let result = substitute_in_formula(initial, "y", &const_int(42));
        let expected = atomic_ge(const_int(42), const_int(10)).into_formula();
        assert_eq!(result, expected);
    }

    #[test]
    fn substitute_does_not_replace_nonmatching_var() {
        // x >= 10  with y ↦ 42  should remain  x >= 10
        let initial = atomic_ge(var("x"), const_int(10)).into_formula();
        let result = substitute_in_formula(initial.clone(), "y", &const_int(42));
        assert_eq!(result, initial);
    }

    #[test]
    fn substitute_respects_lambda_shadowing() {
        // ≥(λx.x, 10)  with x ↦ 42  should NOT substitute the bound x.
        let lambda = IrTerm::Lambda {
            param_name: "x".to_string(),
            param_sort: int_sort(),
            body: Box::new(var("x")),
        };
        let initial = IrFormula::Atomic {
            name: "≥".to_string(),
            args: vec![lambda.clone(), const_int(10)],
        };
        let result = substitute_in_formula(initial.clone(), "x", &const_int(42));
        assert_eq!(result, initial);
    }

    // ---- free-variable computation ----

    #[test]
    fn free_vars_term_var_returns_singleton() {
        let fv = free_vars_term(&var("x"));
        assert_eq!(fv.iter().cloned().collect::<Vec<_>>(), vec!["x".to_string()]);
    }

    #[test]
    fn free_vars_lambda_excludes_param() {
        // λx. y  has free vars {y}, not {x, y}.
        let lambda = IrTerm::Lambda {
            param_name: "x".to_string(),
            param_sort: int_sort(),
            body: Box::new(var("y")),
        };
        let fv = free_vars_term(&lambda);
        let v: Vec<_> = fv.into_iter().collect();
        assert_eq!(v, vec!["y".to_string()]);
    }

    #[test]
    fn free_vars_let_sequential() {
        // let y = z, w = y in w  has free vars {z}.
        // (y is bound by the let; w is bound; z is free in `y = z`.)
        let term = IrTerm::Let {
            bindings: vec![
                provekit_ir_types::LetBinding {
                    name: "y".to_string(),
                    bound_term: var("z"),
                },
                provekit_ir_types::LetBinding {
                    name: "w".to_string(),
                    bound_term: var("y"),
                },
            ],
            body: Box::new(var("w")),
        };
        let fv = free_vars_term(&term);
        let v: Vec<_> = fv.into_iter().collect();
        assert_eq!(v, vec!["z".to_string()]);
    }

    #[test]
    fn free_vars_forall_excludes_bound() {
        // ∀y. P(x, y)  has free vars {x}.
        let body = IrFormula::Atomic {
            name: "P".to_string(),
            args: vec![var("x"), var("y")],
        };
        let f = IrFormula::Forall {
            name: "y".to_string(),
            sort: int_sort(),
            body: Box::new(body),
        };
        let fv = free_vars_formula(&f);
        let v: Vec<_> = fv.into_iter().collect();
        assert_eq!(v, vec!["x".to_string()]);
    }

    // ---- capture-avoidance ----

    #[test]
    fn substitute_avoids_capture_in_forall() {
        // ∀y. P(x, y)  with  x ↦ y
        // Naive: ∀y. P(y, y)  -- WRONG (the substituted y is captured).
        // Capture-avoiding: ∀y_1. P(y, y_1)  -- the binder is renamed first.
        let inner = IrFormula::Atomic {
            name: "P".to_string(),
            args: vec![var("x"), var("y")],
        };
        let f = IrFormula::Forall {
            name: "y".to_string(),
            sort: int_sort(),
            body: Box::new(inner),
        };
        let result = substitute_in_formula(f, "x", &var("y"));
        match result {
            IrFormula::Forall { name, body, .. } => {
                assert_ne!(name, "y", "binder must be alpha-renamed to avoid capture");
                let body_fv = free_vars_formula(&body);
                // Free vars of the body are {y, name}: the substituted y and
                // the renamed binder.
                assert!(body_fv.contains("y"), "y from replacement is free");
                assert!(body_fv.contains(&name), "renamed binder appears in body");
            }
            other => panic!("expected Forall, got {:?}", other),
        }
    }

    #[test]
    fn substitute_avoids_capture_in_exists() {
        // ∃y. P(x, y)  with  x ↦ y  ->  ∃y_1. P(y, y_1)
        // Both refs in body: substituted y, and the renamed binder.
        let inner = IrFormula::Atomic {
            name: "P".to_string(),
            args: vec![var("x"), var("y")],
        };
        let f = IrFormula::Exists {
            name: "y".to_string(),
            sort: int_sort(),
            body: Box::new(inner),
        };
        let result = substitute_in_formula(f, "x", &var("y"));
        match result {
            IrFormula::Exists { name, body, .. } => {
                assert_ne!(name, "y");
                let body_fv = free_vars_formula(&body);
                assert!(body_fv.contains("y"));
                assert!(body_fv.contains(&name));
            }
            other => panic!("expected Exists, got {:?}", other),
        }
    }

    #[test]
    fn substitute_avoids_capture_in_lambda() {
        // λy. x  with  x ↦ y  ->  λy_1. y
        let lam = IrTerm::Lambda {
            param_name: "y".to_string(),
            param_sort: int_sort(),
            body: Box::new(var("x")),
        };
        let result = substitute_in_term(lam, "x", &var("y"));
        match result {
            IrTerm::Lambda {
                param_name, body, ..
            } => {
                assert_ne!(param_name, "y", "lambda param must be alpha-renamed");
                // Body should now be Var("y"), not Var(param_name).
                match *body {
                    IrTerm::Var { name } => assert_eq!(name, "y"),
                    other => panic!("expected Var, got {:?}", other),
                }
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn substitute_avoids_capture_in_let() {
        // let y = 1, z = x in z  with  x ↦ y
        // Naive: let y = 1, z = y in z  -- y captured.
        // Capture-avoiding: let y_1 = 1, z = y in z.
        let term = IrTerm::Let {
            bindings: vec![
                provekit_ir_types::LetBinding {
                    name: "y".to_string(),
                    bound_term: const_int(1),
                },
                provekit_ir_types::LetBinding {
                    name: "z".to_string(),
                    bound_term: var("x"),
                },
            ],
            body: Box::new(var("z")),
        };
        let result = substitute_in_term(term, "x", &var("y"));
        match result {
            IrTerm::Let { bindings, .. } => {
                assert_eq!(bindings.len(), 2);
                assert_ne!(bindings[0].name, "y", "first binding must be alpha-renamed");
                assert_eq!(bindings[1].name, "z");
                // The second binding's bound_term was `Var(x)`, post-subst is
                // `Var(y)` -- the y is the SUBSTITUTED y, not the renamed binding.
                match &bindings[1].bound_term {
                    IrTerm::Var { name } => assert_eq!(name, "y"),
                    other => panic!("expected Var(y), got {:?}", other),
                }
            }
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn substitute_no_rename_when_no_capture_risk() {
        // ∀y. P(x)  with  x ↦ 42  -> ∀y. P(42). No rename needed.
        let inner = IrFormula::Atomic {
            name: "P".to_string(),
            args: vec![var("x")],
        };
        let f = IrFormula::Forall {
            name: "y".to_string(),
            sort: int_sort(),
            body: Box::new(inner),
        };
        let result = substitute_in_formula(f, "x", &const_int(42));
        match result {
            IrFormula::Forall { name, body, .. } => {
                assert_eq!(name, "y", "no rename needed when replacement is closed");
                match *body {
                    IrFormula::Atomic { name: pred, args } => {
                        assert_eq!(pred, "P");
                        assert_eq!(args.len(), 1);
                        match &args[0] {
                            IrTerm::Const { .. } => {}
                            other => panic!("expected Const, got {:?}", other),
                        }
                    }
                    other => panic!("expected Atomic, got {:?}", other),
                }
            }
            other => panic!("expected Forall, got {:?}", other),
        }
    }

    #[test]
    fn substitute_alpha_rename_does_not_clash_with_other_free() {
        // ∀y. P(x, y, y_1)  with  x ↦ y
        // Picking `y_1` as the fresh name would clash with the existing free `y_1`.
        // The fresh-name picker must increment past `y_1` to `y_2`.
        let inner = IrFormula::Atomic {
            name: "P".to_string(),
            args: vec![var("x"), var("y"), var("y_1")],
        };
        let f = IrFormula::Forall {
            name: "y".to_string(),
            sort: int_sort(),
            body: Box::new(inner),
        };
        let result = substitute_in_formula(f, "x", &var("y"));
        match result {
            IrFormula::Forall { name, body, .. } => {
                assert_ne!(name, "y");
                assert_ne!(name, "y_1", "fresh name must not clash with free var y_1");
                // Body's free vars: {y (from replacement), y_1 (original free), name}.
                let fv = free_vars_formula(&body);
                assert!(fv.contains("y"));
                assert!(fv.contains("y_1"));
                assert!(fv.contains(&name));
            }
            other => panic!("expected Forall, got {:?}", other),
        }
    }
}
