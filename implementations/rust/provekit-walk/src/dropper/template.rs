// SPDX-License-Identifier: Apache-2.0

/// The policy family for the emitted drop. Each variant produces the same
/// post-state invariant on the surviving branch; they differ in how they
/// handle the alternative path.
///
/// Per paper 07 §7: the choice between templates is POLICY, not proof.
/// The substrate is grounded by the runtime check; the dropper picks
/// the family member according to user or curator policy.
///
/// Drop shapes are kit-resident per §11. This enum is the entire Rust
/// kit's drop-shape catalog for the "not_null" predicate family.
///
/// **MVP closure verification status:**
/// - `Defensive`: VERIFIED. The emitted `if {var}.is_none() { panic!(...) }` is
///   recognized by lift.rs's if-then-panic path, producing a Not formula over
///   the is_none method call. The re-lift confirms the predicate is discharged.
/// - `Recoverable`: SCAFFOLDING, NOT CLOSURE-VERIFIED.
/// - `EarlyReturn`: SCAFFOLDING, NOT CLOSURE-VERIFIED.
/// - `Expect`: SCAFFOLDING, NOT CLOSURE-VERIFIED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTemplate {
    /// Defensive: panic on violation. Surviving branch: not_null(x).
    /// Substrate edge minted: assert(x.is_some()) -> not_null(x).
    /// Shape: `if {var}.is_none() { panic!("not_null: {var} must be Some"); }`
    /// **Closure-verified for the MVP.**
    Defensive,
    /// Recoverable guard: if-let with early return. Surviving branch: not_null(x).
    /// Shape: `if {var}.is_none() { return Err(NullInput); }`
    /// **SCAFFOLDING -- not closure-verified.**
    Recoverable,
    /// Early-return shape without if-let sugar.
    /// Shape: `if {var}.is_none() { return Default::default(); }`
    /// **SCAFFOLDING -- not closure-verified.**
    EarlyReturn,
    /// Defensive with documented panic message.
    /// Shape: `let {var} = {var}.expect("invariant: caller must supply non-null {var}");`
    /// **SCAFFOLDING -- not closure-verified.**
    Expect,
}

/// Reason a `DropTemplate` cannot currently be rendered to compilable Rust.
///
/// The Defensive template is the only currently-renderable variant. The other
/// three exist in the enum for documenting the policy axis (paper 07 §7), but
/// their render paths produce uncompilable Rust without additional context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRenderable {
    /// The template family is documented but not yet implemented in a
    /// compilable form. Carries the family name for diagnostic context.
    Scaffolding {
        family: &'static str,
        reason: &'static str,
    },
}

impl std::fmt::Display for NotRenderable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotRenderable::Scaffolding { family, reason } => {
                write!(f, "DropTemplate::{family} is scaffolding only: {reason}")
            }
        }
    }
}

impl std::error::Error for NotRenderable {}
