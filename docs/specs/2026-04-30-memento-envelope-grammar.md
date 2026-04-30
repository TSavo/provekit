# ProvekIt: Memento Envelope Grammar

> Author: shared session 2026-04-30 (T + Claude). Per-variant CDDL
> schemas for the data shapes that flow through ProvekIt's content-
> addressed memento store. The spec is authoritative; TypeScript types
> in `src/claimEnvelope/types.ts` and the various producer files
> conform to this grammar or are bugs.

> Companion specs: IR formal grammar, canonicalization grammar
> (computes the bytes that get hashed into a memento's CID),
> signatures and non-repudiation (every signed-required memento's
> signature scheme), chain validity (per-edge invariants between
> mementos), IR extension protocol (extension-declaration memento
> variant).

## 1. What this spec specifies

A memento is a content-addressed claim. Every memento has:

- A `kind` discriminator (this spec's primary CDDL choice point)
- Per-kind body fields
- A CID computed by hashing its canonical bytes (per the
  canonicalization spec)
- Optionally a signature block (per the signatures spec; some kinds
  require signing, some are explicitly unsigned by design)

This document is the per-kind grammar. Each variant is a CDDL rule.
Every implementation that produces or consumes mementos conforms to
these shapes; documents that don't conform are not valid mementos.

## 2. The discriminator union

```cddl
memento =
    catalog-memento
  / property-memento
  / bridge-memento
  / verdict-memento
  / audit-memento
  / deprecation-memento
  / extension-declaration       ; per IR extension protocol spec
  / extension-catalog           ; per IR extension protocol spec
  / public-key-memento          ; per signatures spec

cid = tstr        ; multibase-encoded CID per IPLD; opaque to this spec
sig = bstr        ; Ed25519 signature; per signatures spec
ts  = tstr        ; ISO-8601 UTC timestamp
```

The `extension-declaration`, `extension-catalog`, and
`public-key-memento` variants are specified in their respective
sibling specs. The remaining six variants are specified here.

## 3. Catalog memento

A library's published list of property declarations. The catalog
memento's CID is what `package.json`'s `provekit.proofHash` field
points at.

```cddl
catalog-memento = {
  kind: "catalog",

  ; The library's name and version. Display-only; identity is the
  ; CID, not the name. Two catalogs can share a name and version with
  ; different CIDs (e.g. before-and-after a bump).
  name: tstr,
  version: tstr,

  ; Map from human-readable property name to property memento CID.
  ; Names are local to the catalog; cross-catalog name collisions are
  ; surfaced by the verifier (chain-validity rules), not auto-resolved.
  properties: { + tstr => cid },

  ; Optional list of bridge memento CIDs the catalog also publishes.
  ; A library may carry both its own properties AND its consumer-side
  ; usage commitments (when the library itself depends on other libs).
  ? bridges: [* cid],

  ; Optional pointer to an extension-catalog memento that this catalog
  ; brings into scope when consumed. Published extensions ship with
  ; the library that declares them.
  ? extensions: cid,

  ; Optional dependency manifest: catalogs this catalog references
  ; transitively. Verifiers walk this to populate the resolver's
  ; in-scope catalog set.
  ? depends-on: [* cid],

  signer: cid,
  signature: sig,
  declaredAt: ts,
}
```

**Validation rules:**
- `properties` map values MUST resolve to property mementos (per chain
  validity).
- `bridges` CIDs MUST resolve to bridge mementos.
- `extensions` CID, if present, MUST resolve to an extension-catalog
  memento.
- `signer` CID MUST resolve to a public-key memento; signature MUST
  verify; signer's key MUST be active at `declaredAt`.

**Required signature.** Catalog mementos MUST be signed. Unsigned
catalogs fail closed.

## 4. Property memento

One IR claim plus the canonical-form bytes that hash to its
propertyHash. This is the load-bearing memento: every claim about
code is a property memento.

```cddl
property-memento = {
  kind: "property",

  ; Free-text intent at authoring time. Display-only; the formal
  ; claim is in `formula`. Authors should use this to record the
  ; user-visible reason for the invariant.
  originatingIntent: tstr,

  ; The IR formula expressing the property. Per the IR formal grammar.
  formula: ir-formula,

  ; Optional callsite where the property was first bound. Self-healing
  ; per the substrate-binding rules.
  ? callsite: callsite,

  ; Optional pointer to the source file content (for human readers
  ; who want to see what the IR claim is referring to).
  ? sourceCid: cid,

  ; Optional list of regression test CIDs that ground this claim.
  ? regressionTests: [* cid],

  ; Optional retired-marker. Non-null means the property is retired;
  ; verifiers MUST treat retired properties as semantically gone for
  ; consumer-side bridge resolution (per chain-validity case 4).
  ? retired: deprecation,

  signer: cid,
  signature: sig,
  declaredAt: ts,
}

callsite = {
  filePath: tstr,
  function: tstr / null,
  ? functionHash: tstr,        ; substrate node subtreeHash; self-heal
  ? functionOffset: int,       ; line - fn.startLine at write time
  startLine: int,
  endLine: int,
}

deprecation = {
  reason: tstr,
  retiredAt: ts,
  ? replacedBy: cid,
}

ir-formula = any              ; per the IR formal grammar
```

**Validation rules:**
- `formula` MUST conform to the IR formal grammar.
- The CID of this memento (computed by hashing canonical bytes) IS
  the propertyHash of the formula. Two property mementos with
  byte-identical canonical formulas have identical CIDs.
- If `callsite.functionHash` is present, `functionOffset` MUST also
  be present (the pair is the recovery information per the binding
  spec).
- `signer`/signature/`declaredAt` per signatures spec.

**Required signature.** Property mementos MUST be signed.

## 5. Bridge memento

A consumer-side edge. Names a call site in the consumer's code that
depends on a property published elsewhere (a library, a kit catalog,
a system layer).

```cddl
bridge-memento = {
  kind: "bridge",

  ; The symbol in the consumer's code where this bridge anchors.
  ; Free-text identifier; semantics are scoped to sourceLayer.
  sourceSymbol: tstr,

  ; Which kit/layer the consumer's code lives in. Examples:
  ;   "ts-kit", "rust-kit", "go-kit", "cpp-kit",
  ;   "node-runtime", "v8", "ecma-262", "linux-syscalls"
  sourceLayer: tstr,

  ; The property this bridge depends on. Property memento CID.
  targetContractCid: cid,

  ; Which kit/layer the target property's IR is expressed in. Need
  ; not equal sourceLayer; cross-layer bridges are how the proofHash
  ; chain reaches across language boundaries.
  targetLayer: tstr,

  ; Optional human-readable note explaining why this bridge exists.
  ? notes: tstr,

  signer: cid,
  signature: sig,
  declaredAt: ts,
}
```

**Validation rules:**
- `targetContractCid` MUST resolve to a property memento.
- The target property MUST NOT be retired as of `declaredAt`
  (retired-target bridges fail closed; verifiers route to the
  re-evaluate-invariant workflow per chain validity case 4).
- `sourceSymbol` MUST be non-empty.
- `signer`/signature per signatures spec.

**Required signature.** Bridge mementos MUST be signed by the
consumer (the party making the dependency claim).

## 6. Verdict memento

A solver's verdict on a property. `verdict-memento` covers single-
solver verdicts; composite (multi-solver) verdicts are ALSO
verdict-mementos with multiple per-solver entries.

```cddl
verdict-memento = {
  kind: "verdict",

  ; The property memento this verdict claims a result about.
  propertyCid: cid,

  ; The verdict's claim. unsat means the negation is unsatisfiable
  ; (i.e., the property holds). sat means the negation is satisfiable
  ; (i.e., the property is refuted by the witness).
  ; "undecidable" means the solver(s) returned unknown/timeout AND
  ; the verifier policy did not auto-classify; the verdict explicitly
  ; carries no claim.
  verdict: "unsat" / "sat" / "undecidable",

  ; The compiler used to translate the IR to the solver's input
  ; language (per provekit.config.yaml's SolverEntry.compiler field).
  compiler: tstr,

  ; Per-solver entries. For a single-solver verdict this has one
  ; element; for a composite verdict (e.g. Z3+CVC5 agreement) it has
  ; one element per solver.
  perSolver: [+ {
    solverType: tstr,        ; "z3", "cvc5", "lean4", ...
    solverVersion: tstr,
    rawVerdict: "sat" / "unsat" / "unknown" / "timeout",
    ? witness: any,          ; concrete model when rawVerdict = "sat"
    ? proofArtifactCid: cid, ; reference to a proof memento (Lean, etc)
  }],

  ; True iff every perSolver entry returned the same rawVerdict.
  agreed: bool,

  ; The IR-CID input the verdict was computed against. Validity rule:
  ; this MUST equal the IR CID computed from the property memento's
  ; formula via the canonicalization spec. Mismatches fail closed.
  irCid: cid,

  signer: cid,
  signature: sig,
  declaredAt: ts,
}
```

**Validation rules:**
- `propertyCid` MUST resolve to a property memento.
- `irCid` MUST equal the canonical IR CID of that property's formula.
- For `verdict: "unsat"`, all `perSolver[].rawVerdict` MUST be either
  `unsat` or all be present and `agreed = true`.
- For `verdict: "sat"`, at least one `perSolver[].witness` MUST be
  non-null.
- `compiler` MUST match a registered IR compiler.

**Required signature.** Verdict mementos MUST be signed by the entity
that produced the verdict (the solver runner, typically the verifier
itself or a CI service).

## 7. Audit memento

A Stage's input/output trace. Audit mementos record what a workflow
runner did; they are EXPLICITLY UNSIGNED by design and carry no
verifiable claim — only operational provenance.

```cddl
audit-memento = {
  kind: "audit",

  ; Which workflow Stage ran.
  stageName: tstr,
  ; The stage's `producedBy` identity (e.g. "intake@v1").
  producedBy: tstr,

  ; The stage's serialized input (canonicalized).
  inputCid: cid,
  ; The stage's serialized output (canonicalized).
  outputCid: cid,

  ; A salt to ensure two invocations of the same stage with the
  ; same input produce distinct audit memento CIDs (so two runs are
  ; tracked as separate audit events).
  auditSalt: tstr,

  ; When the stage ran.
  recordedAt: ts,
}
```

**Validation rules:**
- All fields except `auditSalt` (which is opaque) MUST be present.
- `inputCid` and `outputCid` MUST resolve in the memento store.

**Signature: NONE.** Audit mementos are unsigned by design. They
exist for operational debugging and cache identity, not for
verifiable claims. A consumer's verifier MUST NOT treat audit
mementos as evidence for correctness; only as evidence for "this
stage ran with these inputs and produced these outputs."

A verifier MAY require audit mementos to be present for cache hits
on non-deterministic stages; that's a runner-side concern.

## 8. Deprecation memento

A retirement event for a property. References the property being
retired and provides reason text.

```cddl
deprecation-memento = {
  kind: "deprecation",
  retiredPropertyCid: cid,
  reason: tstr,
  retiredAt: ts,
  ? replacedByPropertyCid: cid,
  signer: cid,
  signature: sig,
}
```

**Validation rules:**
- `retiredPropertyCid` MUST resolve to a property memento.
- `signer` MUST be the same key (or a successor in the key's rotation
  history) as the property memento's signer; only the property's
  publisher (or their authorized successor) may retire it.
- `retiredAt` MUST be later than the property memento's `declaredAt`.

**Required signature.** Deprecation mementos MUST be signed.

## 9. Cross-cutting requirements

### 9.1 Field ordering

CDDL maps in this spec are unordered as written, but the
canonicalization spec specifies the on-wire field order for hashing.
Implementations MUST produce canonical bytes per that spec; this
grammar describes WHICH fields exist, not the byte-level ordering.

### 9.2 CID computation

A memento's CID is the hash of its canonical bytes (per
canonicalization spec). Two implementations producing byte-identical
canonical forms produce identical CIDs.

### 9.3 Signature payload

When a memento has a `signer` + `signature` field pair, the signature
payload is the canonical bytes of all OTHER fields (signer included,
signature excluded). The signatures spec details this.

### 9.4 Optional vs required signing

| Variant                | Signature       |
|------------------------|-----------------|
| catalog-memento        | REQUIRED        |
| property-memento       | REQUIRED        |
| bridge-memento         | REQUIRED        |
| verdict-memento        | REQUIRED        |
| audit-memento          | NONE (by design)|
| deprecation-memento    | REQUIRED        |
| extension-declaration  | REQUIRED        |
| extension-catalog      | REQUIRED        |
| public-key-memento     | REQUIRED (self-signed for root keys; signed by an authority for delegated keys; details in signatures spec) |

### 9.5 Reference resolution

Every CID-typed field is a content-addressed reference. The
chain-validity spec specifies what makes the reference graph valid;
this grammar specifies what the reference shapes ARE.

## 10. Conformance criteria

A memento producer or consumer conforms to this spec iff it:

1. **MUST** populate the `kind` field on every memento it produces;
   readers MUST dispatch on `kind` for parsing.
2. **MUST** include all REQUIRED fields per the per-variant CDDL.
3. **MUST** sign mementos whose variant is REQUIRED-signed.
4. **MUST NOT** sign audit mementos.
5. **MUST** validate per-variant rules in §3-§8 before treating a
   memento as authoritative.
6. **MUST** route signature failures, missing required fields, and
   invalid CID references to fail-closed handlers per chain-validity.
7. **SHOULD** preserve unknown fields when reading and re-emitting a
   memento (forward-compatibility); future protocol versions add
   fields, and round-tripping should not strip them.

## 11. The architectural commitment, restated

Mementos are the wire format of the protocol. Every claim that
travels in ProvekIt is a memento; every memento conforms to this
grammar; every conformant implementation produces and consumes
mementos that interop with the reference. The TypeScript types in
`src/claimEnvelope/types.ts` are one realization; alternative
implementations in any language conform to this CDDL or they are not
ProvekIt.

The grammar is the contract. Surface choices (which CBOR tag, which
JSON canonicalization, which CID multibase) live in the
canonicalization spec; field-shape choices live here; signature
mechanics live in the signatures spec; chain-level validity lives in
the chain-validity spec. Together these four specs plus the IR
formal grammar plus the IR extension protocol constitute the protocol
in full.
