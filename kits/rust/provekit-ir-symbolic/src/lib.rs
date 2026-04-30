//! # provekit-ir-symbolic
//!
//! Rust kit's IR-emission surface — the runtime-eval lifting model from
//! `src/ir/symbolic/` (TS reference) ported to Rust idiom.
//!
//! Users authoring `.invariant.rs` files import primitives from this
//! crate, write invariant code using them, and *running* the code
//! produces the IR. No syn/quote AST walking. Just function calls and
//! macro forms.
//!
//! Cross-language equivalence: the IR data structures' serde-JSON shape
//! is byte-equivalent to the TS kit's IrFormula JSON for the same
//! logical claim. That equivalence is the substrate for propertyHash
//! agreement across host languages.
//!
//! ## Example
//!
//! ```ignore
//! use provekit_ir_symbolic::{
//!     property::{begin_collecting, BridgeSpec},
//!     prelude::*,
//! };
//! use provekit_ir_symbolic::{must, describe, exists, forall};
//!
//! let handle = begin_collecting();
//!
//! describe!("parseInt", {
//!     must!("can return zero",
//!         exists!(s: sorts::string() => eq(parse_int(s), num(0_i64))));
//!
//!     must!("preserves int",
//!         forall!(n: sorts::int() =>
//!             implies(gte(n.clone(), num(0_i64)),
//!                     eq(parse_int(str_("0")), num(0_i64)))));
//! });
//!
//! let decls = handle.finish();
//! assert_eq!(decls.len(), 2);
//! ```

pub mod canonicalize;
pub mod connectives;
pub mod primitives;
pub mod property;
pub mod quantifiers;
pub mod types;

// ---------------------------------------------------------------------------
// Re-exports — public surface mirroring `src/ir/symbolic/index.ts`
// ---------------------------------------------------------------------------

// Types
pub use types::{lift_to_term, sorts, BindingScope, IrFormula, IrFormulaLambda, IrTerm, LambdaKind, Liftable, Sort};

// Constants
pub use primitives::{bool_, num, real, str_};

// Built-in function primitives
pub use primitives::{
    abs, array_includes, array_length, ceil, floor, is_finite, is_integer, is_nan, max, min,
    parse_float, parse_int, sign, sqrt, string_includes, string_length,
};

// Term arithmetic
pub use primitives::{add, div, mul, neg, sub};

// Atomic predicates
pub use primitives::{eq, gt, gte, is_false, is_true, lt, lte, neq};

// Connectives
pub use connectives::{and, iff, implies, not, or};

// Quantifier function forms (macros are exported via #[macro_export] from
// quantifiers.rs and property.rs; they live at the crate root namespace).
pub use quantifiers::{exists_with, for_some, forall_with};

// Collector + obligations
pub use property::{
    begin_collecting, bridge, describe, describe_skip, must, must_skip, property as property_fn,
    BridgeSpec, Declaration, FinishHandle,
};

/// Convenience prelude — common imports for invariant files.
pub mod prelude {
    pub use crate::connectives::{and, iff, implies, not, or};
    pub use crate::primitives::{
        abs, add, array_includes, array_length, bool_, ceil, div, eq, floor, gt, gte, is_false,
        is_finite, is_integer, is_nan, is_true, lt, lte, max, min, mul, neg, neq, num, parse_float,
        parse_int, real, sign, sqrt, str_, string_includes, string_length, sub,
    };
    pub use crate::quantifiers::{exists_with, for_some, forall_with};
    pub use crate::types::{sorts, IrFormula, IrTerm, Sort};
}
