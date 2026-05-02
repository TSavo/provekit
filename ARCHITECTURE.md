# ProvekIt Architecture

A walk-through of the protocol's mechanics in roughly fifteen minutes. This document describes the v1.1.0 protocol catalog at CID `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`. Every spec referenced here is itself content-addressed; CIDs are quoted where authoritativeness matters.

## The architecture is a pipeline

ProvekIt is not a library you call. It is a pipeline you run. Data flows in one direction: CDDL → Codegen → Types → Compilers → Proof Bundle → Verification.

### Stage 1: CDDL (the single source of truth)

`protocol/provekit-ir.cddl` is the root of the entire protocol. It defines:

- `IrTerm` — variables, constants, constructors, lambdas, let-bindings
- `IrFormula` — atomic predicates, connectives (and/or/not/implies), quantifiers (forall/exists), choice
- `Sort` — primitive types (Int, String, Bool)
- `Declaration` — contracts, bridges, evidence
- `ProofBundle` — the catalog format (name, version, binaryCid, metadata, members)

The grammar is machine-readable by the `cddl` crate. The spec is human-readable in `protocol/specs/2026-04-30-ir-formal-grammar.md`. Both are content-addressed.

### Stage 2: Codegen (types and compilers from the grammar)

`provekit-ir-codegen` reads the CDDL and emits:

1. **`provekit-ir-types/src/lib.rs`** — serde types matching the spec exactly. `Term`, `Formula`, `Sort`, `Declaration`, `BridgeDeclaration`, `EvidenceTerm`.
2. **`provekit-ir-compiler-smt-lib/src/generated.rs`** — SMT-LIB emitter: `emit_term`, `emit_formula`, `emit_sort`.
3. **`provekit-ir-compiler-coq/src/generated.rs`** — Coq emitter: `emit_term`, `emit_formula`, `sort_to_coq`.

The generated code is never hand-edited. Change the CDDL, run `cargo run -p provekit-ir-codegen`, all three files update automatically. The compilers use the generated types, so they always match the spec.

### Stage 3: Authoring (the developer's surface)

`provekit-ir-symbolic` provides the authoring kit: `forall`, `exists`, `atomic`, `gte`, `eq`, etc. It also provides conversions to/from the generated types, so a developer can:

```rust
use provekit_ir_symbolic::{forall, atomic, gte, var, const_};

let f = forall("n", "Int", atomic("parseInt", vec![var("s")], gte(const_(0))));
```

The `#[provekit::contract]` and `#[provekit::verify]` macros lift source-level annotations to IR formulas.

The `#[provekit::implement(target = "bafy...")]` macro binds a function to a contract by CID:

```rust
#[provekit::implement(target = "bafy...js-parseInt-v24")]
fn my_parse_int(s: &str) -> i64 {
    s.parse().unwrap_or(0)
}
```

This registers: "`my_parse_int` is an implementation of contract `bafy...js-parseInt-v24`." The lifter resolves the function call to that contract CID.

### Stage 4: Proof bundle (the distribution artifact)

The `.proof` file IS the package. It replaces `package.json`, `Cargo.toml`, `go.mod` as the primary distribution artifact for verified software.

```cddl
ProofBundle = {
  kind: "catalog",
  name: "@types/node-v24",
  version: "24.3.0",
  ? binaryCid: "bafy...node-v24-v8-snapshot",
  metadata: {
    "bytecode-cid": "bafy...evm-module",
    "vm": "evm-cancun",
    "entry-point": "parseInt"
  },
  members: {
    "bafy...contract-1": <contract memento bytes>,
    "bafy...bridge-1": <bridge memento bytes>,
    "bafy...evidence-1": <evidence memento bytes>
  },
  signature: <ed25519 signature>
}
```

Key fields:
- `binaryCid` — pins the compiled artifact. Change any bit, the CID changes, the proof fails.
- `metadata` — freeform, decorative, signed but non-normative. For tooling, diagnostics, human review.
- `members` — map from CID to canonical bytes. Every memento is content-addressed and recursively verifiable.

### Stage 5: Verification (the build-time gate)

The build script (or `provekit prove`) walks the DAG:

1. Load all `.proof` bundles from dependencies
2. Index contracts by CID, bridges by source symbol
3. For each call site, resolve the function to its contract CID
4. Walk bridges transitively: `my_fn → js-parseInt-v24 → ref-parseInt-v1`
5. At each hop, verify the implication: does source contract imply target contract?
6. If all hops verify, the call site is discharged
7. Mint a witnessed proof memento: "body of `my_fn` implies contract X, here's the Z3 model"

Three tiers, in order:

**Tier 1: hash equality.** The publisher's post-hash equals the consumer's pre-hash. `memcmp` returns zero. Discharged for free.

**Tier 2: cached implication.** A signed implication memento exists. Check Ed25519 signature once. Discharge every call site sharing this pair.

**Tier 3: solver fallback.** Z3 invoked once per genuinely-novel pair. On `unsat`, mint a fresh implication memento. Next verifier hits Tier 2.

## The handshake is DAG walking

Traditional verification asks: "does post imply pre?"

ProvekIt asks: "is there a path in the DAG from this contract to that contract where every edge is a verified implication?"

The DAG is the proof. Every node is a contract. Every edge is a bridge. The path from `my_fn` to `ref-parseInt-v1` is:

```
my_fn (CID A)
  → bridge A → js-parseInt-v24 (CID B)
    → bridge B → ref-parseInt-v1 (CID C)
```

The verifier checks:
1. Bridge A is valid (signed, source contract matches, target contract resolves)
2. Bridge B is valid (same)
3. The transitive composition holds (A implies B implies C)

If all checks pass, the proof transfers. The JavaScript claim about `parseInt` is now a Rust claim about `parse`. Cross-domain verification is free because the bridges are hash-bounded.

## The `.proof` file as the package format

Traditional:
```
package.json → describes package
node_modules/ → contains code
(no verification included)
```

ProvekIt:
```
<cid>.proof → IS the package
  ├── contracts: what the code guarantees
  ├── bridges: how it relates to other packages
  ├── binaryCid: the exact compiled artifact
  ├── metadata: bytecode references, build flags, etc.
  └── signature: developer-signed, tamper-evident
```

The `.proof` file is self-contained. It needs no network. It needs no registry. It needs no central authority. It is a signed, content-addressed bill of materials for the entire verified artifact.

## Supply chain security

The `binaryCid` field is the supply chain anchor:

- **Compiler backdoor:** Different binary → different CID → proof fails
- **Runtime patch:** JIT override changes binary hash → proof fails
- **Dependency injection:** Wrong package → wrong CID → proof fails
- **Monkey-patch:** User redefined `parseInt` → contract CID doesn't match → proof fails

The alarm is not a security scan. It is a **mathematical contradiction.** The proof was minted for bytes with CID X. The running binary has CID Y. X ≠ Y. The proof is invalid for this binary.

## Cross-language conformance

The IR is language-agnostic. A Rust kit, a TypeScript kit, and a Go kit all emit the same canonical bytes for the same canonical formula. A contract memento minted by the Rust kit and a contract memento minted by the TypeScript kit, expressing the same proposition, share a CID.

The handshake at Tier 1 sees them as identical. A TypeScript consumer of a Rust library has the same Tier-1 discharge fraction as a Rust consumer would. Cross-domain verification works because all kits bridge to the same reference contracts.

## Fail-closed posture

Every gate is fail-closed:

- A memento whose recomputed CID does not match the embedded CID is rejected.
- A memento whose signature does not verify is rejected.
- A `.proof` file with malformed CBOR is rejected.
- A `binaryCid` that does not match the running binary is rejected.
- A bridge whose `targetProofCid` does not resolve is rejected.
- A handshake at Tier 3 that times out returns `REQUIRES_PER_CALLSITE`, never a false positive.
- A protocol catalog whose CID does not match the implementation's declared conformance is rejected.

There is no "best effort" mode. There is no "soft fail" mode. The verifier either has a discharge witness or it does not.

## What this architecture does NOT include

ProvekIt does not author specifications. The kit emits IR; the lift adapter reads existing source-language annotations; the developer keeps writing `proptest!`, `#[contracts::ensures]`, `pydantic.BaseModel`, `zod.object`, or whatever idiom their codebase already uses. The protocol's job is to canonicalize, hash, sign, and check. The act of describing what should be true belongs to the libraries the codebase already trusts.

This is the lift-not-author posture. ProvekIt sits beneath every annotation library; it does not compete with any of them. The architecture's surface area is small: CDDL grammar, codegen pipeline, authoring kit, proof bundle, verifier. The rest is implementation.

## Read further

- [README.md](README.md) for the install path.
- [THESIS.md](THESIS.md) for the deeper architectural claim: hash-bounded verification.
- [PRODUCT.md](PRODUCT.md) for what ProvekIt replaces and complements.
- [docs/per-language-status.md](docs/per-language-status.md) for kit and adapter coverage.
- [protocol/specs/](protocol/specs/) for the canonical spec set, addressed by CID.
