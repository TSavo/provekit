# Porting Sugar to a new host language

This is the master guide for adding a new host language to Sugar. It walks the path from "I want to add Kotlin / Elixir / OCaml / Erlang / Crystal / Nim / V" to "the kit ships and the conformance harness is green."

The polyglot thesis is a contributor story. This document is the on-ramp.

## What you are committing to

A port comprises five pieces, in dependency order:

1. **The conformance harness contract.** Your kit's bytes must match every other kit's bytes for the same canonical formula. This is the load-bearing property; everything else is convention.
2. **The kit** (IR types, canonicalizer, claim envelope, proof envelope, self-contracts, bridge IR). The substrate every other piece sits on top of.
3. **At least one lift adapter.** A kit with no lift adapter cannot demonstrate the lift-not-author story. Pick the most idiomatic annotation library in your language ecosystem and target it first.
4. **CLI integration.** For most v1.x ports, this means "use the Rust CLI as a subprocess." A native CLI in your language is optional and can come later.
5. **LSP plugin** (optional but high-value). Without an LSP, users get no in-editor squigglies, the most visible product surface.

Total scope: multi-week project for one engineer comfortable in the language. The conformance harness gives you a deterministic bar to hit; you'll know when you're done.

## The shape of a kit

Every kit implements the same surface. The kit standard at CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88` is the authoritative spec. The shape:

- **IR types**: data structures matching the CDDL grammar in `protocol/provekit-ir.cddl`. `Term`, `Formula`, `Sort`, `Declaration`, `BridgeDeclaration`.
- **Canonicalizer**: function from `IrFormula` to canonical bytes. Implements JCS (RFC 8785) for the JSON canonical form, then BLAKE3-512 to derive the CID. Must be byte-identical to the Rust canonicalizer for every conformance fixture.
- **Claim envelope codec**: signed memento envelope (Ed25519). Serialize / deserialize / verify.
- **Proof envelope codec**: CBOR-encoded `.proof` catalog. Members map (CID → bytes), top-level signature.
- **Self-contracts package**: every kit must mint a fixed self-contracts catalog whose CID is pinned in `make conformance`. This is how the conformance harness verifies your kit's bytes match the protocol's expected bytes.
- **Bridge IR**: the v1.1.0 9-field `BridgeDeclaration` shape, round-trippable through your kit's IR types.

Of these, **the canonicalizer is the load-bearing piece**. If your canonicalizer agrees with the Rust canonicalizer byte-for-byte on every fixture, the rest is mechanical.

## Step-by-step

### Step 0: read

Before writing a line of code:

1. Read [docs/explanation/architecture.md](../explanation/architecture.md) end to end.
2. Read [docs/explanation/thesis.md](../explanation/thesis.md) end to end.
3. Read the protocol specs at [`protocol/specs/`](../../protocol/specs/). At minimum: the IR formal grammar, the proof file format, the kit standard.
4. Look at an existing kit. Recommended starts: the Go kit (clean, minimal, no decorator macros) or the Python kit (covers the full surface including the embedded verifier). Both are simple enough to read top to bottom.

### Step 1: conformance fixtures FIRST

Do not start by writing IR types. Start by setting up the conformance harness for your kit. The fixtures live at `conformance/` in the repository; they are the canonical input/output pairs. Wire your kit's directory into `make conformance` so that an empty stub fails the check.

This is counterintuitive but load-bearing. The harness is your test oracle. If you write code first and try to validate it later, you will spend weeks debugging mysterious byte mismatches. Write the harness wiring first; let it fail; make it pass one fixture at a time.

See [writing-a-kit/01-conformance-first.md](writing-a-kit/01-conformance-first.md).

### Step 2: canonicalizer

JCS (RFC 8785) for JSON canonicalization, then BLAKE3-512. Every byte matters. Trailing whitespace, key ordering, number representation, escape handling: all of it is specified, and any deviation breaks conformance.

See [writing-a-kit/02-canonicalizer.md](writing-a-kit/02-canonicalizer.md).

### Step 3: claim envelope (signed memento)

Ed25519 signature over the canonical bytes. The envelope's CID is the BLAKE3-512 of the signed bytes. Other kits must be able to verify your envelopes; your kit must be able to verify theirs.

See [writing-a-kit/03-claim-envelope.md](writing-a-kit/03-claim-envelope.md).

### Step 4: proof envelope (CBOR catalog)

The `.proof` file format. CBOR-encoded map: members (CID → bytes), top-level signature, optional `binaryCid`, optional `metadata`. See [docs/reference/proof-bundle/format.md](../reference/proof-bundle/format.md) (when written) and the proof file format spec at `protocol/specs/`.

See [writing-a-kit/04-proof-envelope.md](writing-a-kit/04-proof-envelope.md).

### Step 5: self-contracts

Every kit mints a fixed self-contracts catalog. The CID is pinned in `make conformance`. Your kit's self-contracts must hash to the pinned CID exactly, or the harness fails. See existing kits for the canonical self-contracts content.

See [writing-a-kit/05-self-contracts.md](writing-a-kit/05-self-contracts.md).

### Step 6: bridge IR (v1.1.0 9-field)

The `BridgeDeclaration` round-trip test is the hardest fixture today. Several kits ship "partial" bridge support (they pass the happy-path bytes-equality fixture but cannot construct or round-trip the full 9-field shape). To ship a full kit, this round-trip must work.

See [writing-a-kit/06-bridge-IR.md](writing-a-kit/06-bridge-IR.md).

### Step 7: at least one lift adapter

Pick the most idiomatic annotation library in your ecosystem. Don't pick the most powerful one; pick the most-used one. Coverage matters more than expressiveness. A lift that handles 80% of `@NotNull`, `@Min`, `@Max` is more valuable than a lift that handles 20% of advanced JML predicates.

See [writing-a-lift-adapter/](writing-a-lift-adapter/) for the five-step series.

### Step 8: optional but valuable (LSP plugin)

NDJSON over stdio. Five plugins ship today (Rust, Python, Zig, Ruby, C#). The protocol is small: `initialize`, `parse`, `shutdown`. If your kit reaches Step 7, the LSP is straightforward.

See [writing-an-LSP-plugin.md](writing-an-LSP-plugin.md).

## Decisions you have to make

### Authoring surface

Every language has a different idiom for "annotate this with a contract." Pick one:

- **Decorator macros / attributes**: Python `@provekit.contract`, .NET `[Provekit(...)]`, Java JSR 380 annotations.
- **Comment conventions**: Zig `//provekit:contract`, Go `//provekit:contract` (under evaluation), JML `//@ requires`.
- **Property wrappers / macros**: Swift property wrappers + Swift 5.9 macros (under evaluation).
- **Lift-only**: ship no decorator macros at all; every contract comes through a lift adapter. Good fit for languages whose ecosystem has strong existing annotation libraries.

The right choice is the one that fits your language's idiom. If your community already has `@deal.pre` and `@deal.post`, lift those; don't compete with them.

### Canonicalizer implementation

Three options:

1. **Native**: implement JCS + BLAKE3-512 in your language. Most kits do this. Pure-language implementations are easier to audit and have no FFI surface.
2. **Bind to a vendored C BLAKE3**: link against `tools/blake3-vendored/`. Lower implementation cost but adds a build dependency.
3. **Subprocess to the Rust kit**: shell out to `provekit hash`. Simplest, slowest. Acceptable for prototyping but not for shipping.

Recommended: native pure-language implementation. The Python kit's pure-Python canonicalizer is the reference pattern for "small, auditable, byte-identical."

### Hashing path

BLAKE3-512 with all SIMD paths disabled produces deterministic bytes. Make sure your BLAKE3 binding produces the same bytes as the vendored C implementation in portable mode. Conformance fixtures will catch any deviation.

## What you do NOT have to ship in v1.x

- Native CLI (use the Rust CLI as a subprocess).
- Embedded verifier (optional; can come later).
- LSP plugin (optional but high-value).
- All lift adapters in your ecosystem (one is enough to ship; more later).
- A pure-language Z3 binding (the verifier shells out to Z3 anyway).
- Decorator macros (lift-only kits are valid).

## When you're done

The conformance harness is green. `make conformance` includes your kit, mints your self-contracts catalog, and the CID matches the pinned value. At minimum one lift adapter is shipping. The kit is documented in [docs/reference/per-language-status.md](../reference/per-language-status.md) and a tutorial exists at `docs/tutorials/<language>.md`.

You don't need our blessing. The harness is the gate. Open the PR.

## What to read next

- [writing-a-kit/01-conformance-first.md](writing-a-kit/01-conformance-first.md): start here.
- [docs/reference/kit-standard.md](../reference/kit-standard.md) (when written): the authoritative spec.
- [`protocol/specs/`](../../protocol/specs/): the protocol's content-addressed source of truth.
