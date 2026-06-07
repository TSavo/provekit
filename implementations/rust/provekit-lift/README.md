# provekit-lift

The canonical adoption path for ProvekIt.

## What this is

`provekit-lift` walks an existing Rust workspace, finds annotations from
established testing/specification libraries (`proptest`, `contracts`,
and more to come), translates each one to canonical IR, mints a signed
content-addressed contract memento, and bundles the result into a single
`.proof` catalog file.

You do not rewrite anything. Your existing tests stay where they are.
`provekit-lift` reads what you already have and promotes it.

## Strategic positioning

ProvekIt does **not** compete with existing annotation libraries. It
sits **beneath** them.

```
┌──────────────────────────────────────────────────────────────┐
│  Your existing code                                          │
│                                                              │
│  proptest! { #[test] fn ... }    #[contracts::requires(...)] │
│  ▲                               ▲                           │
│  │                               │                           │
│  │  walks AST              walks AST                         │
│  │                               │                           │
│  └────────── provekit-lift ──────┘                           │
│                   │                                          │
│                   ▼                                          │
│         <cid>.proof  (signed, content-addressed)             │
└──────────────────────────────────────────────────────────────┘
```

Future adapters drop in the same way: `kani`, `prusti`,
`hypothesis-py`, `deal-py`, `bean-validation-java`, `zod-ts`.

`provekit-macros` is the **fallback** for greenfield code where the
developer doesn't already have an annotation library. The recommended
adoption order is: lift first, mint-via-macros only when greenfield.

## Usage

```bash
# As a Cargo subcommand (preferred):
cargo provekit-lift --workspace . --target-dir target/release

# As a direct binary:
provekit-lift --workspace . --target-dir target/release
```

Both forms walk the directory tree, run every registered adapter on
every `.rs` file, mint each lifted contract, and write a single
`<cid>.proof` to the output directory. The CID is printed on stdout.

The output `.proof` loads cleanly through `provekit verify`.

## What gets lifted

### proptest

```rust
proptest! {
    #[test]
    fn nonneg(x: i64) {
        prop_assert!(x >= 0);
    }
}
```

becomes a contract named `nonneg` with `inv = forall x:Int. x >= 0`.

### contracts

```rust
#[requires(x > 0)]
#[ensures(ret >= 0)]
fn sqrt(x: i64) -> i64 { /* ... */ }
```

becomes a contract named `sqrt` with `pre = forall x:Int. x > 0` and
`post = forall x:Int. ret >= 0`.

## Limitations (v0)

The translator deliberately does not translate every Rust expression.
It accepts:

- **Predicates**: a binary comparison (`>`, `>=`, `<`, `<=`, `==`,
  `!=`).
- **Operands**: an identifier, an integer or string literal, or a
  single-argument function call (treated as a constructor in the IR).

Anything else (method calls, field access, indexing, multi-argument
function calls, complex nesting) is **skipped with a warning**. The
test still runs operationally; it just isn't lifted to a contract.

This is by design. Polluting the lattice with un-translatable atoms is
worse than under-coverage. When the kit IR grows the predicates these
shapes need (string `len`, container `contains`, etc.), the adapters
add them.

## Lift layers (Layer 0, Layer 2, Layer 3)

Adapters dispatch in three progressively richer passes. A test that
the cheaper layer cannot reach falls through to the next.

### Layer 0: mechanical pattern matching

Walks the AST for shapes the translator can lift in a single pass with
zero search. For Rust unit tests:
- `assert_eq!(<lhs>, <rhs>)`, `assert_ne!`, `assert!(<binop>)`,
  `assert_matches!` against a literal-leaf pattern.
- Each side must be an identifier, integer/string literal, or
  single-arg ctor call.

Anything outside the whitelist skips with a structured warning. Each
matched assertion mints its own contract memento named
`<test>::<index>`.

### Layer 2: structural lift

Three patterns Layer 0 cannot reach but a structural recognizer can.
Layer 2 runs FIRST; tests it claims are excluded from Layer 0 so we
never double-count.

**Pattern 1: bounded loop as universal quantifier.**

```rust
#[test]
fn squares_are_nonneg() {
    for x in 0..100 {
        assert!(x >= 0);
    }
}
```

Lifts to `forall x:Int. (0 <= x AND x < 100) implies (x >= 0)`. The
loop variable name is preserved (not renamed to `_xN`) so the
canonical IR is stable across runs. Range endpoints accept literal
integers and bare identifiers; `RangeFrom` / `RangeTo` / `RangeFull`
skip with a warning. Nested for-loops are deferred to Layer 2.5.

**Pattern 2: helper-function inlining.**

```rust
fn assert_is_42(x: i64) {
    assert_eq!(x, 42);
}

#[test]
fn many_42s() {
    assert_is_42(42);
    assert_is_42(42);
}
```

Lifts to one contract memento per call site, named
`<test>::call::<i>`. The helper's assertion is lifted with the formal
parameter substituted by the literal argument. Helpers must be
single-parameter, single-statement, single-assertion functions defined
in the same source file. Multi-arg helpers, side-effecting helpers,
and cross-crate helpers skip.

**Pattern 3: multi-assertion characterization conjunction.**

```rust
#[test]
fn parse_int_characterization() {
    assert_eq!(parse_int("0"), 0);
    assert_eq!(parse_int("42"), 42);
    assert_ne!(parse_int("99"), 0);
}
```

Lifts to one contract memento named `<test>` whose body is
`and(...)` of every liftable assertion. Triggered only when every
top-level statement in the body is a recognized assertion AND there
are at least two of them. If only one atom is liftable, the claim is
released so Layer 0 can fall back.

### Layer 3: LLM-assisted lift (future work)

Tests that neither Layer 0 nor Layer 2 claims fall through to a
(planned) LLM-assisted lift. The skip log surfaces what it would have
to handle: method-call chains, multi-statement loop bodies, nested
quantifiers, characterization conjunctions whose atoms exceed the v0
operand whitelist.

## Content-addressed dedup

If two source files express the same property, both lift to the same
canonical IR, hash to the same CID, and collapse to one minted member.
Two functions with the same contract name but different IR fail loud:
the lattice should never silently accept contradiction.

## Architecture

```
provekit-lift              library + two binaries
├── adapters
│   ├── proptest           provekit-lift-proptest crate
│   └── contracts          provekit-lift-contracts crate
└── core                   walk, parse, dispatch, mint, bundle
```

Each adapter is its own crate with its own `lift_file(syn::File,
&str) -> AdapterOutput`. Adding a new adapter is: write a crate, add
it to `provekit-lift/Cargo.toml`, call its `lift_file` from
`lib.rs::lift_path`.
