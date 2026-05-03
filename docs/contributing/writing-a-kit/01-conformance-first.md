# Writing a kit, step 1: conformance first

The single most counterintuitive piece of advice in this whole series: **do not start by writing IR types.** Start by wiring your kit's directory into the conformance harness with a stub that fails every fixture. Then make the fixtures pass one at a time.

Why: the harness is your test oracle. Every byte your kit emits must match the canonical bytes pinned in the fixtures. If you write code first and validate later, you spend weeks debugging mysterious byte mismatches in the canonicalizer, the envelope codec, and the bridge IR all simultaneously. If you wire the harness first and let it drive your code, each piece is forced into agreement byte-by-byte before you move on.

This is not a coding-style preference. It is a load-bearing engineering choice in a protocol where byte-equality is the contract.

## What the conformance harness does

The harness lives at the repository top level (`make conformance`). It runs three checks:

1. **Catalog conformance.** Every spec in `protocol/specs/` is content-addressed. The harness re-derives every spec's CID from the spec bytes and fails on any drift. This is independent of your kit but lives in the same gate.

2. **Per-kit fixture parity.** For each kit, the harness feeds canonical inputs (canonical IR formulas, claim envelopes, bridge declarations) and verifies the kit's emitted bytes match the canonical outputs. Fixtures live at `conformance/fixtures/`. The bar is byte-equality, not semantic equivalence.

3. **Self-contracts mint.** Each kit mints a fixed self-contracts catalog under the foundation key. The harness compares the minted CID against a pinned value. This is the strongest conformance check: a single byte of drift in any kit component causes the self-contracts CID to drift, which the harness detects.

If `make conformance` is green, your kit's bytes agree with every other kit's bytes. The thesis holds for your kit.

## The fixtures

The current fixture set covers (at minimum):

- `eq_atomic`: an atomic predicate `eq(x, 0)`. Tests primitive Term and Formula serialization.
- `pattern1_bounded_loop`: a `forall n in Int. P(n) -> Q(n)` pattern. Tests quantifier serialization.
- `contract_decl`: a Contract declaration with pre and post formulas. Tests Declaration serialization.
- `bridge_decl`: a 9-field BridgeDeclaration. Tests cross-kit bridge round-trip.

Each fixture has an input file (canonical input the kit must accept) and an output file (canonical bytes the kit must emit). Your kit reads input, canonicalizes, hashes, optionally signs, and produces output. The harness compares your output against the fixture's pinned output, byte-for-byte.

Look at how an existing kit wires this. The Go kit's conformance entry point is the simplest reference. The Python kit's is the most thoroughly commented.

## The setup

Roughly, in pseudo-code:

```
implementations/<your-language>/
├── provekit-ir/                # IR types matching the CDDL
├── provekit-canonicalizer/     # JCS + BLAKE3-512
├── provekit-claim-envelope/    # Ed25519 signing
├── provekit-proof-envelope/    # CBOR catalog
├── provekit-self-contracts/    # canonical self-contracts package
├── conformance-runner/         # entry point the harness invokes
└── README.md
```

The conformance runner is a small program that:

1. Reads a fixture name from `argv[1]`.
2. Loads the fixture's input.
3. Walks it through your kit (canonicalize → envelope → hash → emit).
4. Writes the output bytes to stdout.
5. Exits 0 on success, non-zero on error.

The harness invokes your runner once per fixture and `cmp`s the output against the canonical bytes. That's the entire integration.

## Make it fail first

Write the conformance runner as an empty stub. Wire it into `make conformance`. Run `make conformance`. Watch every fixture fail. Now you have a deterministic feedback loop: add code, run `make conformance`, see fewer failures. When all fixtures pass, you're done with this step.

This is the inverse of "write the code, then write tests." It is "wire the test harness, then write code that passes it." For byte-determinism work, only this order works.

## What "byte-equality" actually demands

A few non-obvious requirements that trip up first-time porters:

- **Number representation.** JCS specifies that numbers serialize per ECMA-262 7.1.12.1. Your language's default JSON encoder probably does not. Use a JCS-compliant encoder or write one. Trailing `.0` on integers is wrong. Scientific notation thresholds matter.
- **Key ordering.** JCS specifies UTF-8 codepoint-ordered keys. Your language's default object iteration order is probably insertion order. Sort keys explicitly before encoding.
- **String escapes.** JCS specifies the minimal escape set. Your language's default JSON encoder probably escapes more (forward slashes, non-ASCII). Use the minimal set.
- **No trailing whitespace, no pretty-printing.** Compact JSON output, no newlines, no indent.
- **Hashing the canonicalized bytes, not the deserialized object.** Always hash the bytes you emit. Hashing a re-deserialized representation is a path to non-determinism through implementation-specific details.

If your fixture output diverges by even one byte from the canonical bytes, the fixture fails. The diff is your debugger.

## When this step is done

Every fixture passes. `make conformance` for your kit is green. You have not written any lift adapter, any decorator macro, any LSP plugin, any user-facing API. You have proven that your kit's bytes match the protocol's bytes.

This is the foundation. Step 2 onward is layering on top of a substrate that is now byte-determined.

## Read next

- [02-canonicalizer.md](02-canonicalizer.md) — JCS + BLAKE3-512 in depth.
- [03-claim-envelope.md](03-claim-envelope.md) — Ed25519 signing.
- [docs/reference/conformance-fixtures.md](../../reference/conformance-fixtures.md) (when written) — every fixture's spec.
