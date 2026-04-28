# rust/ — Rust principle partition

TBD: language-specific axioms for Rust go here.

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"), Rust ships the
SMALLEST per-language axiom set. The compile-time safety story (borrow
checker, exhaustive enums, lifetimes, ownership) covers most of what
other languages need runtime axioms for. Examples that may belong here
when a Rust SAST is built:

- `unwrap()` / `expect()` on `Option` / `Result` without prior validation
- `panic!` reachability in non-`#[should_panic]` contexts
- integer-overflow in release mode (where checked arithmetic is off by default)
- `unsafe` block invariants the compiler can no longer enforce

The set is intentionally small: most Rust bug classes are caught
pre-runtime by the language itself.

These are NOT shipped today because the SAST extractor is TypeScript-only.
This directory is the structural slot they will land in.
