# EffectOccurrence Memento

**Status:** v1.0.0 normative
**Date:** 2026-05-13
**Related:**
- `2026-05-09-language-signature-protocol.md`
- `2026-05-09-contract-composition-protocol.md`
- `2026-05-06-effect-discharge-classification.md`
- admissibility spine #796
- CompositionRefusalMemento #795

## §0. Purpose

`FunctionContractMemento.effects` carries semantic occurrences, not bare effect signature CIDs.

An `EffectSignatureMemento` names a family of behavior in the catalog algebra. A family CID is not enough for composition because the composition decision depends on the particular site payload. `MemRead` is the family. `Reads { target: "x" }` is the occurrence that blocks composition through a write to `x`. `AtomicAccess` is the family. `AtomicAccess { target: "counter", kind: "Rmw", ordering: "SeqCst" }` is the occurrence that can be classified as fully specified.

An `EffectOccurrence` is the promoted semantic payload derived from source evidence. `EvidenceMemento` remains provenance: it records raw source facts and observations. `EffectOccurrence` records what those facts mean to the substrate. `EffectSignatureMemento` remains catalog algebra: it describes the effect family and its laws. `LossRecord.effect_divergence` remains transport accounting: it records effects introduced, lost, or transformed by transport. Observer effects belong to wrappers, not wrapped programs.

The canonical `FunctionContractMemento.effects` field is therefore:

```cddl
function-contract-effects = [* effect-occurrence]
```

Legacy `[* cid]` effect arrays are not semantically complete and are handled only by the migration rule in §6.

## §1. CDDL

```cddl
effect-occurrence = {
  args:            json-value,
  discharge_key:   tstr,
  locator:         json-value,
  occurrence_kind: tstr,
  role:            occurrence-role,
  signature_cid:   cid
}

occurrence-role = "pre" / "post" / "invariant" / "body" / "exceptional"

function-contract-effects = [* effect-occurrence]

cid = tstr
json-value = any
```

The key order in every `effect-occurrence` object is normative for human authored examples and mirrors JCS alphabetical order: `args`, `discharge_key`, `locator`, `occurrence_kind`, `role`, `signature_cid`. Implementations MUST JCS canonicalize the containing `FunctionContractMemento` before hashing.

## §1.5. EvidenceMemento bridge

An `EffectOccurrence` is a PROMOTED semantic object, not a primary observation. The substrate's evidence-promotion discipline (per the admissibility-spine framing #796 and `2026-05-13-promotion-decision-memento.md` #791) applies here as much as anywhere else.

The bridge from observation to occurrence is:

1. **Observation:** A lifter or surface adapter records a raw fact via an `EvidenceMemento` whose `source_kind` is one of:
   - `"effect-extractor"` — a per-language effect extractor (Rust borrow-checker output, C SMT scratch-tracker, Java JMM analyzer, etc.) emitted a structural observation.
   - `"native-effect"` — a native annotation surface (Rust `#[may_panic]`, Java `@Effects`, Python `# effects:` doc tag, etc.) emitted a declared effect.

   The `EvidenceMemento.predicate` field carries the observation in the producer's native shape. For an `effect-extractor` observation, the predicate SHOULD carry a candidate `EffectOccurrence` JSON object inline so that the promotion step is a pure validation, not a transformation. For a `native-effect` observation, the predicate carries the declared annotation text and any structured payload extracted from it.

2. **Promotion:** A `PromotionDecisionMemento` (#791) admits the observation. The decision's:
   - `evidence_cids` lists the `EvidenceMemento` CIDs that motivated the admission.
   - `promoted_cid` is the CID of the `FunctionContractMemento` whose `effects` array now contains the `EffectOccurrence`.
   - `gate` is typically `proof` (an effect-extractor's discharge is the gate) or `human` (a manual native-effect declaration with reviewer sign-off).

3. **Storage:** Per §1, the `EffectOccurrence` lives INSIDE `FunctionContractMemento.effects`. The occurrence object itself does NOT carry an `evidence_cid` field; the link from occurrence to evidence is recorded in the `PromotionDecisionMemento` that admitted it. This keeps the FCM body small, content-addressed by semantic shape, and decoupled from per-run provenance.

A consumer that needs to walk from an `EffectOccurrence` back to its supporting evidence MUST query the catalog for any `PromotionDecisionMemento` whose `promoted_cid` equals the enclosing `FunctionContractMemento` CID and whose `decision_payload` cites the matching occurrence (by JCS-canonical occurrence bytes or by index within `effects`). Cataloging strategies for that query (a reverse index, a join view, or per-evidence forward links) are out of scope for this spec.

The substrate does not encode policy here. A producer MAY mint an `EffectOccurrence` directly without going through a primary `EvidenceMemento` if its local policy treats the lifter output as authoritative (this is common for in-tree Rust borrow-checker extraction). When that path is used, the `PromotionDecisionMemento` MUST still exist, AND it MUST mint (or reference) a synthetic `EvidenceMemento` for the lifter output so that `evidence_cids` is non-empty per `PromotionDecisionMemento` (#791) `evidence_cids: [+ cid]`. The synthetic evidence record at minimum names the lifter CID, the extraction stage, and the lifter inputs that produced the observation. Without it, there is no replay path from the FCM back to who admitted the occurrence and why, AND the promotion decision is not well-formed.

## §2. Field Semantics

| Field | Required | Meaning |
|---|---|---|
| `args` | yes | Kind specific semantic payload. This is the discriminating content that bare signature CIDs do not carry. The object keys inside `args` MUST be alphabetized before JCS encoding. Empty payloads use `{}`. Unknown legacy payloads use `{"unknown": true}` only under §6. |
| `discharge_key` | yes | Stable key used by discharge lookup and refusal reporting. It MUST be derived from the occurrence payload, not from the family alone. Examples: `read:x`, `opaque-loop:blake3-512:...`, `atomic:counter:Rmw:SeqCst`. |
| `locator` | yes | Source or IR site locator for diagnostics and memento anchoring. The shape is language specific JSON, but it MUST be JCS canonical. If no precise locator is available, use `{}` rather than `null`. |
| `occurrence_kind` | yes | Canonical occurrence kind string. The v1 list is defined in §3. The kind is semantic and case sensitive. |
| `role` | yes | Contract position where the occurrence is relevant. `pre` means precondition side, `post` means normal-exit postcondition side, `invariant` means loop or object invariant side, `body` means whole contract declaration or body level (the default for declared effects with no narrower position), and `exceptional` means abnormal-exit / panic / throw / early-return postcondition side. `Panics`, `EarlyReturn`, and explicit thrown exceptions normally carry `role: "exceptional"`; reads/writes/io that happen during the normal control-flow carry `role: "body"`. |
| `signature_cid` | yes | CID of the `EffectSignatureMemento` family in the language signature catalog. Consumers use it for algebraic identity, but MUST NOT classify composition from this field alone. |

`args`, `locator`, and `discharge_key` are part of the content addressed contract body. Changing any of them changes the enclosing contract CID.

## §3. Occurrence Kinds

The canonical v1 occurrence kinds are:

- `Reads`
- `Writes`
- `Io`
- `Panics`
- `OpaqueLoop`
- `UnresolvedCall`
- `AtomicAccess`
- `EarlyReturn`
- `Unsafe`
- `ClosureCapture`
- `PinnedReference`
- `RawPointerProvenance`
- `PossibleAliasing`
- `Drop`

The classification mapping for each kind appears in §4 and is anchored against `2026-05-06-effect-discharge-classification.md`. Any effect kind already present in the classifier or in libprovekit that is NOT listed above MUST be lifted into the legacy synthesis path in §6 (treated as `LegacyUnknown`, fails closed) until added to this list under a `schemaVersion` bump.

### `Reads`

Represents an observable read from a named memory, global, field, capability, or abstract state cell.

```json
{
  "args": {
    "target": "x"
  },
  "discharge_key": "read:x",
  "locator": {
    "column": 12,
    "file": "src/lib.rs",
    "line": 42
  },
  "occurrence_kind": "Reads",
  "role": "body",
  "signature_cid": "blake3-512:mem-read-signature"
}
```

### `Writes`

Represents an observable write to a named memory, global, field, capability, or abstract state cell.

```json
{
  "args": {
    "target": "x"
  },
  "discharge_key": "write:x",
  "locator": {
    "column": 8,
    "file": "src/lib.rs",
    "line": 43
  },
  "occurrence_kind": "Writes",
  "role": "body",
  "signature_cid": "blake3-512:mem-write-signature"
}
```

### `Io`

Represents filesystem, network, device, sysfs, environment, clock, randomness, or other external interaction.

```json
{
  "args": {
    "channel": "filesystem",
    "operation": "read"
  },
  "discharge_key": "io:filesystem:read",
  "locator": {
    "file": "src/fs.rs",
    "symbol": "load_config"
  },
  "occurrence_kind": "Io",
  "role": "body",
  "signature_cid": "blake3-512:io-signature"
}
```

### `Panics`

Represents explicit or implicit abnormal exit visible to the contract, including panic, abort, trap, unchecked exception, assertion failure, or language equivalent.

```json
{
  "args": {
    "condition": "index_out_of_bounds",
    "mode": "panic"
  },
  "discharge_key": "panic:index_out_of_bounds",
  "locator": {
    "column": 16,
    "file": "src/lib.rs",
    "line": 51
  },
  "occurrence_kind": "Panics",
  "role": "exceptional",
  "signature_cid": "blake3-512:panic-signature"
}
```

### `OpaqueLoop`

Represents a loop site whose weakest precondition or invariant is not locally available.

```json
{
  "args": {
    "loop_cid": "blake3-512:loop-site"
  },
  "discharge_key": "opaque-loop:blake3-512:loop-site",
  "locator": {
    "block": "bb7",
    "file": "src/lib.rs",
    "line": 64
  },
  "occurrence_kind": "OpaqueLoop",
  "role": "invariant",
  "signature_cid": "blake3-512:opaque-loop-signature"
}
```

### `UnresolvedCall`

Represents a direct, indirect, dynamic, reflective, or table driven call whose callee contract is not resolved in the pool.

```json
{
  "args": {
    "name": "ops.decrypt",
    "resolution": "indirect"
  },
  "discharge_key": "unresolved-call:ops.decrypt",
  "locator": {
    "file": "src/crypto.c",
    "line": 88
  },
  "occurrence_kind": "UnresolvedCall",
  "role": "body",
  "signature_cid": "blake3-512:unresolved-call-signature"
}
```

### `AtomicAccess`

Represents an atomic memory access. A known ordering is payload, not provenance. Unknown ordering is represented by `ordering: null` and remains memento required.

```json
{
  "args": {
    "kind": "Rmw",
    "ordering": "SeqCst",
    "target": "counter"
  },
  "discharge_key": "atomic:counter:Rmw:SeqCst",
  "locator": {
    "column": 20,
    "file": "src/lib.rs",
    "line": 101
  },
  "occurrence_kind": "AtomicAccess",
  "role": "body",
  "signature_cid": "blake3-512:atomic-access-signature"
}
```

### `EarlyReturn`

Represents a control flow branch that exits before the ordinary postcondition site, including Rust `?`, exceptions translated into result flow, or language equivalent.

```json
{
  "args": {
    "try_cid": "blake3-512:try-site"
  },
  "discharge_key": "early-return:blake3-512:try-site",
  "locator": {
    "column": 24,
    "file": "src/parse.rs",
    "line": 22
  },
  "occurrence_kind": "EarlyReturn",
  "role": "exceptional",
  "signature_cid": "blake3-512:early-return-signature"
}
```

### `Unsafe`

Represents a Rust `unsafe` block or any equivalent language-level safety escape whose semantics the substrate has no formal model for. Unconditionally blocks composition under v1; classification cannot be lifted without a `schemaVersion` bump (per `2026-05-06-effect-discharge-classification.md` §6).

```json
{
  "args": {
    "kind": "unsafe-block"
  },
  "discharge_key": "unsafe:unsafe-block",
  "locator": {
    "file": "src/ffi.rs",
    "line": 30
  },
  "occurrence_kind": "Unsafe",
  "role": "body",
  "signature_cid": "blake3-512:unsafe-signature"
}
```

### `ClosureCapture`

Represents a closure that captures one or more values from its enclosing scope. Discharged by a matching `ClosureBindingMemento` whose `header.bodyFnCid` equals `args.body_fn_cid`.

```json
{
  "args": {
    "body_fn_cid": "blake3-512:closure-body",
    "n_captures": 2
  },
  "discharge_key": "closure-capture:blake3-512:closure-body:2",
  "locator": {
    "file": "src/iter.rs",
    "line": 17
  },
  "occurrence_kind": "ClosureCapture",
  "role": "body",
  "signature_cid": "blake3-512:closure-capture-signature"
}
```

### `PinnedReference`

Represents an access to a pinned-storage target (Rust `Pin<&mut T>` or language equivalent). Discharged by a matching `PinInvariantMemento` whose `(function_cid, pinned_target)` pair matches and whose `invariant` is non-empty.

```json
{
  "args": {
    "target": "self.buf"
  },
  "discharge_key": "pinned-reference:self.buf",
  "locator": {
    "file": "src/future.rs",
    "line": 88
  },
  "occurrence_kind": "PinnedReference",
  "role": "body",
  "signature_cid": "blake3-512:pinned-reference-signature"
}
```

### `RawPointerProvenance`

Represents a raw-pointer construction or use whose provenance the substrate needs declared. Discharged by a matching `ProvenanceMemento` whose `header.target` equals `args.target`.

```json
{
  "args": {
    "mutable": true,
    "target": "buf"
  },
  "discharge_key": "raw-pointer-provenance:buf:mut",
  "locator": {
    "file": "src/alloc.rs",
    "line": 41
  },
  "occurrence_kind": "RawPointerProvenance",
  "role": "body",
  "signature_cid": "blake3-512:raw-pointer-provenance-signature"
}
```

### `PossibleAliasing`

Represents an inter-formal aliasing relationship the lifter cannot disprove. Discharged when every unordered pair in `args.formals` has a matching `AliasingMemento`.

```json
{
  "args": {
    "formals": ["dst", "src"]
  },
  "discharge_key": "possible-aliasing:dst,src",
  "locator": {
    "file": "src/copy.rs",
    "line": 14
  },
  "occurrence_kind": "PossibleAliasing",
  "role": "pre",
  "signature_cid": "blake3-512:possible-aliasing-signature"
}
```

### `Drop`

Represents a drop site for a typed value. Discharged by a matching `DropMemento` indexed under `(function_cid, target_type)`; trivial/structural drops classify informational and do not block composition. The `name` in `args` MUST resolve to the canonical `def_id` form used by the lifter's IR before being compared against pool keys (per the discharge-classification spec §1.2).

```json
{
  "args": {
    "name": "std::vec::Vec<u8>"
  },
  "discharge_key": "drop:std::vec::Vec<u8>",
  "locator": {
    "file": "src/buf.rs",
    "line": 73
  },
  "occurrence_kind": "Drop",
  "role": "body",
  "signature_cid": "blake3-512:drop-signature"
}
```

New occurrence kinds MUST define:

1. the `occurrence_kind` string,
2. the `args` schema,
3. the `discharge_key` derivation,
4. the default classification interaction in §4 terms,
5. at least one JCS ordered example.

## §4. Classification Interaction

Classification is evaluated over the full `EffectOccurrence`, not over `signature_cid` alone.

The v1 mapping to `2026-05-06-effect-discharge-classification.md` is:

| Occurrence kind | Classification | Rule |
|---|---|---|
| `Reads` | block | Any concrete read occurrence blocks pure composition unless a future effect aware composition rule explicitly admits the target. |
| `Writes` | block | Any concrete write occurrence blocks pure composition unless a future effect aware composition rule explicitly admits the target. |
| `Io` | block | External interaction blocks pure composition. The `args.channel` and `args.operation` values are diagnostic and policy inputs. |
| `Panics` | block | Abnormal exit blocks pure composition unless a future memento supplies a mechanically checked no panic proof for the concrete condition. |
| `OpaqueLoop` | memento required | Discharged by a valid `LoopInvariantMemento` whose `header.loopCid` equals `args.loop_cid`. |
| `UnresolvedCall` | memento required | Discharged only when the callee contract or future call resolution memento matches the concrete `args.name` and locator. With no such memento kind available, it fails closed. |
| `AtomicAccess` with `ordering: null` | memento required | Discharged by a valid `AtomicOrderingMemento` matching `(args.target, args.kind, resolved ordering)`. |
| `AtomicAccess` with concrete `ordering` | informational dischargeable | The occurrence is fully specified. It remains in `effects` for audit and transport equality, but it does not block composition by itself. |
| `EarlyReturn` | memento required | Discharged by a valid `TryBranchMemento` whose `header.tryCid` equals `args.try_cid`. |
| `Unsafe` | block | Unconditionally blocks pure composition under v1. The classification cannot be lifted by any v1 memento; reclassifying it requires a `schemaVersion` bump per the discharge-classification spec §6. |
| `ClosureCapture` | memento required | Discharged by a valid `ClosureBindingMemento` whose `header.bodyFnCid` equals `args.body_fn_cid`. |
| `PinnedReference` | memento required | Discharged by a valid `PinInvariantMemento` whose `(function_cid, header.pinnedTarget)` pair matches `args.target` and whose `invariant` is non-empty. |
| `RawPointerProvenance` | memento required | Discharged by a valid `ProvenanceMemento` whose `header.target` equals `args.target`. The `args.mutable` flag is a diagnostic and policy input. |
| `PossibleAliasing` | memento required | Discharged when every unordered pair in `args.formals` has a matching `AliasingMemento`. Partial coverage fails closed. |
| `Drop` with non-trivial drop kind | memento required | Discharged by a valid `DropMemento` indexed under `(function_cid, target_type)` where `target_type` is the canonical `def_id` form of `args.name`. |
| `Drop` with trivial/structural drop kind | informational dischargeable | Trivial and structural drops do not block composition; the occurrence stays in `effects` for audit and transport equality. |

The composition guard MUST apply the existing order from effect discharge classification:

1. Check memento required occurrences first and fail fast on the first missing discharge.
2. Then refuse on remaining block occurrences.
3. Then admit informational dischargeable occurrences.

The refusal payload in CompositionRefusalMemento #795 MUST include the complete `EffectOccurrence`, not only `occurrence_kind` or `signature_cid`. This preserves the concrete target, locator, and discharge key needed to fix the refusal.

## §5. Cross References

- `2026-05-09-language-signature-protocol.md`: defines `EffectSignatureMemento` as catalog algebra. `EffectOccurrence.signature_cid` points into that catalog.
- `2026-05-09-contract-composition-protocol.md`: defines composition refusal for impure inputs. This spec replaces bare effect set entries with occurrence objects while preserving the refuse unless classified discipline.
- `2026-05-06-effect-discharge-classification.md`: defines block, memento required, and informational categories. This spec states that the classifier consumes occurrences.
- admissibility spine #796: admissibility checks MUST consider occurrence payloads when deciding whether a contract can enter a composition spine.
- CompositionRefusalMemento #795: refusal records MUST carry the blocking occurrence so producers can mint the correct discharge or relift with the correct semantic payload.

## §6. Migration

Legacy `FunctionContractMemento.effects` arrays of bare CIDs fail closed for semantic consumers.

A consumer that needs storage compatibility MAY synthesize:

```json
{
  "args": {
    "unknown": true
  },
  "discharge_key": "legacy-cid:<cid>",
  "locator": {},
  "occurrence_kind": "LegacyUnknown",
  "role": "body",
  "signature_cid": "<cid>"
}
```

The synthesized occurrence is storage compatibility only. It MUST NOT be treated as discharged, pure, or informational. Composition, admissibility, and transport semantic consumers MUST classify it as memento required with no available discharge, which fails closed.

Relifting is the normative migration path. Relifters MUST promote each observed source fact into a concrete `EffectOccurrence` with a canonical `occurrence_kind`, concrete `args`, and a stable `discharge_key`.

## §7. Out of Scope

- Effect handler protocols that interpret, commute, mask, resume, or algebraically compose effect occurrences.
- Observer effect propagation from instrumentation, monitoring, test wrappers, foreign function shims, or host harnesses. Those effects belong to wrapper contracts, not to the wrapped program's `FunctionContractMemento`.
