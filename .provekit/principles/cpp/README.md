# cpp/ — C/C++ principle partition

TBD: language-specific axioms for C and C++ go here.

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"), C/C++ ships the
LARGEST per-language axiom set. The compile-time safety story is
weakest, so the runtime/SMT-side carries the most weight. Examples
that belong here when a C/C++ SAST is built:

- buffer-overflow on stack/heap arrays
- use-after-free / double-free
- uninitialized read
- dangling pointer
- integer-promotion-narrowing
- format-string injection
- NULL deref of `malloc()` result
- signed/unsigned mismatch comparison
- strict-aliasing violation
- out-of-bounds pointer arithmetic

These are NOT shipped today because the SAST extractor is TypeScript-only.
This directory is the structural slot they will land in.
