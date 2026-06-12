// SPDX-License-Identifier: Apache-2.0

use libsugar::panic_freedom;
use sugar_ir_types::{IrFormula, IrTerm};

use crate::dropper::predicate::{
    formula_contains_predicate, predicate_var_arg, PredicateDescriptor,
};
use crate::dropper::template::{DropTemplate, NotRenderable};

/// NotNullPredicate implements all predicate-specific knowledge for `not_null`.
#[derive(Debug)]
pub struct NotNullPredicate;

impl PredicateDescriptor for NotNullPredicate {
    fn name(&self) -> &str {
        "not_null"
    }

    fn contains(&self, formula: &IrFormula) -> bool {
        formula_contains_predicate(formula, "not_null")
    }

    fn var_arg(&self, formula: &IrFormula) -> Option<String> {
        predicate_var_arg(formula, "not_null")
    }

    /// THE #405 FIX: returns true if `entry_wp` is
    /// `Implies([premise, conclusion])` where:
    /// - `conclusion` contains `not_null(var_name)`
    /// - `premise` contains a guard for `var_name` (`is_some(var)` or
    ///   `Not([is_none(var)])`)
    /// - CONSERVATIVE: if `not_null` appears in BOTH premise AND conclusion,
    ///   return false (emit the gap).
    fn is_premise_guarded(&self, entry_wp: &IrFormula, var_name: &str) -> bool {
        let IrFormula::Implies { operands } = entry_wp else {
            return false;
        };
        if operands.len() < 2 {
            return false;
        }
        let conclusion = &operands[operands.len() - 1];
        let premises = &operands[..operands.len() - 1];

        // Conclusion must contain not_null(var_name).
        if !formula_contains_predicate(conclusion, "not_null") {
            return false;
        }
        if predicate_var_arg(conclusion, "not_null").as_deref() != Some(var_name) {
            return false;
        }

        // Conservative: if not_null appears in any premise, emit the gap.
        if premises
            .iter()
            .any(|p| formula_contains_predicate(p, "not_null"))
        {
            return false;
        }

        // At least one premise must be a guard (is_some(var) or Not(is_none(var))).
        premises.iter().any(|p| is_guard_for(p, var_name))
    }

    fn verified_templates(&self) -> &[DropTemplate] {
        &[DropTemplate::Defensive]
    }

    fn render(&self, template: DropTemplate, var: &str) -> Result<String, NotRenderable> {
        match template {
            DropTemplate::Defensive => Ok(format!(
                "    if {var}.is_none() {{ panic!(\"not_null: {var} must be Some\"); }}\n",
                var = var
            )),
            DropTemplate::Recoverable => Err(NotRenderable::Scaffolding {
                family: "Recoverable",
                reason:
                    "render emits `return Err(NullInput)` but no error type is defined; \
                     pending caller-supplied error_expr support",
            }),
            DropTemplate::EarlyReturn => Err(NotRenderable::Scaffolding {
                family: "EarlyReturn",
                reason:
                    "render emits `return Default::default()` which requires the caller's \
                     return type to implement Default; not closure-verified by current lifter",
            }),
            DropTemplate::Expect => Ok(format!(
                "    let {var}_ok = {var}.expect(\"invariant: caller must supply non-null {var}\");\n",
                var = var
            )),
        }
    }

    fn guard_discharged(&self, formula: &IrFormula, var_name: &str) -> bool {
        formula_contains_guard_for(formula, var_name)
    }
}

/// Returns true if `formula` is a structural guard that discharges `not_null`
/// for `var_name`:
/// - `is_some(var_name)` (Atomic)
/// - `Not([is_none(var_name)])` (lift shape; `Not(is_some)` is NOT a guard)
fn is_guard_for(formula: &IrFormula, var_name: &str) -> bool {
    match formula {
        IrFormula::Atomic { name, args } => {
            name.as_str() == panic_freedom::IS_SOME && has_var(args, var_name)
        }
        IrFormula::Not { operands } => {
            if operands.len() == 1 {
                if let IrFormula::Atomic { name, args } = &operands[0] {
                    return name.as_str() == panic_freedom::IS_NONE && has_var(args, var_name);
                }
            }
            false
        }
        _ => false,
    }
}

fn has_var(args: &[IrTerm], var_name: &str) -> bool {
    args.iter().any(|t| match t {
        IrTerm::Var { name } => name == var_name,
        _ => false,
    })
}

/// Returns true if `formula` contains a `Not` node whose single operand is
/// `Atomic { name: "is_none" | "is_some", args }` with a `Var` argument
/// matching `var_name`. This is the exact shape lift.rs produces for
/// if-then-panic.
fn formula_contains_guard_for(formula: &IrFormula, var_name: &str) -> bool {
    match formula {
        IrFormula::Not { operands } => {
            if operands.len() == 1 {
                if let IrFormula::Atomic { name, args } = &operands[0] {
                    let is_guard = name.as_str() == panic_freedom::IS_NONE;
                    let has_var = args.iter().any(|t| match t {
                        IrTerm::Var { name } => name == var_name,
                        _ => false,
                    });
                    if is_guard && has_var {
                        return true;
                    }
                }
            }
            operands
                .iter()
                .any(|o| formula_contains_guard_for(o, var_name))
        }
        IrFormula::Atomic { .. } => false,
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Implies { operands } => operands
            .iter()
            .any(|o| formula_contains_guard_for(o, var_name)),
        IrFormula::Forall { body, .. } | IrFormula::Exists { body, .. } => {
            formula_contains_guard_for(body, var_name)
        }
        IrFormula::Choice { body, .. } => formula_contains_guard_for(body, var_name),
        IrFormula::DivergenceBetween { source, target } => {
            formula_contains_guard_for(source, var_name)
                || formula_contains_guard_for(target, var_name)
        }
        // Substitute and Apply are meta-level; guard detection does not descend into them.
        IrFormula::Substitute { .. } | IrFormula::Apply { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sugar_ir_types::{IrFormula, IrTerm};

    fn var(name: &str) -> IrTerm {
        IrTerm::Var { name: name.into() }
    }

    fn atom(name: &str, var_name: &str) -> IrFormula {
        IrFormula::Atomic {
            name: name.into(),
            args: vec![var(var_name)],
        }
    }

    fn not_atom(name: &str, var_name: &str) -> IrFormula {
        IrFormula::Not {
            operands: vec![atom(name, var_name)],
        }
    }

    fn implies_not_null(premise: IrFormula, var_name: &str) -> IrFormula {
        IrFormula::Implies {
            operands: vec![premise, atom("not_null", var_name)],
        }
    }

    #[test]
    fn not_null_predicate_verified_templates_returns_defensive_only() {
        let templates = NotNullPredicate.verified_templates();
        assert_eq!(
            templates.len(),
            1,
            "one verified template for not_null (Expect is scaffolding)"
        );
        assert!(templates.contains(&DropTemplate::Defensive));
        assert!(
            !templates.contains(&DropTemplate::Expect),
            "Expect is not closure-verified"
        );
    }

    #[test]
    fn defensive_template_renders_panic_shape() {
        let rendered = NotNullPredicate
            .render(DropTemplate::Defensive, "x")
            .expect("Defensive must render OK");
        assert!(rendered.contains("x.is_none()"), "must guard x");
        assert!(rendered.contains("panic!"), "must panic on violation");
        assert!(
            rendered.contains("not_null"),
            "panic msg must name invariant"
        );
    }

    #[test]
    fn not_null_kit_render_still_emits_rust_is_none() {
        let rendered = NotNullPredicate
            .render(DropTemplate::Defensive, "x")
            .expect("Defensive must render OK");

        assert!(
            rendered.contains("x.is_none()"),
            "Rust kit render must keep Rust Option syntax: {rendered}"
        );
        assert!(
            !rendered.contains("concept:panic-freedom.option.none"),
            "Rust kit render must not emit substrate concept names: {rendered}"
        );
    }

    #[test]
    fn recoverable_template_returns_not_renderable() {
        let result = NotNullPredicate.render(DropTemplate::Recoverable, "x");
        let err = result.expect_err("Recoverable must return NotRenderable");
        match err {
            NotRenderable::Scaffolding { family, .. } => {
                assert_eq!(family, "Recoverable");
            }
        }
    }

    #[test]
    fn early_return_template_returns_not_renderable() {
        let result = NotNullPredicate.render(DropTemplate::EarlyReturn, "x");
        let err = result.expect_err("EarlyReturn must return NotRenderable");
        match err {
            NotRenderable::Scaffolding { family, .. } => {
                assert_eq!(family, "EarlyReturn");
            }
        }
    }

    #[test]
    fn expect_template_renders_fresh_name_binding() {
        let rendered = NotNullPredicate
            .render(DropTemplate::Expect, "x")
            .expect("Expect must render OK with fresh-name binding (fix #407)");
        assert!(
            rendered.contains("x_ok"),
            "fresh name x_ok preserves downstream types"
        );
        assert!(rendered.contains("x.expect"), "uses Option::expect");
    }

    #[test]
    fn defensive_template_substitutes_var_name() {
        let rendered = NotNullPredicate
            .render(DropTemplate::Defensive, "my_var")
            .expect("Defensive renders OK");
        assert!(
            rendered.contains("my_var"),
            "Defensive template must contain var name 'my_var': {}",
            rendered
        );
    }

    #[test]
    fn is_premise_guarded_true_for_is_some_premise() {
        let wp = IrFormula::Implies {
            operands: vec![
                IrFormula::Atomic {
                    name: "is_some".into(),
                    args: vec![IrTerm::Var { name: "x".into() }],
                },
                IrFormula::Atomic {
                    name: "not_null".into(),
                    args: vec![IrTerm::Var { name: "x".into() }],
                },
            ],
        };
        assert!(NotNullPredicate.is_premise_guarded(&wp, "x"));
    }

    #[test]
    fn not_is_none_discharges_guard() {
        let not_is_none = not_atom(panic_freedom::IS_NONE, "x");

        assert!(is_guard_for(&not_is_none, "x"));
        assert!(NotNullPredicate.guard_discharged(&not_is_none, "x"));
        assert!(NotNullPredicate.is_premise_guarded(&implies_not_null(not_is_none, "x"), "x"));
    }

    #[test]
    fn result_predicates_do_not_discharge_not_null_guard() {
        for name in [
            panic_freedom::IS_OK,
            panic_freedom::IS_ERR,
            "concept:panic-freedom.result.ok",
            "concept:panic-freedom.result.err",
        ] {
            let premise = atom(name, "x");
            assert!(
                !is_guard_for(&premise, "x"),
                "result predicate {name} must not discharge not_null"
            );
            assert!(
                !NotNullPredicate.is_premise_guarded(&implies_not_null(premise, "x"), "x"),
                "result predicate {name} must not guard a not_null implication"
            );
        }
    }

    #[test]
    fn malformed_option_concepts_do_not_discharge_not_null_guard() {
        for name in [
            "concept:panic-freedom.option.SOME",
            "concept:panic-freedom.option.some ",
            " concept:panic-freedom.option.some",
            "method:concept:panic-freedom.option.some",
            "concept:panic-freedom.option.null",
        ] {
            let premise = atom(name, "x");
            assert!(
                !is_guard_for(&premise, "x"),
                "malformed option concept {name} must not discharge not_null"
            );
            assert!(
                !NotNullPredicate.is_premise_guarded(&implies_not_null(premise, "x"), "x"),
                "malformed option concept {name} must not guard a not_null implication"
            );
        }

        for name in [
            "concept:panic-freedom.option.NONE",
            "concept:panic-freedom.option.none ",
            " concept:panic-freedom.option.none",
            "method:concept:panic-freedom.option.none",
            "concept:panic-freedom.option.null",
        ] {
            let premise = not_atom(name, "x");
            assert!(
                !is_guard_for(&premise, "x"),
                "malformed negated option concept {name} must not discharge not_null"
            );
            assert!(
                !NotNullPredicate.guard_discharged(&premise, "x"),
                "malformed negated option concept {name} must not count as a discharged guard"
            );
        }
    }

    #[test]
    fn is_premise_guarded_false_for_plain_not_null() {
        let wp = IrFormula::Atomic {
            name: "not_null".into(),
            args: vec![IrTerm::Var { name: "x".into() }],
        };
        assert!(!NotNullPredicate.is_premise_guarded(&wp, "x"));
    }

    #[test]
    fn is_premise_guarded_conservative_when_predicate_in_both() {
        let wp = IrFormula::Implies {
            operands: vec![
                IrFormula::Atomic {
                    name: "not_null".into(),
                    args: vec![IrTerm::Var { name: "x".into() }],
                },
                IrFormula::Atomic {
                    name: "not_null".into(),
                    args: vec![IrTerm::Var { name: "x".into() }],
                },
            ],
        };
        assert!(!NotNullPredicate.is_premise_guarded(&wp, "x"));
    }

    /// Regression: `Not(is_some(x))` means `x` is None, so it does NOT
    /// discharge a not_null guard. Only `Not(is_none(x))` discharges.
    #[test]
    fn not_is_some_does_not_discharge_guard() {
        let not_is_some = IrFormula::Not {
            operands: vec![IrFormula::Atomic {
                name: "is_some".into(),
                args: vec![IrTerm::Var { name: "x".into() }],
            }],
        };
        assert!(!is_guard_for(&not_is_some, "x"));

        let wp = IrFormula::Implies {
            operands: vec![
                not_is_some.clone(),
                IrFormula::Atomic {
                    name: "not_null".into(),
                    args: vec![IrTerm::Var { name: "x".into() }],
                },
            ],
        };
        assert!(!NotNullPredicate.is_premise_guarded(&wp, "x"));
        assert!(!NotNullPredicate.guard_discharged(&not_is_some, "x"));
    }
}
