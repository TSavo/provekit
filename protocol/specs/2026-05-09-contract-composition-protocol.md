# Contract Composition Protocol (CCP)

**Status:** v0.1.0 draft
**Date:** 2026-05-09
**Layer:** extension protocol over the proof substrate, the lift-plugin-protocol, the memento envelope grammar, the handshake algorithm, and paper 07 §6's "compose for free, compress to nothing" theorem

## Section 0. Purpose

CCP defines how atomic contract mementos compose across function call sites into ComposedFunctionContract mementos whose CIDs are the algebraic composition of their atomic constituents. The composed CID is what the handshake algorithm's tier 2 cache reuses for O(1) discharge of structurally-equivalent chains across any future program in any language.

The wire-level claim:

```text
atomic FunctionContractMementos (per function, per lifter)
  + effect sets (per function, per lifter)
    → compose_chain_contracts(...)              [one canonical primitive]
    → ComposedFunctionContract memento (CID-addressed)
    → handshake tier 2 cache hit on this CID for any equivalent chain
```

CCP names the canonical compose primitive, its binding modes for cross-language consumers, the materialization-timing options, the per-language prerequisite (effects), and the cross-language equivalence requirement that makes federation work.

CCP is the architectural answer to: *how does composition stay correct when N lifters in N languages all need to produce structurally-equivalent composed CIDs?* The answer is one canonical implementation, called identically by every consumer.

## Section 1. Relation to existing protocols

| Existing artifact | CCP role |
|---|---|
| Memento envelope grammar | Defines `contract-evidence` body shape; CCP populates ComposedFunctionContract bodies of that shape. No schema change required. |
| Handshake algorithm | Defines tier 2 cache lookup `implications_by_pair`; CCP fills the cache by producing reusable composed contracts. |
| Contract merge semantics | Defines how multiple contracts on the same function combine (∧). Distinct from CCP, which composes contracts ACROSS function boundaries (call-site composition). The two protocols compose: merge per function, then CCP across calls. |
| Lift-plugin-protocol | Each per-language lifter dispatched via JSON-RPC. CCP extends what lifters MAY emit (composed contracts in addition to atomic ones). |
| Paper 07 §6 | "Compose for free, compress to nothing", the structural theorem CCP operationalizes. |
| FRP | Fix receipts MAY cite ComposedFunctionContract CIDs as `policyCid` when the policy is a chain-level guarantee rather than a per-function one. |
| PPP | Predicates over ComposedFunctionContract are strictly more expressive than predicates over atomic contracts. PPP's substrate v2 schema MAY add a `composed_contracts` relation. |

CCP does not replace the merge spec, the envelope grammar, or the handshake. It names the composition primitive that makes the handshake's tier 2 cache populate richly enough to amortize verification cost across the ecosystem.

## Section 2. The composition function

`compose_chain_contracts` is a pure, deterministic function with this signature:

```text
compose_chain_contracts:
  inputs:
    atoms: [FunctionContractMemento]    ; ordered by call-graph depth, leaf first
    effect_sets: [EffectSet]             ; one per atom, same order
  output:
    Result<ComposedFunctionContract, CompositionError>
```

**Determinism guarantees.** Given identical inputs (canonical-encoded), `compose_chain_contracts` produces byte-identical output. No clock-time inputs. No random salts. No platform-dependent ordering. The output CID is therefore a function of input CIDs and the version of the compose function itself.

**Purity refusal.** If any atom's effect set is non-empty, composition refuses with `CompositionError::ImpureInput` and identifies the impure atom by CID. Composition is sound only over pure subtrees.

**Singular formal substitution.** For an atom F with formals `[a, b, c]` called as `F(x, G(y), z)`, only the second argument `G(y)` triggers composition (G's contract substitutes into F's at the b-position). The first and third arrivals (`x`, `z`) are leaf substitutions that do not introduce new contract-level composition.

**CID-namespaced result variable.** Each atom's `post` formula references a free variable (conventionally `result`). When composing F into a caller's pre, F's `result` is renamed to `result_<F.cid>` to avoid free-variable collision across nested composition.

**Effect-set composition.** The composed contract's effect set is the disjoint union of its atoms' effect sets. Pure ∪ Pure = Pure. Composition of any subtree containing one impure atom refuses; partial composition over a subtree that excludes the impure atom is permissible if the call graph allows it.

## Section 3. Effects

Per-language prerequisite. Without effects extraction, composition cannot be sound; the lifter cannot decide which subtrees are safely composable.

The canonical Effect kinds:

```text
Effect ::=
  | Reads  { target: <string> }      ; named memory read (field, global)
  | Writes { target: <string> }      ; named memory write
  | Io                                ; arbitrary I/O (filesystem, network, sysfs)
  | Unsafe                            ; raw memory or hardware access bypass
  | Panics                            ; explicit BUG_ON / panic / abort
  | UnresolvedCall { name: <string> } ; indirect dispatch via function pointer
```

A function's effect set is the disjoint union of effects observable in its body. Pure functions have effect set = ∅.

**Per-language extraction notes:**

- **Rust** (`sugar-walk/src/contract.rs`): already implemented. Reads/writes from MIR borrow analysis; Io from std::io and similar trait impls; Unsafe from unsafe blocks; Panics from `panic!` / `unwrap`; UnresolvedCall from dyn-trait dispatch and function pointers.
- **C** (planned): Reads/writes from libclang AST `MemberExpr` and `ArraySubscriptExpr` walks; Io from sysfs/debugfs/netlink/ioctl entry signatures; Unsafe from raw bitops, MMIO accesses, type punning; Panics from BUG_ON / WARN_ON / panic chains (already in c-assertions lifter); UnresolvedCall from indirect dispatch through ops tables (the rxkad ops-table case demonstrated in 2026-05-09's experimental record).
- **Java**: Reads/writes from field-access AST; Io from `java.io` / `java.net` references; Unsafe from `sun.misc.Unsafe`; Panics from `throw` of unchecked exceptions; UnresolvedCall from interface dispatch and reflection.
- Other languages: per-lifter responsibility.

Effect extraction MAY be conservative (over-tag effects) but MUST NOT be liberal (missing real effects). Conservative tagging refuses some valid compositions but never produces unsound ones. Liberal tagging produces unsound composed contracts and breaks correctness.

## Section 4. Materialization timing

Composition can occur at lift time (eager) or at prove time (lazy). Both produce identical CIDs because `compose_chain_contracts` is canonical and deterministic.

**Eager materialization (lifter-side).** A lifter that has effect-tracking and access to the canonical compose primitive walks its emitted call graph, composes pure subtrees, and emits ComposedFunctionContract mementos as additional members of its `.proof` bundle. The bundle ships with the composed contracts pre-materialized; downstream verifiers consume them via tier 1 (hash equality) on the composed CID.

**Lazy materialization (verifier-side).** A verifier walking a `.proof` catalog may discover atomic contracts plus call-edge mementos but no pre-composed chains. As the handshake's tier 3 fires for a (post, pre) pair, the verifier MAY invoke `compose_chain_contracts` to produce a ComposedFunctionContract memento, persist it for cache reuse, and mark the call site discharged via the resulting implication.

**Both paths produce identical CIDs.** This is enforced by construction: the compose primitive is a single canonical function. A producer who composes eagerly and a consumer who composes lazily over the same atomic constituents produce byte-identical ComposedFunctionContract bodies, and therefore byte-identical CIDs.

A consumer MUST NOT trust a lifter-produced composed contract over its own re-derivation when both inputs are present. The contract is checked by recomputing its CID from the canonical bytes; signature and producer identity are metadata.

## Section 5. Canonical implementation

The compose function lives in **libsugar** (the workspace-internal Rust library at `implementations/rust/libsugar/`). The function signature:

```rust
pub fn compose_chain_contracts(
    atoms: &[FunctionContractMemento],
    effect_sets: &[EffectSet],
) -> Result<ComposedFunctionContract, CompositionError>
```

The implementation MUST be:

1. **Pure.** No state, no I/O, no clock, no random.
2. **Deterministic.** Identical inputs → identical outputs, byte-for-byte.
3. **Schema-versioned.** The function carries a CCP version tag in the produced ComposedFunctionContract body. A consumer with CCP v1 produces different CIDs from a consumer with CCP v2 over the same atomic inputs. The version is part of the canonical bytes hashed.
4. **Reference-only.** No mutation of inputs. The function consumes immutable references and returns a fresh value.

**Test corpus.** The canonical implementation is paired with a fixture-based test corpus that pins composed CIDs to expected hex values for a representative set of input shapes. Any change to the compose function that alters expected CIDs requires a CCP version bump and a deprecation notice.

## Section 6. Binding modes for cross-language consumers

The canonical compose function is exposed via three binding modes. The choice of binding is a consumer ergonomics question; the resulting CIDs are identical across modes.

### 6.1 Direct Rust linking

Rust consumers (sugar-walk, sugar-verifier, sugar-cli, future Rust-side lifters) link to libsugar and call `compose_chain_contracts` directly. Zero-copy where possible. No process boundary.

### 6.2 C ABI FFI

A `sugar-compose.h` header exposes a C-callable wrapper:

```c
typedef struct pk_composition_result pk_composition_result;

pk_composition_result *pk_compose_chain_contracts(
    const char *atoms_jcs,        // JCS-encoded JSON array of FunctionContractMemento
    const char *effects_jcs,      // JCS-encoded JSON array of EffectSet
    size_t atoms_len,
    size_t effects_len
);

const char *pk_composition_result_cid(const pk_composition_result *r);
const char *pk_composition_result_body_jcs(const pk_composition_result *r);
const char *pk_composition_result_error(const pk_composition_result *r);
void pk_composition_result_free(pk_composition_result *r);
```

The C lifter family (sugar-lift-c-kernel-doc, sugar-lift-c-sparse, sugar-lift-c-assertions) and any other native consumer links to libsugar's static library and uses this header. Marshaling is JCS-encoded JSON across the boundary; libsugar owns the canonical encoding.

### 6.3 JSON-RPC subprocess

For consumers that cannot link Rust (TypeScript / Python / Ruby / PHP lifters running in their own runtime), the canonical compose is accessible via a `sugar compose` CLI subprocess speaking JSON-RPC over stdin/stdout. The protocol mirrors the lift-plugin-protocol shape:

```text
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
← {"jsonrpc":"2.0","id":1,"result":{"protocol_version":"sugar-compose/1","ccp_version":"1.0.0"}}

→ {"jsonrpc":"2.0","id":2,"method":"compose","params":{
    "atoms": [...],
    "effects": [...]
  }}
← {"jsonrpc":"2.0","id":2,"result":{"composed_cid":"blake3-512:...","body_jcs":"..."}}
   OR
← {"jsonrpc":"2.0","id":2,"error":{"code":-1,"message":"impure input","atom_cid":"blake3-512:..."}}

→ {"jsonrpc":"2.0","id":3,"method":"shutdown"}
← {"jsonrpc":"2.0","id":3,"result":null}
```

The CLI subprocess is itself a thin wrapper over libsugar. Same canonical implementation; just a different transport.

## Section 7. Cross-language equivalence

The federation guarantee CCP makes is empirically testable in bug-zoo via a cross-language equivalence specimen.

The specimen ships:

- A Rust source: an iterator-chain `vec.iter().map(double).filter(positive).sum()` where `double` and `positive` and `sum` are pure functions with known contracts.
- A C source: a structurally-equivalent computation expressed as `sum(filter(map(double, vec, n), positive, n), n)` with each helper as a pure function with the same contract shape.
- A test runner that lifts both sources via their respective lifters, composes via the canonical compose primitive, and asserts that the resulting ComposedFunctionContract CIDs are byte-identical.

If the test passes, federation across the C and Rust lifters is empirically confirmed. If it fails, the divergence is the precise feature request: which side's lifter or compose binding is producing different bytes, and why.

The bug-zoo specimen is the load-bearing test for the entire CCP federation property. It is the empirical guarantee that paper 07 §6's structural claim holds under the actual implementation.

Future lifters (Java, Go, TypeScript, Python, etc.) extend the specimen with structurally-equivalent source and assert against the same composed CID.

## Section 8. Failure modes

### 8.1 Impure input

`compose_chain_contracts` refuses any input where an atom's effect set is non-empty. The error names the impure atom by CID and the offending effects. Consumers MUST handle this case; they MUST NOT attempt to coerce composition on impure subtrees.

A consumer MAY decompose the impure subtree into sub-chains that are individually pure and compose those separately, treating the impure atom as a barrier.

### 8.2 Schema-version mismatch

If atoms' producers used different schema versions (e.g., one atom is FunctionContractMemento v1.0.0, another is v1.1.0), composition refuses with `CompositionError::SchemaVersionMismatch`. Consumers MUST upgrade atoms to a common schema version (typically by re-lifting the source) before composing.

### 8.3 Effect-set incompatibility

Reserved for future effect-kind extensions where two effect kinds CONFLICT rather than merely combine. v1.0.0 has no conflicting effect kinds; the disjoint union is always well-defined. Future versions MAY introduce conflicts (e.g., a `MutuallyExclusive` effect kind), in which case composition refuses with `CompositionError::IncompatibleEffects`.

### 8.4 Determinism violation

If a third-party implementation of `compose_chain_contracts` produces different CIDs for the same canonical inputs as libsugar, the third-party implementation is non-conformant. Verifiers SHOULD refuse to admit composed contracts produced by non-conformant implementations. The conformance check is empirical: run the bug-zoo cross-language equivalence specimen.

### 8.5 Lifter effects-tracking gap

A lifter that emits atomic FunctionContractMementos without effect sets, or with conservative-but-incomplete effect sets, produces atoms that compose into UNSOUND composed contracts. The composed contract claims pure-composition properties that may not hold.

This is the most insidious failure mode because it is silent. The composed CID hashes; the verifier admits it; downstream consumers reuse it. The unsoundness only surfaces when a real input violates the falsely-claimed pure-composition property.

Mitigation: lifters MUST declare their effect-tracking completeness as a soundness memento attached to the lift output. A lifter that says "I track all effect kinds" carries a stronger soundness claim than one that says "I track Reads/Writes only." Composed contracts from incomplete-effect-tracking lifters carry the lifter's soundness memento as a reference; consumers decide locally whether to admit them.

## Section 9. Composition algebra

The composition algebra implemented by `compose_chain_contracts` is defined by these rules. Any conforming implementation MUST follow them byte-for-byte.

**Rule 1. Singular formal substitution.** For atom F with formals `[p1, p2, ..., pN]` called as `F(arg1, arg2, ..., argN)`, only the formals where `argi` is itself a function-call expression trigger composition. Leaf arrivals (variables, literals, struct-member accesses) do not trigger composition; they are simple substitutions in F's pre and post.

**Rule 2. Inner-result renaming.** When composing inner atom G into outer atom F at formal position `pi`, G's `post` formula uses the free variable conventionally named `result`. Before substituting G's post into F's pre at position `pi`, G's `result` is renamed to `result_<G.cid>`. This avoids free-variable collisions when multiple nested compositions reference different inner results.

**Rule 3. Effect-set union.** The composed contract's effect set is `effects(F) ∪ ⋃ effects(Gi for each composed inner Gi)`. Empty if and only if all inputs are empty.

**Rule 4. Composed pre.** The composed pre is F's pre with each composed inner G substituted at the appropriate formal position. Free variables in F's pre that referenced `pi` (where `argi` is now a composition) are replaced by G's post (with `result` renamed per Rule 2) conjoined with G's pre.

**Rule 5. Composed post.** The composed post is F's post with the same substitutions applied. Free variables in F's post that referenced `pi` are replaced as in Rule 4.

**Rule 6. Composed body CID.** A new bodyCid is computed by hashing the canonical encoding of (F.bodyCid, [Gi.bodyCid for each composed inner], composition algebra version). This bodyCid is content-derived from inputs.

**Rule 7. Composed contractName.** The composed contract's name is `composed:<F.contractName>:<G1.contractName>:...:<GN.contractName>` joined in canonical order (call-graph depth, leftmost first). This is human-readable; the load-bearing identity is the CID.

**Rule 8. Composed schemaVersion.** The composed schemaVersion is the maximum schemaVersion across all inputs (semver max). If any input is at v2 and others at v1, the composed contract is at v2.

These rules are the formal specification. The libsugar implementation MUST implement them; the bug-zoo specimen MUST verify them.

## Section 10. Worked example: kernel C function chain

Take the kernel call chain `kmalloc → memset → return`:

```c
void *zalloc_buffer(size_t n) {
    void *p = kmalloc(n, GFP_KERNEL);
    if (!p) return NULL;
    memset(p, 0, n);
    return p;
}
```

The C lifter (with effect tracking; today's gap) would emit three atomic FunctionContractMementos:

```text
M_kmalloc:    pre = (n > 0); post = (result == null OR allocated(result, n));
              effects = { Reads(GFP_KERNEL), Writes(slab_state) }   ; impure
M_memset:     pre = (allocated(p, n)); post = (zeroed(p, n));
              effects = { Writes(*p) }                              ; impure
M_zalloc_buffer: pre = (n > 0); post = (result == null OR (allocated(result, n) AND zeroed(result, n)));
              effects = { Reads(GFP_KERNEL), Writes(slab_state), Writes(*result) }   ; impure
```

The composition refuses at v1 because all three are impure. This is correct: kmalloc has side effects on the slab allocator state; memset writes to caller-provided memory; both are observable.

But pure subtrees within the kernel DO exist: arithmetic helpers, format-string parsers without I/O, container traversals. For those, the C lifter emits pure atoms; CCP composes; the chain CID becomes a substrate-cached implication.

For the rxkad in-place chain studied in 2026-05-09's experimental record:

```text
rxkad_verify_packet → rxkad_verify_packet_2 → skb_to_sgvec → crypto_skcipher_decrypt
```

Each of these has effects (Reads on skb, Writes on the SGL contents, Io on the crypto subsystem). Composition refuses. The chain stays in tier 3 (per-call-site Z3 discharge) under v1 of CCP. A future v2 of CCP that introduces effect-aware composition (composition over not-purely-pure chains where effects are explicitly tracked) MAY admit these chains; out of scope for v1.

## Section 11. Versioning and revocation

CCP itself versions. v1.0.0 is described in this document. Future versions (v1.1.0, v2.0.0) extend the algebra rules, the effect kinds, the binding modes. Each version is its own canonical implementation; producers tag their composed contracts with the CCP version used to compose.

A consumer MAY admit composed contracts produced under multiple CCP versions if the consumer's verifier knows how to validate each. Contracts produced under a CCP version the consumer doesn't know are treated as opaque (consumer cannot recompute the CID, so cannot verify).

Revocation: if a flaw is discovered in a CCP version's algebra that produces unsound composed contracts, the version is withdrawn via a signed revocation memento under libsugar's maintainer key. Consumers SHOULD honor revocation; verifiers MUST refuse to admit composed contracts under withdrawn versions.

## Section 12. Pipeline

```text
Source code (any language)
  │
  ▼ (per-language lifter)
Atomic FunctionContractMementos + EffectSets
  │
  ▼ (compose_chain_contracts in libsugar, called via FFI / CLI / direct-link)
ComposedFunctionContract mementos (CID-addressed, content-derived)
  │
  ▼ (emitted into .proof bundle alongside atomics, eager)
  │   OR
  │ (computed by verifier on-demand during prove, lazy)
  ▼
Handshake algorithm tier 2 cache populated
  │
  ▼ (handshake fires on every call site sharing the (post, pre) pair)
O(1) discharge of structurally-equivalent chains across the ecosystem
  │
  ▼ (the implication closure that the proofchain head carries)
Federation across languages (any lifter that respects CCP contributes)
```

Every composed CID is content-addressed. Every binding mode produces identical CIDs by construction. The federation property is empirically guaranteed by the bug-zoo cross-language equivalence specimen.

## Appendix A. Canonical encoding for compose inputs

`compose_chain_contracts` consumes its inputs in canonical encoding to ensure determinism. The encoding:

- Atoms are canonicalized as JCS (JSON Canonicalization Scheme, RFC 8785) over the FunctionContractMemento body.
- Effect sets are canonicalized as JCS over an array of Effect objects ordered lexicographically by (kind, target).
- Atoms are passed in call-graph order (leftmost / leaf first).
- Effects are passed in the same order as atoms.
- The compose function rejects any input that fails JCS-validity check.

Consumers writing wrappers (FFI, CLI) MUST canonicalize inputs before calling. Consumers MUST NOT post-process compose's output bytes; the output IS canonical by construction.

## Appendix B. Reference implementation surface

The v1 reference implementation lives at `implementations/rust/libsugar/src/compose.rs` (planned). It exposes:

- `pub fn compose_chain_contracts(...)`, the canonical primitive
- `pub struct EffectSet`, opaque type
- `pub enum Effect`, the effect kinds
- `pub enum CompositionError`, the failure variants
- `pub struct ComposedFunctionContract`, the output type
- `pub const CCP_VERSION: &str = "1.0.0"`

The C ABI FFI lives at `implementations/rust/libsugar/include/sugar-compose.h` (planned). It exposes the C-callable wrappers per Section 6.2.

The CLI subcommand `sugar compose` lives in sugar-cli (planned). It speaks JSON-RPC per Section 6.3.

A test corpus at `implementations/rust/libsugar/tests/compose_corpus/` pins composed CIDs to expected hex values across a representative set of input shapes. CCP version bumps invalidate this corpus; the corpus is regenerated and committed under the new version's signature.

The bug-zoo cross-language equivalence specimen at `menagerie/bug-zoo/specimens/BZ-COMPOSITION-001-cross-language-equivalence/` (planned) ships the Rust + C structurally-equivalent sources and the test runner that asserts byte-identical composed CIDs. This specimen IS the federation guarantee, in executable form.
