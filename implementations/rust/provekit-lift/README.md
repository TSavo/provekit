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
