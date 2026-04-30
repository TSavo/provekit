# Cross-language DAG composition demo: divide

> One C++ library. Four language consumers. One proof DAG.

## What this demonstrates

The C++ library `divide(n, d)` has a published contract:
"divideRequiresNonZeroDenominator." That contract has a propertyHash
— `16bfe84f690ee50e` in the demo run. The hash is deterministic from
the canonical FOL form, independent of which language authored the
contract.

Four language consumers each wrap `divide` with their own guard:

| Language | Wrapper name | Surface form |
|---|---|---|
| TypeScript | `safeDivide` | `usage.ts` + `usage.invariant.ts` |
| Rust | `safe_divide` | `usage.rs.example` + `usage.invariant.rs.example` |
| Go | `SafeDivide` | `usage.go.example` + `usage.invariant.go.example` |
| C++ | `safe_divide` | `usage.cpp.example` + `usage.invariant.cpp.example` |

Each consumer's invariant says "my wrapper uphlds the precondition
d != 0 before calling divide." Each consumer mints a memento with
`inputCids` that includes the library's CID — this composes the
consumer's verification AGAINST the library's contract.

A root memento composes all four consumer mementos plus the library
contract. The DAG has the shape:

```
root (composite "all consumers safely use divide")
├── library contract H_divide
├── ts-consumer memento → [ts-precondition, H_divide]
├── rust-consumer memento → [rust-precondition, H_divide]
├── go-consumer memento → [go-precondition, H_divide]
└── cpp-consumer memento → [cpp-precondition, H_divide]
```

The library's CID appears in EVERY consumer's `inputCids`. The library
is a SHARED LEAF; the precondition proofs are PER-LANGUAGE LEAVES;
the root composes them.

## Run it

```sh
npx tsx scripts/cross-language-demo/divide/dag-demo.ts
```

Output goes to `scripts/output/cross-language-divide/`:
- `00-library-contract.json` — the C++ library's published contract memento
- `01-consumer-ts.json` — TS consumer's memento
- `01-consumer-rust.json` — Rust consumer's memento
- `01-consumer-go.json` — Go consumer's memento
- `01-consumer-cpp.json` — C++ consumer's memento
- `99-root.json` — composite root memento

All deterministic across runs (epoch timestamp; pinned producer key).

## What the hashes show

A real run produces output like:

```
ts   safeDivide      propertyHash: 3ee1b732d6e26169
rust safe_divide     propertyHash: a0761924418042cd
go   SafeDivide      propertyHash: c711e2fd426c67b7
cpp  safe_divide     propertyHash: a0761924418042cd
```

**Rust and C++ produce identical propertyHashes** — `a0761924418042cd`
in both cases. Their wrappers have the same name (`safe_divide`); the
canonical FOL form is byte-identical; the hash converges.

TS (`safeDivide`) and Go (`SafeDivide`) produce different hashes
because their wrapper names differ — different ctor names in the IR
produce different canonical bytes.

**This is the cross-language equivalence claim made operational:**
when canonical FOL structure converges, propertyHashes converge,
regardless of host language. The hashes don't care which kit produced
them; they only care about the FOL bytes.

## What's real, what's stub

**Real:**
- The TS-kit's lifter (`src/ir/lift/`) — projects type-checked TS
  predicate AST into IrFormula
- The canonicalizer (`src/canonicalizer/`) — canonical FOL → propertyHash
- The claim envelope (`src/claimEnvelope/`) — wrapper schema, CID
  construction, signature
- The producer keys (`src/producerKeys/`) — ed25519 signing
- All hashes in the demo output are real cryptographic hashes; all
  signatures round-trip through `verifyEnvelopeSignature`

**Stub (until corresponding kits ship):**
- The Rust kit's lifter — Rust's `usage.invariant.rs.example` describes
  the surface a Rust kit would consume
- The Go kit's lifter — same
- The C++ kit's lifter — same
- The library's `usage.invariant.cpp.example` — the C++ kit doesn't
  exist; the file documents the surface form

For each stub kit, this demo HAND-CONSTRUCTS the equivalent IrFormula
in TypeScript representing what that kit's lifter would produce.
That IrFormula is then canonicalized via the existing TS canonicalizer
— giving a propertyHash that, by spec, every conformant kit's
canonicalizer must produce for the same logical claim.

## Why this matters

The framework's claim is that the host language is the IR — but the IR
is universal at the canonical FOL level. Cross-language composition
happens via shared propertyHashes. The library's contract is published
once; every consumer in every language references it by hash. The
substrate handles multi-language software mechanically.

This is what makes AI-scale multi-language systems trustable. AI
generates a C++ library; publishes its contract. AI agents in TS /
Rust / Go / other C++ compose against the contract. No human ever
needs to understand the cross-language integration manually. The
DAG IS the integration.

## Spec references

- `docs/MANIFESTO.md` — outward-facing thesis
- `protocol/specs/2026-04-29-correctness-is-a-hash.md` — the architectural
  punchline (cross-language section)
- `protocol/specs/2026-04-29-implementation-fungibility.md` — the framework's
  identity is the spec, not the implementation
- `protocol/specs/2026-04-29-per-language-kit-standard.md` — what each kit
  must provide
- `protocol/specs/2026-04-29-ast-canonicalizer.md` — the byte-identical
  hash construction every kit conforms to
