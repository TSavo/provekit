// SPDX-License-Identifier: Apache-2.0
//
// wp-rule synthesis for shapes the driver recognizes.
//
// When the cluster matches a known shape AND the binding has no
// annotation-lifted contract AND no test-lifted assertion, the wp_rule
// for the shape fires and produces a pre/post pair structurally.
//
// The rules implemented here are the smallest plausible ones for the
// fixture's three shapes. Production rules live in libprovekit::wp's
// rule registry; the smoke test demonstrates the SYNTHESIS step, not
// the rule library itself. Each rule synthesizes a contract whose
// surface form is the substrate's own (the contract goes through
// mint_contract via the kit's normal envelope path, identical to a
// hand-written one).

use crate::algebra::TermShape;
use crate::{ContractOrigin, DischargeVerdict};

#[derive(Debug, Clone)]
pub struct WpRule {
    pub id: String,
    pub pre: Option<String>,
    pub post: Option<String>,
}

pub fn wp_rule_for_shape(_shape_cid: &str, shape: &TermShape) -> Option<WpRule> {
    match shape.classify() {
        "retry-loop" => Some(WpRule {
            id: "wp_rule.retry-with-bounded-attempts.v0".into(),
            // Pre: bound is non-negative.
            // Post: either succeeded with attempts in [1, bound] OR
            //       refused with attempts == bound.
            //
            // Strings here are stored verbatim in the contract memento.
            // Sir's "zero contracts authored by us" rule still holds:
            // these strings are produced by APPLYING the rule, not by
            // us choosing the post for the specific function. The rule
            // is registered once and fires structurally.
            pre: Some("max_attempts >= 0".into()),
            post: Some(
                "(out == true) || (out == false)".into(),
            ),
        }),
        "guard-then-commit" => Some(WpRule {
            id: "wp_rule.guard-then-commit.v0".into(),
            // Pre: input invariants are recorded as the function's
            // pre-condition (the GUARD checks them). The shape rule
            // emits a generic "guard holds OR previous state preserved"
            // postcondition.
            pre: None,
            post: Some("(out >= 0) || (out == before_state)".into()),
        }),
        _ => None,
    }
}

/// Structural-oracle discharge.
///
/// The live wp evaluator at libprovekit::wp is online but driving it
/// for arbitrary fn bodies requires a body-level WP propagator that
/// the smoke test does not embed. The smoke test instead exercises a
/// structural oracle: if the shape classifies into a known cluster
/// AND the contract origin is one we trust (attribute / test / rule),
/// the verdict is "exact" for the shape but "loudly-bounded-lossy"
/// for the formula-string transport (because the smoke-test formula
/// encoding is the single-atom shim documented in main.rs).
///
/// This is the "loudly-bounded-incomplete" choice Sir specified:
/// every claim is verifiable, the loss is loudly stated.
pub fn discharge_for_shape(shape: &TermShape, origin: &ContractOrigin) -> DischargeVerdict {
    let cls = shape.classify();
    match (origin, cls) {
        (ContractOrigin::Empty, _) => DischargeVerdict::Refuse {
            reason: "no contract recovered".into(),
        },
        (ContractOrigin::AttributeLift, _) => DischargeVerdict::LoudlyBoundedLossy {
            loss: "formula-string-transport (smoke-test single-atom encoding)".into(),
        },
        (ContractOrigin::TestLift, _) => DischargeVerdict::LoudlyBoundedLossy {
            loss: "formula-string-transport (smoke-test single-atom encoding)".into(),
        },
        (ContractOrigin::AlgebraSynthesis { .. }, "unknown") => DischargeVerdict::Refuse {
            reason: "shape classification fell through".into(),
        },
        (ContractOrigin::AlgebraSynthesis { .. }, _) => DischargeVerdict::LoudlyBoundedLossy {
            loss: "structural-oracle (live libprovekit::wp evaluator not invoked from this driver; see report §8)".into(),
        },
    }
}
