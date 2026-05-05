// SPDX-License-Identifier: Apache-2.0
//
// WP construction and substitution. Built on top of `provekit_ir_types::IrFormula`
// so the output is directly compatible with the v1.5.0 substrate's mementos.
//
// This is the formula-side of the walk. The walk module owns the AST traversal
// and calls into here for transformations.

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

// ----- Substitution: wp(let x = e, P) = P[e/x] -----

/// Substitute `replacement` for every occurrence of `Var { name == var_name }`
/// in `formula`. This is the WP transformation rule for assignment:
/// `wp(x := e, P) = P[e/x]`.
///
/// Walks `IrFormula` and `IrTerm` recursively. Lambda binders that shadow
/// `var_name` are respected (no substitution under a shadowing binder).
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
            // Bound-variable shadowing: if the quantifier binds `var_name`,
            // do not substitute under its body.
            let body = if name == var_name {
                body
            } else {
                Box::new(substitute_in_formula(*body, var_name, replacement))
            };
            IrFormula::Forall { name, sort, body }
        }
        IrFormula::Exists { name, sort, body } => {
            let body = if name == var_name {
                body
            } else {
                Box::new(substitute_in_formula(*body, var_name, replacement))
            };
            IrFormula::Exists { name, sort, body }
        }
        IrFormula::Choice {
            var_name: bound,
            sort,
            body,
        } => {
            let body = if bound == var_name {
                body
            } else {
                Box::new(substitute_in_formula(*body, var_name, replacement))
            };
            IrFormula::Choice {
                var_name: bound,
                sort,
                body,
            }
        }
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
            // Shadowing: do not substitute under a lambda that binds `var_name`.
            let body = if param_name == var_name {
                body
            } else {
                Box::new(substitute_in_term(*body, var_name, replacement))
            };
            IrTerm::Lambda {
                param_name,
                param_sort,
                body,
            }
        }
        IrTerm::Let { bindings, body } => {
            // Sequential let: each binding sees the substitutions made before it,
            // and downstream substitution stops if any binding rebinds `var_name`.
            let mut shadowed = false;
            let bindings: Vec<_> = bindings
                .into_iter()
                .map(|b| {
                    let bound_term = if shadowed {
                        b.bound_term
                    } else {
                        substitute_in_term(b.bound_term, var_name, replacement)
                    };
                    if b.name == var_name {
                        shadowed = true;
                    }
                    provekit_ir_types::LetBinding {
                        name: b.name,
                        bound_term,
                    }
                })
                .collect();
            let body = if shadowed {
                body
            } else {
                Box::new(substitute_in_term(*body, var_name, replacement))
            };
            IrTerm::Let { bindings, body }
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
}
