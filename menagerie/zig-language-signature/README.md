# Zig Language Signature Memento

This draft catalog describes the operation algebra emitted by `provekit-lift-zig-source`.

Version: `0.1.0-draft`

The lifter emits AST-level Zig source terms only. It does not run Zig Sema or evaluate comptime code. Unsupported syntax is refused with a structured `Refusal` instead of being mapped to a catch-all operation.

## Core Operations

The current source-lift subset includes statement sequencing, declarations, assignments, returns, conditionals, loops, calls, integer and boolean operators, field/index/pointer operations, `zig:source-unit`, and panic/unreachable markers. All operation CIDs are language-prefixed through `zig:` names.

Short-circuit `zig:and` and `zig:or` mark their right slot as `evaluation: "unevaluated"`. `zig:source-unit(bytes, operational_term)` preserves the original source bytes while exposing the lifted operational term.

## Effects

The wire effects emitted by the lifter match the canonical `Effect` shapes used by `provekit-walk`: `reads`, `writes`, `io`, `unsafe`, `panics`, `unresolved_call`, and `opaque_loop`. Effects are sorted by the canonical rank order before JSON emission.
