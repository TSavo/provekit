// SPDX-License-Identifier: Apache-2.0
//
// WP construction helpers for the walk pipeline.
//
// The substitution algebra (capture-avoiding `substitute_in_formula` /
// `substitute_in_term` and free-variable computation) used to live here
// and was duplicated into `libprovekit::compose`. Per spec
// 2026-05-13-wp-as-formula.md §2.2 it now has a single canonical home in
// `libprovekit::wp`; this module re-exports those functions so the rest
// of walk's pipeline keeps importing `crate::wp::substitute_in_formula`
// unchanged. Walk's body-level WP propagation (the Dijkstra propagator
// in `walk.rs` and the loop/exception rules in `loops_and_exceptions.rs`)
// is unchanged by this PR; the broader rework of that propagator onto
// `libprovekit::wp`'s evaluator is a later step of the wp-as-formula
// migration.
//
// What stays here: the `Wp` newtype (a WP-specific wrapper around
// `IrFormula` that makes WP operations explicit at walk call sites) and
// the small term / atomic-predicate constructors walk's tests use.

use provekit_ir_types::{IrFormula, IrTerm, Sort};
use serde_json::Value;

// Re-export the canonical substitution algebra from libprovekit::wp.
pub use libprovekit::wp::{
    free_vars_formula, free_vars_term, substitute_in_formula, substitute_in_term,
};

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

/// `IrTerm::Const` carrying an IEEE-754 float constant as a raw bit
/// pattern. The bit pattern is stored as a tagged JSON object
/// `{"__float_bits__": <u64>}` so it is:
///   1. Distinguishable from a plain integer constant with the same bit
///      pattern.
///   2. Byte-deterministic under JCS (object keys are ASCII-sorted; the
///      single-key object is trivially stable).
///   3. Survives round-trip through `serde_json` without precision loss
///      (u64 fits in JSON Number exactly).
///
/// `width` is the IEEE-754 width in bits (32 or 64). The 32-bit case
/// stores the f32 bits zero-extended to u64.
///
/// ## NaN / -0.0 policy
///
/// The bit pattern is stored verbatim:
///   - +0.0 (f32) → bits = 0x00000000
///   - -0.0 (f32) → bits = 0x80000000
///   - NaN, +inf, -inf: stored as their exact bit patterns.
///
/// Downstream solvers must apply their own float theory to interpret
/// comparisons. We do NOT model NaN-inequality here.
pub fn const_float(bits: u64, width: u8) -> IrTerm {
    IrTerm::Const {
        value: serde_json::json!({"__float_bits__": bits}),
        sort: Sort::Float { width },
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
}
