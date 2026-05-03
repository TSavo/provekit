# Bridge Linkage Protocol

**Status:** v1.0.0 normative spec (closes the substrate composition arc)
**Date:** 2026-05-03
**Companion specs:** `2026-05-03-contract-cid-vs-attestation-cid.md`, `2026-05-03-contract-set-extension.md`, `2026-05-03-substrate-layers-envelope-header-body.md`, `2026-05-03-version-chains-pinning.md`, `2026-05-03-bridge-target-dimensionality.md`
**Companion manifesto:** `docs/launch/substrate-not-blockchain.md` §10, §11, §12

## §0. Motivation

This spec closes the substrate composition arc. §10 of the manifesto established that composition is free: subsetting is hashing, and any view over the leaves can be content-addressed without new substrate primitives. §11 named the address as multi-dimensional. §12 named the pin as a tuple whose rank equals the rank of the assertion. The bridge target-dimensionality addendum (`2026-05-03-bridge-target-dimensionality.md`) named the shape of a single bridge.

What remained: who authors bridges. The Phase 2 cross-kit bridge work (PRs #92, #93, #104, #106, #107, #109) had developers writing bridges by hand, slab by slab, using placeholder strings (`pending-csharp-counterpart:<name>`, `deferred:phase-3-proof-bundle`) where addresses were unresolved. That hand-authored shape is non-substrate: a placeholder string is not an address, and a developer copy-pasting bridges across kits is not composition under §10.

The closure: bridges are derivable. A bridge `B → A` is the predicate-level claim `post_B ⊃ pre_A`. Both `pre_A` and `post_B` are predicates already lifted into ProofIR mementos; both are content-addressed as `contractCid_A` and `contractCid_B`. The relation `post_B ⊃ pre_A` is not a new memento; it is the existing call edge `B → A` reinterpreted under predicate logic. The lifter already extracts both pieces from source: per-function contracts, and call edges (the AST tells us which functions call which). LSP backends already resolve cross-file and cross-language references for IDE features. ProvekIt re-uses the existing infrastructure: the lifter emits a call-edge stream alongside the contract stream, and the linker (the rust CLI orchestrator under `provekit prove`) derives bridges mechanically from `(contracts ∪ call-edges)`.

The user never writes a bridge. The linker writes them, byte-deterministically, from inputs that already exist in the lifter's output. ProvekIt's notion of linkage replaces traditional linker semantics (connect symbols by name + type) with predicate-level linkage (connect contracts by `post ⊃ pre` satisfaction obligations) at the same place in the pipeline.

## §1. Definitions

A **call edge** is a memento describing a call site within a lifted compilation unit:

```
CallEdge := {
    schemaVersion: "1",
    kind:               "call-edge",
    sourceContractCid:  <contractCid of the calling function's contract>,
    targetContractCid:  <contractCid of the called function's contract> | null,
    callSiteLocus:      <Locus per ir-formal-grammar.md>,
    targetSymbol:       <language-local symbol name+signature, used when targetContractCid is null>,
    evidenceTerm:       <ProofIR Term encoding the satisfaction obligation post_B ⊃ pre_A>
}
```

A **derived bridge** is a bridge memento minted by the linker from one call edge and the contract memento set:

```
DerivedBridge := {
    schemaVersion:      "2",
    kind:               "bridge",
    envelope: { signer, declaredAt, signature },
    header: {
        kind:               "bridge",
        sourceContractCid:  <from CallEdge.sourceContractCid>,
        target:             { kind: "contract", cid: <resolved targetContractCid> }
    },
    metadata: {
        callSite:           <CallEdge.callSiteLocus>,
        derivedRelation:    {
            kind: "post-implies-pre",
            evidenceTerm: <CallEdge.evidenceTerm>
        },
        derivedBy:          "linker",
        linkerVersion:      <semver of the linker that derived this bridge>
    }
}
```

The **linker** is the component (in ProvekIt v1, the rust CLI under `provekit prove`) that takes the union of per-kit lifter output and emits the closure of derived bridges over the contract set.

## §2. Normative changes

**R1. Lifter output is two streams.** A lift-plugin-protocol RPC server SHALL emit, per compilation unit it lifts:
1. Contract mementos (the existing `kind: "contract"` shape per `ir-formal-grammar.md`).
2. Call-edge mementos (per §1) for every call site within the lifted code.

The two streams are emitted in the same response envelope. A lifter that emits contracts but no call edges is non-conformant under this spec; a lifter that emits call edges referencing contracts not in its own output (i.e., cross-kit calls) sets `targetContractCid` to null and populates `targetSymbol` for linker resolution.

**R2. Linker derives bridges.** The linker SHALL, given the union `U = ⋃_kit (contracts_kit ∪ call-edges_kit)`, derive a bridge memento for each call edge `e ∈ U` as follows:
- Set `header.sourceContractCid = e.sourceContractCid`.
- Resolve target: if `e.targetContractCid` is non-null, use it directly. Otherwise resolve `e.targetSymbol` against the union of contracts (per R3). Set `header.target = { kind: "contract", cid: <resolved> }`.
- Set `metadata.callSite = e.callSiteLocus`.
- Set `metadata.derivedRelation = { kind: "post-implies-pre", evidenceTerm: e.evidenceTerm }`.
- Sign the envelope with the linker's signing key.

The derived bridge memento is content-addressed. Re-running the linker over the same inputs produces a byte-identical bridge envelope (modulo the envelope-axis fields per `2026-05-03-substrate-layers-envelope-header-body.md`).

**R3. Cross-kit call resolution.** When a call edge has `targetContractCid: null`, the linker SHALL resolve `targetSymbol` against the union of all contract mementos in `U`. The resolution algorithm:
1. For each contract memento `c ∈ U`, compute its language-canonical signature: `(c.name, c.metadata.signature, c.metadata.kit)`.
2. Match `targetSymbol` against the canonical signatures, with kit-specific resolvers handling FFI conventions (cgo's `C.foo` maps to `cpp-kit:foo`, Python's `ctypes.dll.foo` maps to `cpp-kit:foo`, etc.).
3. If exactly one contract matches, use its CID. If zero or multiple match, emit a `kind: "linker-error"` memento naming the unresolved or ambiguous symbol; verification SHALL fail-closed against any binary whose .proof contains a linker-error memento.

The kit-specific FFI resolvers are themselves protocol contracts; the lift-plugin-protocol gains a `resolve_ffi_target` method per kit (additive, v1.1.0 of the lift-plugin-protocol).

**R4. Bridges are not authored.** Implementations of the lift-plugin-protocol MUST NOT emit `kind: "bridge"` mementos directly from the lifter. Bridges are exclusively derived by the linker per R2 and R3. Per-kit code that mints bridges manually (the Phase 2 `cross_kit_bridges.<ext>` slabs in cpp/csharp/zig/swift/go) is NON-NORMATIVE under this spec; the migration removes them entirely.

A consumer that wants to assert a non-derived bridge (e.g., a hand-curated cross-kit binding for a contract whose call site is hidden behind dynamic dispatch) MAY mint a bridge directly under their own signing key. Such bridges are valid mementos but are flagged with `metadata.derivedBy: "<consumer name>"` rather than `"linker"`. The substrate verifies them identically; the consumer carries the trust posture.

**R5. The link bundle is content-addressed.** The linker emits a `LinkBundle` memento containing:
- `contractSetCid`: per `2026-05-03-contract-set-extension.md` over the union of contracts in `U`.
- `callEdgeSetCid`: `blake3-512(JCS(<sorted call edges in U>))`.
- `bridgeSetCid`: `blake3-512(JCS(<sorted derived bridges>))`.
- `linkBundleCid`: `blake3-512(JCS(<the LinkBundle object minus this field>))`.

Re-running the linker over byte-identical input streams produces a byte-identical `linkBundleCid`. This is the rank-3 pin per manifesto §12 at the linker's output: `(contractSetCid, callEdgeSetCid, bridgeSetCid)` together with the linker's signature constitutes the linker's rank-3 attestation that the bridge derivation is closed and complete over the input.

The shipped `.proof` bundle includes the `linkBundleCid`. Verifiers re-derive the bridge set from the contracts and call edges and check byte-equality of the recomputed `bridgeSetCid` against the bundle's claim. Mismatches fail-closed.

## §3. Migration

The Phase 2 cross-kit bridges in cpp/csharp/zig/swift/go (`implementations/<lang>/.../cross_kit_bridges.<ext>`) become non-normative under R4. The implementation follow-up:
- Removes the `cross_kit_bridges.<ext>` source files from each kit.
- Removes the per-kit bridge tests pinning hand-authored bridge bytes.
- Adds the call-edge stream emission to each kit's lift-plugin-protocol RPC server (R1).
- Adds the kit-specific FFI resolver per R3.
- Adds the linker pass to the rust CLI's `provekit prove` flow.
- Adds the LinkBundle emission and bundle CID verification to the verifier per R5.

The 10 hand-authored counterpart contracts that Phase 2 also added per kit are retired under spec `2026-05-03-bridge-target-dimensionality.md` R6: bridges anchor at the rust contractCid, kits do not re-declare counterparts.

Existing `.proof` bundles minted before this spec contain hand-authored bridges. Such bundles MAY remain valid as historical artifacts; new bundles MUST emit derived bridges under R2 and a LinkBundle memento under R5.

## §4. Conformance test

A kit's lifter conforms to this spec if all of the following hold:
1. Lifting a polyglot fixture produces both contract mementos and call-edge mementos.
2. Every call-edge memento's `sourceContractCid` matches an emitted contract memento's CID.
3. Cross-language fixture (e.g., a Go file calling a C function via cgo) produces a call-edge memento with `targetContractCid: null` and a populated `targetSymbol`.
4. The kit's `resolve_ffi_target` method correctly resolves an FFI call to the target kit's contractCid, given the union of contracts.

A linker conforms if all of the following hold:
1. Given byte-identical input streams (contracts ∪ call-edges) over two runs, the linker emits a byte-identical `linkBundleCid`.
2. Given a call-edge with unresolved `targetContractCid` and zero matching contracts in the union, the linker emits a `kind: "linker-error"` memento.
3. Given a call-edge with multiple matching contracts (ambiguous symbol), the linker emits a `kind: "linker-error"` memento naming all candidates.
4. The derived bridges' JCS bytes are byte-equivalent across linker implementations: a bridge derived from the same `(contracts, call-edges)` inputs produces the same `bridgeSetCid` regardless of which language the linker is implemented in.

## §5. Architectural framing

This spec is the substrate's notion of linkage. Traditional linkers connect symbols by name and type signature, producing a binary where every reference resolves. ProvekIt's linker connects contracts by predicate satisfaction (`post_B ⊃ pre_A`) at the same place in the pipeline, producing a `.proof` bundle where every call-site obligation is content-addressed and verifiable.

The connection between the two: a traditional linker error ("undefined reference to `foo`") and a ProvekIt linker error ("unresolved targetSymbol `foo`") are the same error at two different rank levels. Traditional linkage is rank-1 (the symbol exists). ProvekIt linkage is rank-2 (the symbol exists and its precondition is established by the caller's postcondition). Composition over §10 gives the closure: every call edge becomes a bridge, every bridge is content-addressed, every linkBundle is content-addressed, every `.proof` is the closure of the program's call graph at the predicate level.

The user writes contracts. The lifter extracts contracts and call edges. The linker derives bridges and emits a content-addressed link bundle. The verifier walks the bundle and validates each obligation. Cross-language calls work because ProofIR is the common predicate language across kits, per the cross-kit conformance gate that has held byte-identity for months.

ProvekIt linkage replaces the type system's role in cross-call safety with predicate-level satisfaction at compile time. No type-system commitment is required from any participating kit; ProofIR is the type system underneath.
