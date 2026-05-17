# ExamManifestMemento Normative Spec

**Status:** v1.0.0 normative draft. Catalog entry to be appended in follow-up CI mint.
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

For v1.0.0 the manifest carries only the question set and each question's
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
    schemaVersion: "provekit-exam-manifest/v1"
  }
}

exam-manifest-content = {
  concept_hub_version: tstr,
  question_kinds:      [+ tstr],
  questions:           [+ exam-question]
}

exam-question = {
  concept:               tstr,
  expected_answer_shape: tstr,
  kind:                  tstr,
  parameters:            { * tstr => any }
}
```

Objects use alphabetical JCS key order per
`2026-04-30-canonicalization-grammar.md`. Producers MUST emit canonical bytes;
consumers MUST canonicalize before hashing or signature verification.

## §2 The Question-Kind Enum

The v1.0.0 canonical labels are:

- `morphism`
- `realization`
- `sort`
- `effect`
- `boundary-tag`
- `composition`

The enum is open per PEP 1.7.0 §2.1. Shape-level validators accept unknown
kinds. The consumer that receives an unknown kind decides whether to refuse it,
and records that refusal at the consumer boundary rather than failing the
manifest shape.

## §3 Per-Question Parameters Schema Per Kind

| Kind | Parameters | Question |
| --- | --- | --- |
| `morphism` | `{from_language: tstr}` | How does `from_language` express the concept? |
| `realization` | `{target_language: tstr, target_library: tstr}` | How does `target_language` render the concept via `target_library`? |
| `sort` | `{language: tstr, language_type: tstr}` | How does the language's `language_type` map to the concept's sort? |
| `effect` | `{language: tstr, effect_signature: tstr}` | How does the language realize the concept's effect signature? |
| `boundary-tag` | `{library: tstr, api: tstr, target_concept: tstr}` | Does the library's `api` bind to `target_concept`? |
| `composition` | `{language: tstr, composition_name: tstr}` | How does the language realize the composition pattern? |

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
  "metadata": {"schemaVersion": "provekit-exam-manifest/v1"}
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
exam_manifest_schema_version = "provekit-exam-manifest/v1"
```

A runtime MAY compile in a default built-in `exam-manifest` plugin per PEP
1.7.0 §0.1 "a built-in default is still a memento at a fixed declared CID."
Substrate v1.0.0 ships a built-in that loads manifest JSON from a local file
path or from a catalog CID lookup; third-party plugins MAY emit manifests via
JSON-RPC.

Plugin RPC: invoke method is `provekit.plugin.invoke` per PEP 1.7.0 §4.
Request payload is `{path: tstr}` or `{cid: tstr}` (one or the other; both is
a refusal). Response payload is the canonical ExamManifestMemento JSON per §1
of this spec.

## §8 Versioning

`schemaVersion` is `provekit-exam-manifest/v1`. Future versions such as `/v2`
are new schemas with their own CIDs. Backward compatibility commitments follow
`2026-04-30-protocol-versioning.md`.

## §9 Trichotomy At The Question Boundary

| Outcome | Condition |
| --- | --- |
| `exact` (answered) | A morphism, realization, sort, effect, boundary-tag, or composition memento exists in the catalog with byte-identical signature to the question's `parameters` and `expected_answer_shape`. The catalog index or audit tool decides this. |
| `loudly-bounded-lossy` (answered with characterized divergence) | The answer memento exists but carries a non-empty `loss_record_contribution` per `2026-05-14-transport-gap-and-partial-morphism-protocol.md`. |
| `refuse` | The answer memento does not exist, and a `TransportGapMemento` exists in `concept-shapes/gaps/` citing this question in future issue #1106, or no gap record exists yet. For v1.0.0, open is the default; cited refusal is the loud refusal. |

The manifest itself does not decide which state applies. It only gives every
consumer the same question identity to cite.

## §10 Out Of Scope For v1.0.0

- `ExamCoverageMemento` and per-question `answered_at` or `refused_at` tracking. This is decoupled per architect ruling; the manifest carries only the question set without coverage state.
- Cross-manifest translation mementos. Federation handshake refuses on CID mismatch; explicit translation is future work.
- Auto-generation of manifest entries from concept hub shape specs. The generator tool is issue #1105; this spec describes only the shape the generator must produce.
- Per-question discharge tracking. Each answer's refinement obligation discharge is tracked via existing `catalog/receipts/`; the manifest only references receipt CIDs once answers are minted, via the future `ExamCoverageMemento` family.

## Closing Citations

The manifest formalizes the audit lineage from
`docs/audits/2026-05-12-concept-library-completeness-probe.md` and
`docs/audits/2026-05-12-concept-library-completeness-probe-operation-layer.md`.
Paper 12 and Paper 13 provide the substrate grounding: ProofIR is the universal
boundary, and language signatures plus algorithm mementos make morphisms and
realizations the operational primitives that the exam asks about.
