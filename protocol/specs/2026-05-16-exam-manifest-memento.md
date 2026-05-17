# ExamManifestMemento Normative Spec

**Status:** v1.1.0 normative draft. v1 manifests remain parseable for backward compatibility. Catalog entry to be appended in follow-up CI mint.
**Date:** 2026-05-16
**Author:** T Savo
**Related:**
- `2026-05-12-sugar-dict-memento.md`
- `2026-05-13-promotion-decision-memento.md`
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md`
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization)
- `2026-04-30-ir-formal-grammar.md` (IR formal grammar)
- `2026-04-30-memento-envelope-grammar.md` (memento envelope grammar)
- `2026-04-30-protocol-catalog.json` (protocol catalog)
- `2026-05-12-plugin-protocol.md` (PEP 1.7.0; consumer specification per PEP §2.1 plugin-kind enum)
- `2026-05-09-algorithm-memento-protocol.md` (AMP)
- `2026-05-09-language-signature-protocol.md` (LSP)
- `docs/audits/2026-05-12-concept-library-completeness-probe.md`
- `docs/audits/2026-05-12-concept-library-completeness-probe-operation-layer.md`
- `docs/papers/12-after-languages-how-proofir-represents-every-language.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`

## §0 Purpose

`ExamManifestMemento` makes the substrate exam explicit. It is the stable,
content-addressed question set the substrate considers part of canonical
coverage for one concept hub version.

The manifest is not coverage. The manifest records the questions. Coverage is
dynamic and records which questions are answered, refused, or still open. At the
question boundary the trichotomy is:

| State | Meaning |
| --- | --- |
| answered | A catalog answer CID exists for the question. |
| refused | A gap or refusal memento exists for the question. |
| open | No answer or refusal is recorded yet. |

For v1.1.0 the manifest carries only the question set and each question's
`expected_answer_shape`. Per-question `answered_at` and `refused_at` tracking is
deferred to a separate `ExamCoverageMemento` family. This keeps manifest CIDs
stable while coverage evolves.

## §1 The Memento

The wire shape is the standard envelope, header, metadata shape. The header
contains `content`; metadata declares the schema version. The `cid` is derived
from metadata plus content as specified in §5.

```cddl
exam-manifest-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,
    signer:     pubkey
  },
  header: {
    cid:     cid,       ; DERIVED
    content: exam-manifest-content
  },
  metadata: {
    schemaVersion: exam-manifest-schema-version
  }
}

exam-manifest-schema-version =
    "provekit-exam-manifest/v1.1" /
    "provekit-exam-manifest/v1"

exam-manifest-content = {
  concept_hub_version: tstr,
  question_kinds:      [+ question-kind],
  questions:           [+ exam-question]
}

exam-question = {
  concept:               tstr,
  expected_answer_shape: tstr,
  kind:                  question-kind,
  parameters:            { * tstr => any }
}

question-kind = v1-1-question-kind / v1-question-kind

v1-1-question-kind =
    "concept-realization" /
    "boundary-realization" /
    "boundary-tag" /
    "sort-classification" /
    "effect-classification" /
    "morphism" /
    "composition"

v1-question-kind =
    "realization" /
    "sort" /
    "effect"
```

Objects use alphabetical JCS key order per
`2026-04-30-canonicalization-grammar.md`. Producers MUST emit canonical bytes;
consumers MUST canonicalize before hashing or signature verification.

## §2 The Question-Kind Enum

The v1.1 canonical labels are:

- `concept-realization`
- `boundary-realization`
- `boundary-tag`
- `sort-classification`
- `effect-classification`
- `morphism`
- `composition`

The v1 labels `realization`, `sort`, and `effect` remain parseable only when
`metadata.schemaVersion` is `provekit-exam-manifest/v1`. v1.1 producers MUST
emit the canonical labels above and MUST NOT emit the v1 labels.

The enum is open per PEP 1.7.0 §2.1. Shape-level validators accept unknown
kinds. The consumer that receives an unknown kind decides whether to refuse it,
and records that refusal at the consumer boundary rather than failing the
manifest shape.

## §3 Per-Question Parameters Schema Per Kind

The envelope keeps `parameters` as an open map for forward compatibility, but
v1.1 producers MUST use the per-kind schemas below. `expected_answer_shape`
names the memento family that answers the question.

### §3.1 `concept-realization`

Question: for this `(concept, language)` pair, is there a realization tag, which
tag kind is it, and what data is needed to emit it?

```cddl
concept-realization-parameters = {
  language: tstr
}

realization-tag-kind =
    "first-class" /
    "composition" /
    "boundary" /
    "sugar-carrier"
```

Expected answer shape: `RealizationMemento`. The answer carries the tag-kind
enum above and tag-kind-specific data. This replaces the v1 cross-product
`realization` question.

### §3.2 `boundary-realization`

Question: for this `(boundary contract, target language, target library)` tuple,
how does the native library realize the boundary contract?

```cddl
boundary-realization-parameters = {
  target_language:       tstr,
  target_library:        tstr,
  boundary_contract_cid: cid
}
```

Expected answer shape: `BoundaryRealizationMemento`. A v1.1 producer MUST only
emit this question when `target_library` is native to `target_language`; it MUST
not create cross-product questions for libraries from other languages.

### §3.3 `boundary-tag`

Question: does the library API bind to the target boundary contract?

```cddl
boundary-tag-parameters = {
  library:                  tstr,
  api:                      tstr,
  target_boundary_contract: cid
}
```

Expected answer shape: `BoundaryTagMemento`.

### §3.4 `sort-classification`

Question: how does the language classify the concept sort?

```cddl
sort-classification-parameters = {
  language: tstr
}
```

Expected answer shape: `SortMorphismMemento`.

### §3.5 `effect-classification`

Question: how does the language classify the concept effect category?

```cddl
effect-classification-parameters = {
  language: tstr
}
```

Expected answer shape: `EffectSignatureMemento`.

### §3.6 `morphism`

Question: how does `from_language` express the concept?

```cddl
morphism-parameters = {
  from_language: tstr
}
```

Expected answer shape: `MorphismMemento`.

### §3.7 `composition`

`composition` is reserved in the v1.1 enum for compatibility with the substrate
question taxonomy, but composition questions are deferred to v2.0. v1.1
producers MUST emit zero questions whose `kind` is `composition`.

For unknown kinds, `parameters` remains an open map. The consumer that decides
whether to refuse the kind also decides the parameters schema it understands.

## §4 Canonicalization

Canonicalization is JCS per `2026-04-30-canonicalization-grammar.md`. Within
each object, keys are ordered alphabetically by the JCS encoder.

Before producing the CID input:

1. Sort `question_kinds[]` lexicographically.
2. Sort `questions[]` by `(kind, concept, JCS-sorted parameters, expected_answer_shape)`.
3. Encode the payload object `{content: ..., metadata: ...}` as JCS.
4. Hash those bytes with BLAKE3-512.

The `parameters` component in the sort key is the JCS byte string of the
parameters map, not a host runtime's map iteration order.

## §5 CID Semantics

`header.cid` is derived from BLAKE3-512 over JCS of metadata plus content, per
the standard envelope pattern:

```text
cid_input = JCS({
  "content":  <exam-manifest-content with sorted arrays>,
  "metadata": {"schemaVersion": "provekit-exam-manifest/v1.1"}
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

The envelope signature is over the same JCS bytes. Two manifests with
byte-identical content payloads and metadata MUST produce byte-identical CIDs
even when produced by different runtimes or delivered by different transports.

## §6 Federation

Two consumer runtimes federate on this manifest when they pin to the same
manifest CID. Different manifest CIDs require explicit translation. That
translation is deferred to a future `ExamManifestTranslationMemento` family or
equivalent.

The PEP 1.7.0 plugin loading protocol gains an `exam-manifest-cid` handshake in
issue #1108. This spec defines the manifest shape that handshake checks.

## §7 Plugin-kind specification

The ExamManifestMemento is a `pep/1.7.0` plugin-memento (per
`2026-05-12-plugin-protocol.md`) with `kind = "exam-manifest"`. The plugin's
declared-behavior CID is the manifest CID per §5 of this spec.

Plugin discovery follows PEP 1.7.0 §3: a runtime resolves an `exam-manifest`
plugin via `~/.config/provekit/exam-manifest/<name>/manifest.toml` or
`.provekit/exam-manifest/<name>/manifest.toml`. Project-local shadows
user-global, consistent with `lift` and `realize` plugins.

Plugin manifest TOML schema:

```toml
name = "<plugin-name>"
version = "1.0.0"
protocol_versions = ["pep/1.7.0"]
command = ["<argv-to-spawn-plugin>"]
working_dir = "<optional path relative to workspace root>"

[capabilities]
kind = "exam-manifest"
exam_manifest_schema_version = "provekit-exam-manifest/v1.1"
```

A runtime MAY compile in a default built-in `exam-manifest` plugin per PEP
1.7.0 §0.1 "a built-in default is still a memento at a fixed declared CID."
Substrate v1.1.0 ships a built-in that loads manifest JSON from a local file
path or from a catalog CID lookup; third-party plugins MAY emit manifests via
JSON-RPC.

Plugin RPC: invoke method is `provekit.plugin.invoke` per PEP 1.7.0 §4.
Request payload is `{path: tstr}` or `{cid: tstr}` (one or the other; both is
a refusal). Response payload is the canonical ExamManifestMemento JSON per §1
of this spec.

## §8 Versioning

`schemaVersion` is `provekit-exam-manifest/v1.1` for current manifests. The
previous `provekit-exam-manifest/v1` format remains parseable for backward
compatibility, but v1.1 producers MUST emit the refined v1.1 question kinds.
Future versions such as `/v2` are new schemas with their own CIDs. Backward
compatibility commitments follow `2026-04-30-protocol-versioning.md`.

## §9 Trichotomy At The Question Boundary

| Outcome | Condition |
| --- | --- |
| `exact` (answered) | A morphism, concept-realization, boundary-realization, boundary-tag, sort-classification, effect-classification, or future composition memento exists in the catalog with byte-identical signature to the question's `parameters` and `expected_answer_shape`. The catalog index or audit tool decides this. |
| `loudly-bounded-lossy` (answered with characterized divergence) | The answer memento exists but carries a non-empty `loss_record_contribution` per `2026-05-14-transport-gap-and-partial-morphism-protocol.md`. |
| `refuse` | The answer memento does not exist, and a `TransportGapMemento` exists in `concept-shapes/gaps/` citing this question in future issue #1106, or no gap record exists yet. For v1.1.0, open is the default; cited refusal is the loud refusal. |

The manifest itself does not decide which state applies. It only gives every
consumer the same question identity to cite.

## §10 Out Of Scope For v1.1.0

- `ExamCoverageMemento` and per-question `answered_at` or `refused_at` tracking. This is decoupled per architect ruling; the manifest carries only the question set without coverage state.
- Cross-manifest translation mementos. Federation handshake refuses on CID mismatch; explicit translation is future work.
- Auto-generation of manifest entries from concept hub shape specs. The generator tool is issue #1105; this spec describes only the shape the generator must produce.
- Per-question discharge tracking. Each answer's refinement obligation discharge is tracked via existing `catalog/receipts/`; the manifest only references receipt CIDs once answers are minted, via the future `ExamCoverageMemento` family.
- Composition question emission. The `composition` label is reserved, but v1.1 producers emit no composition questions. The parameter contract is deferred to v2.0.

## §11 Schema v1.1 Amendment

This amendment follows `docs/plans/2026-05-17-realization-tag-kinds-and-marketplace-ruling.md`.

R11 narrows the exam manifest to structural substrate coverage. The manifest
asks one `concept-realization` question per `(concept, language)` pair. Vendor
specific sugar dictionaries, witness policy, and IDE rendering stay outside the
manifest and remain plugin or consumer concerns.

R12 moves the realization tag kind into the answer. `RealizationMemento` is the
tagged answer shape for `concept-realization`, with tag kinds `first-class`,
`composition`, `boundary`, and `sugar-carrier`. The manifest no longer fans out
`realization` questions across every `(target_language, target_library)` pair.

The `schemaVersion` for current manifests is `provekit-exam-manifest/v1.1`.
Consumers that already parse `provekit-exam-manifest/v1` MUST continue to parse
that version for backward compatibility, but v1 producers and v1.1 producers
produce different manifest CIDs because metadata participates in the CID input.

The narrowed v1.1 question categories are:

- `concept-realization`: per `(concept, language)`; parameters `{language: tstr}`; answer shape `RealizationMemento`.
- `boundary-realization`: per `(boundary contract, target language, target library)`; parameters `{target_language: tstr, target_library: tstr, boundary_contract_cid: cid}`; answer shape `BoundaryRealizationMemento`.
- `boundary-tag`: per `(library, api, target boundary contract)`; parameters `{library: tstr, api: tstr, target_boundary_contract: cid}`; answer shape `BoundaryTagMemento`.
- `sort-classification`: per `(sort, language)`; parameters `{language: tstr}`; answer shape `SortMorphismMemento`.
- `effect-classification`: per `(effect category, language)`; parameters `{language: tstr}`; answer shape `EffectSignatureMemento`.
- `morphism`: per `(concept, language)`; parameters `{from_language: tstr}`; answer shape `MorphismMemento`.
- `composition`: reserved for v2.0; v1.1 emits no composition questions.

`boundary-realization` replaces the boundary-flavored subset of v1
cross-product `realization` questions. A v1.1 producer MUST only emit a
`boundary-realization` question when the target library is native to the target
language and the question cites the boundary contract by CID.

## Closing Citations

The manifest formalizes the audit lineage from
`docs/audits/2026-05-12-concept-library-completeness-probe.md` and
`docs/audits/2026-05-12-concept-library-completeness-probe-operation-layer.md`.
Paper 12 and Paper 13 provide the substrate grounding: ProofIR is the universal
boundary, and language signatures plus algorithm mementos make morphisms and
realizations the operational primitives that the exam asks about.
