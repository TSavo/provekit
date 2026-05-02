# IR Compiler Protocol (`provekit-ir-compiler/2`)

**Status:** v1.4.0 normative draft
**Date:** 2026-05-02
**Catalog property:** Listed in the v1.4.0 catalog as `ir-compiler-protocol-v2`; CID is computed from this file's bytes per `2026-04-30-protocol-catalog-format.md` §2.1 (raw-byte BLAKE3-512).
**Owner:** verifier crate + every conformant IR compiler.
**Supersedes:** `2026-04-30-ir-compiler-protocol.md` (v1, listed in the v1.3.0 catalog at `ir-compiler-protocol`). The v1 spec remains valid for any v1.3.0-or-earlier verifier; v1.4.0 verifiers running in `coverage_required` consensus mode require v2 conformance.

## §0. The protocol is the bytes

A v2 compiler's wire output is a pair of byte strings: the compiled solver script, and the OpacityManifest. Both are part of the protocol. Two conformant compilers in two languages translating the same IR document with the same dialect produce byte-identical scripts AND byte-identical manifests. The English in this document describes those bytes; if the bytes disagree with the English, the bytes win and the English is updated.

## §1. Why v2 exists

The v1 spec (`2026-04-30-ir-compiler-protocol.md`) gave each compiler a hard binary choice when faced with an IR position it could not soundly translate: refuse the whole compile (`compile_error.unsupported_*`) or silently elide the position. The first cuts off composition; the second is unsound.

ProvekIt v1.4.0 introduces a multi-solver consensus mode that wants to compose partial competence: an SMT solver that handles arithmetic but not predicate quantification; a Coq solver that handles induction but is slow on bitvectors; a Lean solver that handles dependent types but not strings. Each compiler is the authority on what its theory can soundly handle. The verifier's job is to compose them.

The v2 contract closes the gap with a third option: **a compiler emits a tractable script for everything it can soundly translate, marks every untranslatable position with a theory-equivalent of "trust me," and records each marked position in an OpacityManifest.** The verifier reads every solver's manifest; consensus succeeds when, for every opaque position any solver reported, some *other* solver in the pool both handled that position AND returned `Discharged`.

The IR is unchanged. **The IR knows nothing about Coq, opacity, or solver capabilities.** The IR knows about `Lambda`, `Atomic`, `Forall`, kit predicates. Each compiler decides what its theory can soundly translate. Opacity is a compiler-side concept; the IR's only role is to be the universal language whose subterms get content-addressed by `positionCid`.

This spec defines:

1. The new `OpacityManifest` emission requirement (§3).
2. The compiler opacity-emission rule: emit a tractable placeholder for each opaque position, AND record it in the manifest (§4).
3. Position content-addressing: `positionCid` is the BLAKE3-512 of the JCS-canonical bytes of the opaque IR subterm (§5).
4. The closed-enum reason codes shipped in v1.4.0 plus the extension procedure (§6).
5. The updated JSON-RPC method shapes that carry the manifest (§7).
6. Backward-compatibility positioning relative to v1 (§8).

The manifest's grammar, byte-for-byte canonicalization rules, and worked example live in the standalone spec `2026-05-02-opacity-manifest-grammar.md`. This spec defines the *requirement* to emit one; that spec defines the manifest's *shape*.

## §2. Inheritance from v1

Everything in `2026-04-30-ir-compiler-protocol.md` carries forward unchanged unless this spec says otherwise:

- Plugin discovery via `~/.config/provekit/ir-compilers/<name>/manifest.toml`.
- Manifest schema (`name`, `version`, `protocol_version`, `binary`, `dialects`).
- JSON-RPC over stdio framing.
- The `provekit.ir.handshake`, `provekit.ir.compile`, `provekit.ir.shutdown` methods (with the v2 amendments below).
- The dialect registry: `smt-lib-v2.6`, `smt-lib-v2.6-bv`, `smt-lib-v2.6-delta`, `tptp-fof`, `tptp-thf`, `lean-tactic-mode`, `gallina`, `isabelle-hol`. New dialects continue to be added by amending the relevant spec.
- The error model (`-32700`, `-32600`, …, `2000`–`2004`).
- Capability negotiation at handshake time.
- The bootstrapping shape: in-process trait + standalone subprocess binary, sharing one emit implementation.

The single substantive change v2 introduces is the OpacityManifest emission requirement. The single contract change is that `provekit.ir.compile` now returns the manifest as a sibling field of `preamble` and `body`.

## §3. The emission requirement

**INVARIANT (v2 emission):** A v2-conformant compiler MUST, on every successful `provekit.ir.compile` call, emit an `OpacityManifest` per `2026-05-02-opacity-manifest-grammar.md`. The manifest's `protocolVersion` field MUST be the literal `"ir-compiler-protocol/2"`. The manifest's `compiler` field MUST equal the dialect identifier this compiler serves. Empty `opacities: []` is the byte-shape for "this compiler translated every position soundly."

**INVARIANT (manifest is non-optional):** A compiler that returns a successful `compile` response without an `opacityManifest` field is non-conformant with v2. A v1.4.0 verifier consuming such a response MUST NOT treat it as v2 input; the verifier MUST either (a) treat the compiler as v1-only (excluded from `coverage_required` consensus) or (b) reject the response.

**INVARIANT (sound emission required for opacity):** A compiler that marks a position opaque MUST emit a syntactically valid theory-side placeholder for that position. The placeholder MUST be the theory-equivalent of "trust me" (vacuous, tautological, or admitted) such that the resulting solver script remains syntactically valid AND tractable for the solver. The opaque parts are MARKED, not omitted.

The required placeholders by dialect:

| Dialect | Placeholder for an opaque position |
|---|---|
| `smt-lib-v2.6`, `smt-lib-v2.6-bv`, `smt-lib-v2.6-delta` | `(assert true)` substituted at the syntactic position of the opaque subterm. If the position is sub-formula-level (inside a Boolean structure), the entire smallest-enclosing-assertion is replaced with `(assert true)` — the compiler MUST NOT splice `true` into a partial expression. |
| `gallina` | `Admitted.` for the lemma corresponding to the opaque position. |
| `lean-tactic-mode` | `sorry` at the tactic position of the opaque position. |
| `isabelle-hol` | `oops` (or `sorry`, per Isabelle's conventions) at the proof position of the opaque position. |
| `tptp-fof`, `tptp-thf` | The tautological axiom `fof(opaque_<positionCid>, axiom, $true).` (or its `thf` analog), substituted at the position. |

For dialects not enumerated above, the compiler MUST document its placeholder convention in the dialect's spec; the convention MUST satisfy the soundness rule: the resulting script remains syntactically valid for the solver, and the solver's verdict on the script is sound *modulo* the positions marked opaque.

**INVARIANT (placeholder + manifest atomicity):** Every position the compiler emitted a placeholder for MUST appear as an `Opacity` entry in the manifest. Conversely, every `Opacity` entry in the manifest MUST correspond to a placeholder in the emitted script. The placeholder-only-no-manifest case is unsound; the manifest-only-no-placeholder case is malformed.

## §4. Position content-addressing

`Opacity.positionCid` is defined and normatively specified in `2026-05-02-opacity-manifest-grammar.md` §3. Restated for this spec:

```
positionCid = "blake3-512:" || hex(BLAKE3-512(JCS(opaque_subterm)))
```

The "opaque subterm" is the IR-JSON node the compiler chose to mark opaque. The compiler's choice of granularity is the compiler's authority; the verifier compares positions by `positionCid` only.

**INVARIANT (positional content-addressing):** Two compilers that mark the same syntactic IR subterm opaque produce the same `positionCid`. The verifier matches opacities across solvers exclusively by `positionCid` equality.

## §5. Reason codes (closed enum + extension procedure)

The v1.4.0 reason codes:

| Reason code | Meaning |
|---|---|
| `kit_predicate_no_semantics` | The opaque subterm is an `Atomic` predicate application whose name is registered as a kit predicate, but the compiler has no theory semantics for it. |
| `nested_lambda` | The opaque subterm is a `Lambda` whose body contains another `Lambda`. The compiler's lambda fragment is first-order; nested-lambda bodies are out of range. |
| `predicate_quantification` | The opaque subterm is a `Lambda` whose `paramSort` is `Bool` or a function sort. The compiler can quantify over individual values but not over predicates. |
| `dependent_type` | The opaque subterm references a value-dependent type. The compiler's type system is non-dependent. |
| `other:<freeform>` | Anything else. The freeform suffix documents the reason for human auditors and participates in byte-level manifest equality. |

**Closed-enum justification:** the verifier's coverage rule needs to know "this opacity reason is the same class of opacity another solver claimed to handle." A closed enum gives stable comparison. `other:<freeform>` is the open-bucket extension for novel opacity classes; v1.4.0 ships with the four codes above. Future minor versions of `ir-compiler-protocol/2` MAY add codes additively. The full extension procedure and its forward-compatibility guarantee is in `2026-05-02-opacity-manifest-grammar.md` §4.

**INVARIANT (closed-enum stability):** The four base reason codes are stable across the lifetime of `ir-compiler-protocol/2`. Conformant compilers MUST emit one of the base codes whenever their reason matches one of the four meanings, rather than falling through to `other:`.

## §6. Updated JSON-RPC method shapes

### §6.1 `provekit.ir.handshake`

The handshake response gains one optional field:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "name": "smt-lib-reference",
    "version": "0.2.0",
    "protocol_version": "ir-compiler-protocol/2",
    "dialects": ["smt-lib-v2.6"],
    "supported_sorts": ["Int", "Bool", "Real", "String"],
    "supported_predicates": [ "=", "<", "<=", ">", ">=", "and", "or", "not", "implies", "forall", "exists" ],
    "opacity_reason_codes": [
      "kit_predicate_no_semantics",
      "nested_lambda",
      "predicate_quantification",
      "dependent_type"
    ]
  }
}
```

`protocol_version` MUST be `"ir-compiler-protocol/2"`. `opacity_reason_codes` SHOULD enumerate the closed-enum codes the compiler will actually emit; verifiers use this for diagnostic display only. Compilers MAY also emit `other:<...>` codes at runtime even if `opacity_reason_codes` doesn't list them.

A verifier that performs handshake MUST reject any compiler whose `protocol_version` is not in `{ "ir-compiler-protocol/1", "ir-compiler-protocol/2" }`. A compiler claiming v1 is excluded from `coverage_required` consensus per `2026-05-02-multi-solver-protocol-v2.md`.

### §6.2 `provekit.ir.compile`

The response gains a required `opacityManifest` field:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "preamble": "(set-logic ALL)\n(declare-const x Int)\n",
    "body": "(assert (not (> x 0)))\n(check-sat)\n",
    "free_vars": [ { "name": "x", "sort": "Int" } ],
    "opacityManifest": {
      "compiler": "smt-lib-v2.6",
      "compilerVersion": "0.2.0",
      "opacities": [],
      "protocolVersion": "ir-compiler-protocol/2"
    }
  }
}
```

**INVARIANT:** Every successful `compile` response from a v2 compiler carries `opacityManifest` as a sibling of `preamble`, `body`, and `free_vars`. The manifest's bytes are the JCS-canonical form per `2026-05-02-opacity-manifest-grammar.md` §6.

When `opacities` is non-empty, the `body` MUST contain the placeholder substitutions specified in §3. The verifier MAY parse the script to confirm the placeholders are present; it is not required to. The trust anchor is the catalog-signed compiler implementation.

### §6.3 `provekit.ir.shutdown`

Unchanged from v1.

## §7. Capability negotiation, error model, plugin discovery

Unchanged from v1 except:

- The `protocol_version` value `"ir-compiler-protocol/2"` is now also valid (in addition to `"ir-compiler-protocol/1"`).
- The `2001 compile_error.unsupported_sort` and `2002 compile_error.unsupported_predicate` codes are still valid escape hatches when a compiler decides a position is so badly malformed it cannot even mark it opaque (e.g., an unrecognized sort referenced from a free variable). v2 compilers SHOULD prefer marking the position opaque over erroring out, so that the consensus mechanism has a chance to compose around it; erroring out remains valid for genuinely malformed input.
- A new error code `2005 compile_error.opacity_emission_failed` is added for the rare case where a compiler successfully decides a position is opaque but cannot emit a syntactically valid placeholder for it. This is a compiler bug; the verifier reports it and stops.

## §8. Backward-compatibility

**v1 compilers participate in v1 modes.** A v1 compiler (returning `protocol_version: "ir-compiler-protocol/1"`, no `opacityManifest`) continues to work in any v1 multi-solver mode (`Single`, `Chain`, `Portfolio { first-wins }`, `Portfolio { consensus, coverage_required: false }`). The verifier passes its compiled output to the named solver as before; no manifest is consulted, no coverage rule applies.

**v1 compilers do NOT participate in `coverage_required` consensus.** The v2 multi-solver spec (`2026-05-02-multi-solver-protocol-v2.md`) requires every solver in a `coverage_required = true` portfolio to emit a v2 manifest. A pool that includes any v1 compiler is rejected at consensus time with `compile_error.opacity_manifest_missing`-equivalent telemetry. This is **scorched earth, no back-compat**, by design: composing partial competence requires every compiler in the pool to declare what it cannot soundly handle.

**v2 compilers participate in v1 modes.** A v2 compiler is forward-compatible. When invoked in a v1 multi-solver mode, the verifier MAY ignore the `opacityManifest` field. The placeholders in `body` may surface as vacuous discharges; the operator who configured a v1-only solver pool with v2 compilers has accepted this.

**Catalog migration.** The v1.4.0 catalog cut (separate spec work) decides whether `ir-compiler-protocol-v2` enters the catalog as a sibling of `ir-compiler-protocol` (this spec's draft assumption) or replaces it. Either way, the v1 spec's CID remains valid in any older catalog that referenced it.

The v1 spec CID, for cross-reference: `blake3-512:71fc7ac22997938629d835f87e4e8a322026d77c1e1f834c9fbe0f79cca4e903792c628e96d3004c88d29706f4d87bc042ff837fef571c0cb3012495a03003d3` (per the v1.3.0 catalog).

## §9. Acceptance

This spec is satisfied by:

- The reference Rust implementation at `implementations/rust/provekit-ir-compiler-smt-lib/` updated to emit OpacityManifests on every `compile` call.
- The byte-fixture suite at `tests/opacity-manifest-fixtures/` (specified in `2026-05-02-opacity-manifest-grammar.md` §10).
- A v2 conformance test asserting that the worked example in `2026-05-02-opacity-manifest-grammar.md` §8 round-trips byte-identically through the SMT-LIB v2.6 compiler.
- A v1 backward-compatibility test asserting that an existing v1 multi-solver demo continues to work when its compiler is upgraded to v2 (the manifest is emitted but ignored).
- The `provekit ir-compiler list` command surfacing each plugin's `protocol_version` so operators can audit pool composition before enabling `coverage_required` consensus.

## §10. Related specs

- `2026-05-02-opacity-manifest-grammar.md` — the manifest's byte-level shape, canonicalization, position content-addressing, worked example. This spec REQUIRES the manifest; that spec DEFINES it.
- `2026-05-02-multi-solver-protocol-v2.md` — the verifier's consumption rule that uses manifests to compose consensus verdicts.
- `2026-04-30-ir-compiler-protocol.md` — the v1 spec this v2 supersedes. CID `blake3-512:71fc7ac22997938629d835f87e4e8a322026d77c1e1f834c9fbe0f79cca4e903792c628e96d3004c88d29706f4d87bc042ff837fef571c0cb3012495a03003d3` per the v1.3.0 catalog.
- `2026-04-30-ir-formal-grammar.md` — the IR-JSON grammar whose subterms are content-addressed by `positionCid`. Unchanged in v1.4.0; the IR is opacity-agnostic.
- `2026-04-30-protocol-catalog-format.md` — the rule by which this spec's CID is computed.
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative for the manifest bytes.
