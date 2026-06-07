// SPDX-License-Identifier: Apache-2.0
//
// dropper/mod.rs -- generative completion for the Rust kit (paper 07 §7).
//
// The dropper closes the lifter's loop. See individual sub-modules for details
// on each concern.
//
// Phase 1: gap detection -- identify arrivals where the WP contains an
//   undischarged leaf predicate (gap.rs).
// Phase 2: cached-witness lookup -- template families per predicate (predicates/).
// Phase 3: source emission -- render and splice into source (emit.rs).
// Phase 4 (deferred, #382 follow-up): mint-on-miss via solver portfolio.

pub mod emit;
pub mod error;
pub mod gap;
pub mod predicate;
pub mod predicates;
pub mod template;
pub mod verify;

pub use emit::{emit_drop, EmitResult};
pub use error::DropFailure;
pub use gap::{detect_gaps, Gap};
pub use predicate::{
    formula_contains_predicate, predicate_var_arg, PredicateDescriptor, PredicateRegistry,
};
pub use predicates::not_null::NotNullPredicate;
pub use template::{DropTemplate, NotRenderable};
pub use verify::verify_closure;

use crate::walk::walk_callsites_to_entry;
use crate::wp::Wp;

/// Run all four steps of the dropper end-to-end: detect, lookup, emit, **verify**.
///
/// This is the main entry point for the dropper. Per paper 07 §7, an emission
/// that does not actually close the gap is not generative completion. This
/// function therefore calls `verify_closure` after `emit_drop` and only
/// returns `Ok(EmitResult)` when re-lift structurally confirms the gap is
/// discharged.
///
/// Parameters:
/// - `source`: the Rust source text containing both the callee and caller.
/// - `callee_name`: the function whose precondition has a gap.
/// - `caller_name`: the function calling `callee_name` where the gap arises.
/// - `callee_formal_params`: formal parameter names for the callee.
/// - `callee_precondition`: the WP representing the callee's precondition.
/// - `descriptor`: the predicate descriptor (e.g. `&NotNullPredicate`).
/// - `template`: which drop template to use.
pub fn drop_gap(
    source: &str,
    callee_name: &str,
    caller_name: &str,
    callee_formal_params: &[String],
    callee_precondition: Wp,
    descriptor: &dyn PredicateDescriptor,
    template: DropTemplate,
) -> Result<EmitResult, DropFailure> {
    let file: syn::File = syn::parse_str(source).map_err(|_| DropFailure::SourceParseFailed)?;

    let caller_fn = file
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Fn(f) = item {
                if f.sig.ident == caller_name {
                    return Some(f.clone());
                }
            }
            None
        })
        .ok_or_else(|| DropFailure::CallerNotFound {
            caller_name: caller_name.to_string(),
        })?;

    // Phase 1: detect gaps.
    let walks = walk_callsites_to_entry(
        &caller_fn,
        callee_name,
        callee_formal_params,
        callee_precondition.clone(),
    );
    let gaps = detect_gaps(&walks, descriptor);
    let gap = gaps
        .into_iter()
        .next()
        .ok_or_else(|| DropFailure::NoGapDetected {
            predicate: descriptor.name().to_string(),
        })?;

    // Phase 2: validate against descriptor's verified templates.
    let candidates = descriptor.verified_templates();
    if candidates.is_empty() {
        return Err(DropFailure::UnknownPredicate {
            predicate: descriptor.name().to_string(),
        });
    }
    if !candidates.contains(&template) {
        return Err(DropFailure::TemplateNotCandidate {
            predicate: descriptor.name().to_string(),
            requested: template,
        });
    }

    // Pre-render check: surface NotRenderable as a structured error.
    descriptor
        .render(template, &gap.var_name)
        .map_err(DropFailure::NotRenderable)?;

    // Phase 3: emit.
    let emit = emit_drop(source, &gap, template, descriptor).ok_or(DropFailure::EmitFailed)?;

    // Phase 4 (verification): re-lift and confirm closure.
    if !verify_closure(
        &emit.modified_source,
        &gap,
        callee_formal_params,
        callee_precondition,
        descriptor,
    ) {
        return Err(DropFailure::ClosureVerificationFailed { emit });
    }

    Ok(emit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, const_int, var, Wp};
    use sugar_ir_types::{IrFormula, IrTerm};

    fn not_null_wp(var_name: &str) -> Wp {
        Wp(IrFormula::Atomic {
            name: "not_null".to_string(),
            args: vec![IrTerm::Var {
                name: var_name.to_string(),
            }],
        })
    }

    const FIXTURE_SRC: &str = r#"
fn f(x: Option<i32>) -> i32 {
    x.unwrap()
}

fn caller(x: Option<i32>) {
    f(x);
}
"#;

    #[test]
    fn end_to_end_drop_gap_defensive() {
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            not_null_wp("x"),
            &NotNullPredicate,
            DropTemplate::Defensive,
        );
        let emit = result.expect("drop_gap must succeed for not_null fixture");
        assert_eq!(emit.template, DropTemplate::Defensive);
        assert_eq!(emit.var_name, "x");
        let parse_result: Result<syn::File, _> = syn::parse_str(&emit.modified_source);
        assert!(
            parse_result.is_ok(),
            "emitted source parses: {:?}",
            parse_result.err()
        );
        let guard_pos = emit
            .modified_source
            .find("x.is_none()")
            .expect("guard present");
        let callsite_pos = emit.modified_source.find("f(x)").expect("callsite present");
        assert!(guard_pos < callsite_pos, "guard before callsite");
    }

    #[test]
    fn drop_gap_returns_template_not_candidate_for_scaffolding() {
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            not_null_wp("x"),
            &NotNullPredicate,
            DropTemplate::Recoverable,
        );
        match result {
            Err(DropFailure::TemplateNotCandidate { requested, .. }) => {
                assert_eq!(requested, DropTemplate::Recoverable);
            }
            other => panic!("expected TemplateNotCandidate, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_no_gap_detected_when_predicate_absent() {
        // Precondition is x >= 10, not not_null(x) — NotNullPredicate finds no gap.
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            atomic_ge(var("x"), const_int(10)),
            &NotNullPredicate,
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::NoGapDetected { predicate }) => {
                assert_eq!(predicate, "not_null");
            }
            other => panic!("expected NoGapDetected, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_unknown_predicate_when_descriptor_has_no_templates() {
        // A descriptor with no verified templates produces UnknownPredicate.
        #[derive(Debug)]
        struct EmptyDescriptor;
        impl PredicateDescriptor for EmptyDescriptor {
            fn name(&self) -> &str {
                "unknown_pred"
            }
            fn contains(&self, formula: &IrFormula) -> bool {
                formula_contains_predicate(formula, "unknown_pred")
            }
            fn var_arg(&self, formula: &IrFormula) -> Option<String> {
                predicate_var_arg(formula, "unknown_pred")
            }
            fn is_premise_guarded(&self, _: &IrFormula, _: &str) -> bool {
                false
            }
            fn verified_templates(&self) -> &[DropTemplate] {
                &[]
            }
            fn render(&self, _: DropTemplate, _: &str) -> Result<String, NotRenderable> {
                Err(NotRenderable::Scaffolding {
                    family: "EmptyDescriptor",
                    reason: "test",
                })
            }
            fn guard_discharged(&self, _: &IrFormula, _: &str) -> bool {
                false
            }
        }

        let unknown_pred_wp = Wp(IrFormula::Atomic {
            name: "unknown_pred".to_string(),
            args: vec![IrTerm::Var {
                name: "x".to_string(),
            }],
        });
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            unknown_pred_wp,
            &EmptyDescriptor,
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::UnknownPredicate { predicate }) => {
                assert_eq!(predicate, "unknown_pred");
            }
            other => panic!("expected UnknownPredicate, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_caller_not_found_for_missing_caller() {
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "nonexistent_caller",
            &["x".to_string()],
            not_null_wp("x"),
            &NotNullPredicate,
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::CallerNotFound { caller_name }) => {
                assert_eq!(caller_name, "nonexistent_caller");
            }
            other => panic!("expected CallerNotFound, got {:?}", other),
        }
    }
}
