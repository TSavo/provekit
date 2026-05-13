# Sort Morphism Memento

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- `2026-05-09-language-signature-protocol.md`
- `2026-05-12-plugin-protocol.md`
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md`
- `menagerie/<lang>-language-signature/specs/sort_*.spec.json`
- `menagerie/concept-shapes/catalog/realizations/`
- admissibility-spine umbrella issue #796

## §0. Purpose

ProvekIt's concept hub federates semantics across languages. A `concept:option` or `concept:list` realization says how an operation or abstraction in one language corresponds to the hub, and existing realization equations record the loss dimensions that make that correspondence exact, lossy, or refused.

Types and sorts need the same substrate layer. Rust `i64`, Java `long`, C `int` on a selected ABI, and Python `int` are not just spelling variants. They have value sets, precision behavior, runtime representations, and guard requirements that can agree in one direction while disagreeing in another. Recording that information only inside catalog realization-equation `loss_record` entries makes type transport implicit and hard to compose.

`SortMorphismMemento` is the sort-level counterpart to concept realization mementos. It is a content-addressed statement that one sort can be transported to another sort with declared direction, precision loss, range loss, representation constraints, and runtime guards. Consumers use it when mapping signatures, checking transport gaps, and deciding whether a program can move across a language boundary without silently changing the domain of values.

### §0.1 Two-stage composition with parametric concept realization

`SortMorphismMemento` is intentionally narrow in scope. It describes ONE sort mapping. Parametric concept realizations such as `concept:option<T> → java:Optional<U>` compose with sort morphisms in TWO STAGES, not as pre-instantiated catalog entries:

1. Look up the parametric concept realization: `concept:option<T> → java:Optional<U>` (one equation, with type-variable slots `T` and `U`).
2. Look up the sort morphism for the concrete type substitution: e.g., `i64 → long` (one `SortMorphismMemento`).
3. The composed realization for `concept:option<i64> → java:Optional<long>` is `(parametric_realization_cid, [sort_morphism_cid])`, NOT a separately minted equation.

The substrate avoids catalog explosion by composing these on demand rather than pre-minting one equation per `(concept × sort-tuple)`. The parametric concept realization machinery is specified separately in `ParametricRealizationMemento` and `RealizationPlanMemento` (TSavo/provekit#801, deferred). This spec covers the sort-side only; composition lives in the realization spec.

In particular, this spec MUST be readable in isolation: a `SortMorphismMemento` is a self-contained statement about two sorts, with no embedded reference to concepts. Concept-level orchestrators read `SortMorphismMemento` instances to populate the slots of a parametric realization plan; the morphism does not know it will be used that way.

## §1. Wire shape (CDDL)

```cddl
; Shared scalar types:
;   cid, iso8601, json-value, pubkey, signature
;
; Locked JCS key order: alphabetical within each object.
; Layer order is envelope, header, metadata.

sort-morphism-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,             ; over JCS({header, metadata})
    signer:     pubkey
  },
  header: {
    cid:                          cid,    ; DERIVED, see §7
    direction:                    morphism-direction,
    kind:                         "sort-morphism",
    precision_loss:               precision-loss,
    range_loss:                   range-loss,
    representation_constraints:   [* representation-constraint],
    runtime_guards:               [* runtime-guard],
    schemaVersion:                "1",
    source_language_signature_cid: cid,   ; LanguageSignatureMemento pinning the ABI/language version the source sort lives in
    source_sort_cid:              cid,
    target_language_signature_cid: cid,   ; LanguageSignatureMemento pinning the ABI/language version the target sort lives in
    target_sort_cid:              cid
  },
  metadata: {
    ? note:       tstr,
    ? source_url: tstr
  }
}

morphism-direction = "bidirectional" / "left-to-right" / "right-to-left"

precision-loss = "none" / "lossy-rounding" / "lossy-truncation" / tstr

range-loss = "none" / "narrowing" / "widening" / "saturating" / "wrapping" / tstr

representation-constraint = {
  kind:    tstr,
  ? param: json-value
}

runtime-failure-mode = "panic" / "saturate" / "wrap" / "refuse"

runtime-guard = {
  ? failure_mode: runtime-failure-mode,
  kind:           tstr,
  ? predicate:    tstr
}
```

## §2. Field semantics

| Layer | Field | Required | Meaning |
|---|---|---:|---|
| envelope | `declaredAt` | yes | ISO-8601 UTC minting timestamp. |
| envelope | `signature` | yes (swarm) | Ed25519 signature over `JCS({header, metadata})`. Local-only dev mementos MAY use an unsigned sentinel if the consuming catalog permits it. |
| envelope | `signer` | yes | Minter public key. |
| header | `cid` | yes | Content CID of this memento, DERIVED per §7. |
| header | `direction` | yes | The direction in which the sort transport relation is valid. See §3. |
| header | `kind` | yes | MUST be `"sort-morphism"`. |
| header | `precision_loss` | yes | Whether mapped values lose numerical or representational precision without necessarily leaving the target range. See §4. |
| header | `range_loss` | yes | Whether mapped values lose, gain, clamp, or wrap admissible value range. See §4. |
| header | `representation_constraints` | yes | Runtime or ABI facts that MUST hold for the morphism to be valid. Empty means no extra representation premise beyond the referenced sort mementos. |
| header | `runtime_guards` | yes | Checks or conversions a consumer MUST emit or prove before using this morphism when the morphism is not statically total. Empty means no guard is required. |
| header | `schemaVersion` | yes | MUST be `"1"`. |
| header | `source_language_signature_cid` | yes | CID of the `LanguageSignatureMemento` (per `2026-05-09-language-signature-protocol.md`) pinning the language version and ABI within which `source_sort_cid` is interpreted. This anchor closes the version/ABI question for the source side: a Rust 1.75 `i64` and a Rust 2027-edition `i64` are different sorts under different signatures, so morphisms over each must be distinct mementos. |
| header | `source_sort_cid` | yes | CID of the sort named on the left side of the memento. Interpreted under `source_language_signature_cid`. |
| header | `target_language_signature_cid` | yes | CID of the `LanguageSignatureMemento` pinning the language version and ABI within which `target_sort_cid` is interpreted. |
| header | `target_sort_cid` | yes | CID of the sort named on the right side of the memento. Interpreted under `target_language_signature_cid`. |
| metadata | `note` | no | Human-readable note. Omitted when absent. |
| metadata | `source_url` | no | Source document or catalog URL supporting the declaration. Omitted when absent. |

All object keys MUST be JCS-canonicalized in alphabetical order. Producers MUST emit empty arrays, not omitted fields, for `representation_constraints` and `runtime_guards` when there are no entries.

## §3. Direction semantics

`direction = "left-to-right"` means the valid map is from `source_sort_cid` to `target_sort_cid`. Consumers MUST NOT use the same memento in the reverse direction. If the reverse map exists with different losses or guards, it MUST be minted as a separate memento or represented by another memento whose direction permits that reverse use.

`direction = "right-to-left"` means the valid map is from `target_sort_cid` to `source_sort_cid`. This exists for catalogs that store pair facts in a stable source/target order while the proved transport direction runs the other way. New producer code SHOULD prefer swapping source and target and using `"left-to-right"` unless there is a catalog-indexing reason to preserve the pair order.

`direction = "bidirectional"` means the relation is valid in both directions under the same declared losses, constraints, and guards. It is appropriate for equal-width integer aliases such as Rust `i64` and Java `long` when the sort mementos agree on signedness and two's-complement representation. It is not appropriate when one direction widens and the other narrows, such as Rust `i64` and Python `int`; that pair needs directional facts.

Direction is part of the CID input. Changing direction, even with identical sort CIDs and loss labels, mints a different memento.

## §4. Loss dimensions

`precision_loss` and `range_loss` are separate dimensions. A morphism can preserve precision while widening range, such as Rust `i64` to Python `int`. A morphism can preserve range while losing precision, such as a decimal or rational sort into a binary floating sort over a bounded interval. Producers MUST NOT encode one dimension's loss by changing the other dimension.

### §4.1 Precision loss canonical values

| Value | Meaning |
|---|---|
| `none` | The target can represent every source value in the morphism domain without changing the mathematical value. |
| `lossy-rounding` | The target rounds values to a nearby representable value. This is typical for real, decimal, or rational values mapped to floating sorts. |
| `lossy-truncation` | The target discards lower-order or fractional information. This is typical for float-to-int, decimal-scale reduction, and fixed-point scale reduction. |
| open `tstr` | Extension label. Producers MUST document the semantics in metadata or a companion spec. Consumers that do not understand the label MUST treat it as non-`none`. |

### §4.2 Range loss canonical values

| Value | Meaning |
|---|---|
| `none` | The source and target value domains coincide for the declared direction. |
| `narrowing` | Some source values are outside the target domain. The morphism requires a proof or a runtime guard to avoid silent loss. |
| `widening` | Every source value is in the target domain, and the target admits additional values not present in the source. |
| `saturating` | Out-of-range values are clamped to target bounds. This is a lossy totalization, not an exact narrowing guard. |
| `wrapping` | Out-of-range values are mapped modulo the target representation. This is a lossy totalization, not an exact narrowing guard. |
| open `tstr` | Extension label. Producers MUST document the semantics in metadata or a companion spec. Consumers that do not understand the label MUST treat it as non-`none`. |

### §4.3 Composition under transport

Sort transport composes along paths such as `source language sort -> concept hub sort -> target language sort`. Composition MUST be conservative:

1. `none` is the identity in each dimension.
2. If every edge has `precision_loss = "none"`, the composed precision loss is `none`; otherwise it is non-`none`.
3. If every edge has `range_loss = "none"`, the composed range loss is `none`; otherwise it is non-`none`.
4. `widening` followed only by `widening` or `none` composes to `widening`.
5. `narrowing` anywhere in the path composes to at least `narrowing` unless the consumer recomputes the exact source-to-target value relation from sort denotations and proves no source value is excluded.
6. `saturating` or `wrapping` anywhere in the path remains visible in the composed result. If both occur, the composed value MUST use an open label such as `"compound:saturating+wrapping"` or the consumer MUST refuse.
7. Multiple non-`none` precision modes that are not captured by a canonical value MUST use an open compound label or refuse.

Consumers MAY recompute a tighter composed loss from the referenced sort mementos and guard predicates. They MUST NOT report a composed loss that is less restrictive than any unproven edge in the path.

## §5. Representation constraints

`representation_constraints` lists facts that must hold at runtime, ABI selection time, or language-version selection time. A consumer MAY discharge a constraint statically from the referenced sort mementos. If it cannot discharge a required constraint, it MUST refuse to use the morphism.

Canonical v1.0.0 constraint kinds:

| Kind | Meaning | Typical `param` |
|---|---|---|
| `bit-width-equal` | Source and target use the same storage width for the mapped value. | `64` or `{ "bits": 64 }` |
| `bit-width-at-least` | Target width is at least source width for the declared direction. | `{ "source_bits": 32, "target_bits": 64 }` |
| `endianness-fixed` | Byte-level transport assumes a fixed byte order. This is only relevant when the morphism exposes bytes or layout, not for pure mathematical integers. | `"little"` or `"big"` |
| `ieee754` | Floating representation follows IEEE 754 for the declared format. | `"binary32"`, `"binary64"`, or `{ "format": "binary64" }` |
| `signedness-equal` | Source and target agree on signed versus unsigned interpretation. | `"signed"` or `"unsigned"` |
| `two's-complement` | Signed fixed-width integers use two's-complement representation. | `true` or `{ "bits": 64 }` |
| `utf-8` | String or byte transport requires valid UTF-8 at the boundary. | `true` |

The `kind` enum is open. Unknown kinds are accepted at shape level but not automatically discharged. A consumer that cannot prove an unknown representation constraint MUST refuse rather than silently assuming it.

## §6. Runtime guards

`runtime_guards` records checks or conversions that the target-side emitter must insert, or prove already holds, before it may use the morphism. A guard makes the boundary explicit; it does not erase the loss label.

Canonical guard kinds include:

| Kind | Use | Failure modes |
|---|---|---|
| `range-check` | Check that the source value fits the target range before narrowing. | `panic`, `refuse` |
| `null-check` | Check that a nullable source value is not null before mapping into a non-null target sort. | `panic`, `refuse` |
| `saturating-cast` | Clamp out-of-range source values to target bounds. | `saturate` |
| `wrapping-cast` | Map out-of-range source values modulo target width. | `wrap` |
| `panic-on-overflow` | Emit target code that traps when a conversion or arithmetic operation overflows. | `panic` |

`predicate`, when present, names the IR formula or human-readable predicate the guard enforces. A producer SHOULD use an IR formula reference when one exists. Plain text predicates are permitted for draft mementos but are not enough for automatic discharge.

The `failure_mode` controls how the consumer treats failed guard evaluation:

| Failure mode | Meaning |
|---|---|
| `panic` | Emitted target code aborts, throws, or traps at the boundary. |
| `saturate` | Emitted target code clamps to a bound. This requires `range_loss = "saturating"` or an open equivalent. |
| `wrap` | Emitted target code wraps modulo representation width. This requires `range_loss = "wrapping"` or an open equivalent. |
| `refuse` | The transport pipeline refuses unless it can prove the predicate statically. |

If `runtime_guards` is non-empty, the emitter MUST either insert all listed guards in the declared direction or prove their predicates before emission. It MUST NOT drop guards because the value appears safe in examples or tests.

## §7. CID construction

The `cid` is the BLAKE3-512 digest of the JCS-canonical bytes of the `header` object with `cid` elided:

```text
cid_input = JCS({
  "direction":                  <direction>,
  "kind":                       "sort-morphism",
  "precision_loss":             <precision_loss>,
  "range_loss":                 <range_loss>,
  "representation_constraints": <representation_constraints>,
  "runtime_guards":             <runtime_guards>,
  "schemaVersion":              "1",
  "source_sort_cid":            <source_sort_cid>,
  "target_sort_cid":            <target_sort_cid>
})

cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

All header fields except `cid` itself are part of the CID input. Metadata and envelope fields are not part of the CID. The envelope signature covers `JCS({header, metadata})`, so a signer attests both the derived CID and the non-CID metadata.

Two mementos that differ only by `precision_loss`, `range_loss`, a guard predicate, or a representation constraint MUST have different CIDs.

## §8. Cross-references

- `protocol/specs/2026-05-09-language-signature-protocol.md` defines `SortMemento` and `LanguageSignatureMemento`. `source_sort_cid` and `target_sort_cid` point at sort mementos in that layer.
- `protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md` defines the transport trichotomy and multidimensional loss records consumed by cross-language migration. `SortMorphismMemento` supplies the type and sort side of the same composition story.
- `menagerie/<lang>-language-signature/specs/sort_*.spec.json` contains the in-tree sort declarations that become source and target referents for these mementos.
- `menagerie/concept-shapes/catalog/realizations/` contains existing concept realization equations. This spec is intentionally parallel to those operation and abstraction realization facts, but scoped to sorts.
- The admissibility-spine umbrella issue #796 is the integration point for deciding when a language has enough sort and concept coverage to be admitted as a transport target.

## §9. Examples

Example CIDs are illustrative and not recomputed.

### §9.1 Rust `i64` to and from Java `long`

```json
{
  "envelope": {
    "declaredAt": "2026-05-13T17:00:00Z",
    "signature": "ed25519:UNSIGNED_DEV_ONLY",
    "signer": "ed25519:foundation-v0"
  },
  "header": {
    "cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "direction": "bidirectional",
    "kind": "sort-morphism",
    "precision_loss": "none",
    "range_loss": "none",
    "representation_constraints": [
      {
        "kind": "bit-width-equal",
        "param": 64
      },
      {
        "kind": "signedness-equal",
        "param": "signed"
      },
      {
        "kind": "two's-complement",
        "param": true
      }
    ],
    "runtime_guards": [],
    "schemaVersion": "1",
    "source_language_signature_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "source_sort_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "target_language_signature_cid": "blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555",
    "target_sort_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "metadata": {
    "note": "Both sorts denote signed 64-bit two's-complement integers under their respective pinned language signatures (rust 1.75 LP64 / java 17).",
    "source_url": "menagerie/rust-language-signature/specs/sort_int.spec.json"
  }
}
```

### §9.2 Rust `i64` to Python `int`

```json
{
  "envelope": {
    "declaredAt": "2026-05-13T17:05:00Z",
    "signature": "ed25519:UNSIGNED_DEV_ONLY",
    "signer": "ed25519:foundation-v0"
  },
  "header": {
    "cid": "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
    "direction": "left-to-right",
    "kind": "sort-morphism",
    "precision_loss": "none",
    "range_loss": "widening",
    "representation_constraints": [
      {
        "kind": "two's-complement",
        "param": {
          "source_bits": 64
        }
      }
    ],
    "runtime_guards": [],
    "schemaVersion": "1",
    "source_language_signature_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "source_sort_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "target_language_signature_cid": "blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777",
    "target_sort_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "metadata": {
    "note": "The reverse direction is narrowing and requires a separate guarded morphism. The current Python draft signature records Python values in sort_value.spec.json; a precise int sort can refine that target CID.",
    "source_url": "menagerie/python-language-signature/specs/sort_value.spec.json"
  }
}
```

### §9.3 Rust `i32` to Rust `i16`

```json
{
  "envelope": {
    "declaredAt": "2026-05-13T17:10:00Z",
    "signature": "ed25519:UNSIGNED_DEV_ONLY",
    "signer": "ed25519:foundation-v0"
  },
  "header": {
    "cid": "blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
    "direction": "left-to-right",
    "kind": "sort-morphism",
    "precision_loss": "none",
    "range_loss": "narrowing",
    "representation_constraints": [
      {
        "kind": "two's-complement",
        "param": {
          "source_bits": 32,
          "target_bits": 16
        }
      }
    ],
    "runtime_guards": [
      {
        "failure_mode": "panic",
        "kind": "range-check",
        "predicate": "source_value >= -32768 && source_value <= 32767"
      }
    ],
    "schemaVersion": "1",
    "source_language_signature_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "source_sort_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "target_language_signature_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "target_sort_cid": "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
  },
  "metadata": {
    "note": "The range-check makes failed narrowing explicit. A consumer that cannot emit or prove the guard must refuse. Source and target language signatures are identical (both rust 1.75) since this is an intra-language morphism.",
    "source_url": "menagerie/rust-language-signature/specs/sort_int.spec.json"
  }
}
```

## §10. Out of scope

- Generic-instantiation morphisms. Mapping `List<T>` to another `List<U>` depends on concept-hub instantiation and element-sort morphisms, not this ground sort memento alone.
- Structural-type subtyping. Width, depth, row, and nominal subtyping need a separate structural relation over sort descriptions.
- Pointer-sort morphisms. Address, reference, lifetime, aliasing, and provenance claims need a separate pointer-sort spec with memory-model constraints.
