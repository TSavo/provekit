# Sugar Dict Memento (`pep/1.7.0`, kind = `"sugar"`)

**Status:** v1.0.0 normative draft. First consumer of the universal plugin protocol.
**Date:** 2026-05-12
**Author:** T Savo
**Related:**
- `2026-05-12-plugin-protocol.md` (the protocol this spec consumes; NORMATIVE)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization)
- `2026-04-30-ir-formal-grammar.md` (`IrFormula` shape used in `predicate_pattern`)
- `2026-05-11-desugaring-and-the-core-compression.md` (the inverse direction; this spec is "re-sugaring")
- `2026-05-12-loss-function-memento.md` (the loss function consulted at selection time, §4.4)
- `2026-05-13-compound-contract-memento.md` (the compound this spec renders from)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 (`loss-record` shape returned by `loss_record_contribution`)
- `implementations/rust/provekit-cli/src/cmd_transport.rs` (the deferred-list note "cosmetic re-sugaring after the core-form realizer" that this spec closes)

## §1. Purpose

Canonical contract clauses produced by the core-form realizer are abstract `IrFormula` trees. To be readable by a target-language tool (a Spring app, a JML-annotated Java method, a JUnit 5 test, or a comment-as-documentation line), they MUST be rendered into surface syntax. The rendering is NOT a single fixed function: it is plural by construction. Different surfaces compete for the same canonical clause under best-only policy; under inclusive policy, multiple selected surfaces intentionally compose. Every selected surface carries its own loss record, scored against the loaded loss function (`2026-05-12-loss-function-memento.md`).

A `sugar` plugin is a content-addressed dictionary of rendering entries. Multiple sugar plugins MAY be loaded simultaneously; per-clause selection is loss-minimizing or inclusive depending on the active emission policy (§4). The mechanism replaces the "cosmetic re-sugaring after the core-form realizer" item parked in `implementations/rust/provekit-cli/src/cmd_transport.rs` line 296 (`deferred` field of `TransportReport`).

### §1.1 What a sugar dict is NOT

- Not a desugaring rule (those are oriented left-to-right surface-to-core, per `2026-05-11-desugaring-and-the-core-compression.md`). A sugar dict is the INVERSE: oriented core-to-surface.
- Not a code generator. A sugar dict emits ONE clause at a time given ONE canonical clause; it does not compose against a program structure.
- Not a discharge backend. Sugar dicts produce surface SYNTAX, not discharge witnesses.

### §1.2 Trichotomy mapping

| Outcome                 | Condition for a single canonical clause                                                              |
|-------------------------|------------------------------------------------------------------------------------------------------|
| `exact`                 | The selected entry's `loss_record_contribution` is empty (no dimension has a non-`false` formula).   |
| `loudly-bounded-lossy`  | The selected entry has a non-empty `loss_record_contribution`; the loss-record is carried forward.   |
| `refuse`                | Under `--strict-sugar`, no entry in any LOADED sugar dict matched. The clause is emitted as a refuse-memento, NOT as a comment fallback. |

## §2. The `content` payload

The `content` payload of a sugar plugin memento (`kind = "sugar"`, per `2026-05-12-plugin-protocol.md` §1) is the following CDDL shape:

```cddl
; Imports:
;   ir-formula        ; from 2026-04-30-ir-formal-grammar.md
;   loss-record       ; from 2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.3
;   cid               ; "blake3-512:" tstr

; Locked JCS key order: entries, sugar_name, target_language
sugar-content = {
  entries:         [+ sugar-entry],
  sugar_name:      tstr,                       ; e.g. "spring", "jml", "junit5", "comment", "pydantic"
  target_language: tstr                        ; e.g. "java", "python", "typescript", "comment"
}

; Locked JCS key order: applicability_guard, emission_template,
; loss_record_contribution, mode, predicate_pattern
sugar-entry = {
  ? applicability_guard:     ir-formula,       ; OPTIONAL; if present, MUST evaluate to true for the entry to apply
  emission_template:         emission-template,
  loss_record_contribution:  loss-record-contribution,
  ? mode:                    tstr,             ; OPTIONAL; when present, entry applies only when request modes include it
  predicate_pattern:         ir-formula        ; with named holes (free variables) the matcher binds
}

; The emission template. The `template` string MAY contain `${name}`
; placeholders bound by the matcher's hole substitution; placeholders that
; are not bound at emission time MUST cause the entry to refuse-match
; (the entry is treated as non-applicable for this clause).
;
; Locked JCS key order: kind, surface_locator, template
emission-template = {
  kind:            "verbatim" / "computed",   ; v1.0.0: ONLY "verbatim" is wired
  surface_locator: tstr,                       ; e.g. "annotation:before-method", "comment:above", "import:top-of-file"
  template:        tstr                        ; the surface-language template string
}

; The loss-record contribution can be either a literal loss-record
; (constant per entry) OR a formula over the bound holes that evaluates to
; a loss-record at emission time. v1.0.0 wires ONLY the literal form;
; "formula" is spec'd but rejected with a refuse at load time.
;
; Locked JCS key order: form, value
loss-record-contribution = {
  form:  "literal" / "formula",
  value: any                                   ; for "literal": a loss-record; for "formula": an ir-formula
}
```

### §2.1 Field semantics

| Field                                    | Required | Meaning |
|------------------------------------------|----------|---------|
| `sugar-content.entries`                  | yes      | One or more rendering entries. MUST be sorted by the JCS-canonical bytes of `predicate_pattern` ascending at JCS time. |
| `sugar-content.sugar_name`               | yes      | A free-form label for the sugar dict (e.g., `"spring"`, `"comment"`). Part of the plugin CID. |
| `sugar-content.target_language`          | yes      | Target language identifier (e.g., `"java"`, `"python"`, `"comment"`). The sentinel value `"comment"` denotes a language-agnostic comment sugar that emits documentation strings; consumer code MAY apply it to any target language by attaching the emission to a per-language comment surface. |
| `sugar-entry.predicate_pattern`          | yes      | An `IrFormula` with named holes (free variables). At selection time, the matcher attempts to unify this against the canonical clause. If unification succeeds, the entry's holes are bound to subterms. |
| `sugar-entry.applicability_guard`        | no       | OPTIONAL `IrFormula` over the bound holes. If present, MUST evaluate to `true` for the entry to apply (e.g., a Spring `@Min` entry MIGHT guard on `is_integer_literal(${k})` to refuse non-literal lower bounds). OMITTED when absent. |
| `sugar-entry.emission_template`          | yes      | The surface-syntax template, plus a `surface_locator` placing it relative to the host code element. |
| `sugar-entry.loss_record_contribution`   | yes      | The loss-record this entry incurs when selected. Under v1.0.0 the `form` MUST be `"literal"`; the `value` is a `loss-record` literal as defined in `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3. |
| `sugar-entry.mode`                       | no       | OPTIONAL runtime observation mode applicability filter. When present, the entry applies only if the current realization request's mode vector includes the same mode string (for example `"witness"` for JUnit witness harness sugar, or `"gate"` for Bean Validation gate sugar). When absent, the entry is mode-agnostic and remains compatible with non-observation clauses. |

### §2.2 Pattern matching

The matcher unifies `predicate_pattern` against the canonical clause's `IrFormula` per `2026-04-30-ir-formal-grammar.md` first-order syntactic unification, treating any free variable as a hole. Bound holes flow into `applicability_guard` (if present) and into `emission_template.template` via `${name}` substitution. Unification failure means the entry does NOT match.

### §2.3 Observation-mode applicability

`concept:contract-observation(callsite_cid, contract_cid, mode)` routes
Witness, Monitor, Emitter, and Gate through the same concept hub. Sugar entries
that are meaningful for only one of those modes MUST declare `mode`. For
example, a JUnit witness harness entry declares `"mode": "witness"` so it cannot
render for a request whose mode vector omits `witness`; a Bean Validation entry
declares `"mode": "gate"` so it renders only when gate sugar is requested.
Comment-floor entries and other universal fallbacks omit `mode`.

## §3. CLI surface

Per `2026-05-12-plugin-protocol.md` §3 and §7:

```
--plugin sugar:<source>        # canonical
--sugar <source>               # per-kind alias
```

Repeated loads compose (§4). The order is preserved into the registry's `load_order` and is consulted for tie-breaking (§4.4).

Additional sugar-specific flags:

| Flag                          | Effect                                                                                             |
|-------------------------------|----------------------------------------------------------------------------------------------------|
| `--strict-sugar`              | When no entry in any LOADED sugar dict matches a clause, refuse instead of falling through. §5.   |
| `--allow-comment-fallback`    | Permits the special `comment` sugar to act as a final fallback even when other sugars matched. §5.|

## §4. Selection and emission algorithm

For each canonical contract clause C produced by the realizer:

The emission policy described in this section is codified as a content-addressed artifact in `2026-05-18-sugar-selection-policy-memento.md`.

### §4.0 Emission policy

A sugar consumer MUST apply one of these policies for a clause:

| Policy | Meaning |
|--------|---------|
| `best-only` | Candidate surfaces compete. The consumer emits exactly one selected candidate: the lowest-loss candidate after §4.2 scoring and §4.4 tie-break. |
| `inclusive` | Candidate surfaces compose. The consumer emits every applicable candidate that survives §4.1, ordered by §4.2 scoring and §4.4 tie-break. No candidate is discarded merely because another applicable candidate has lower loss. |

`best-only` is the default for ordinary single-surface contract rendering. `inclusive` is REQUIRED when a realization request carries a `concept:contract-observation` mode vector and the selected sugars cover different requested modes. A kit MUST NOT drop a `witness` sugar because a `gate` sugar has lower loss, or vice versa; those are additive observations over the same contract. Mode-agnostic entries (for example comment-floor sugar) also participate under `inclusive` and MUST carry their declared loss record if emitted.

### §4.1 Step 1: enumerate matching entries

For each sugar plugin S in the registry's `load_order` (CLI flag order plus built-ins at end), for each entry E in `S.content.entries`:

1. Attempt unification of `E.predicate_pattern` against C. If unification fails, skip E.
2. If `E.mode` is present and the current realization request mode vector does not include it, skip E.
3. If `E.applicability_guard` is present, evaluate it under the bound holes. If it evaluates to anything other than `true`, skip E.
4. Otherwise, record `(S, E, bindings)` as a candidate.

The matcher MUST be deterministic: two runs with identical inputs MUST produce the same candidate list in the same order.

### §4.2 Step 2: score each candidate

For each candidate `(S, E, bindings)`, the candidate's loss-record is `E.loss_record_contribution.value` (literal form; v1.0.0). Hand the loss-record to the LOADED loss function (`2026-05-12-loss-function-memento.md`); the loss function returns a score.

### §4.3 Step 3: rank and select

Sort candidates by score ascending (lower = better).

Under `best-only`, the TOP candidate (lowest score after tie-break) wins and all other candidates are rejected for this clause.

Under `inclusive`, every candidate remains selected. Ranking is still normative because it defines deterministic emission order and audit order, but it does not discard lower-ranked candidates.

### §4.4 Step 4: tie-break

If two or more candidates tie, break ties as follows:

1. Prefer the candidate whose sugar dict appears LATER in `load_order` (later flags win; matches the §7 rule of the protocol spec for "user-loaded plugins beat built-ins of the same kind").
2. If still tied, prefer the candidate whose entry index within its sugar dict is LOWER (entries earlier in `entries` win).
3. If still tied (impossible by §2.1 sort INVARIANT, but recorded for completeness), refuse with `reason_kind = "sugar-tiebreak-ambiguity"` under `best-only`; under `inclusive`, preserve deterministic registry order and include both candidates.

### §4.5 Step 5: emit

Render each selected candidate's `emission_template` with the bound holes substituted. Attach each selected candidate's `loss_record_contribution` to the emission's audit trail (carried forward in any compound that aggregates the emission, per `2026-05-13-compound-contract-memento.md` §6.2).

For example, when a `witness,gate` Java realization has matching JUnit witness sugar and Bean Validation gate sugar for the same canonical non-null clause, an inclusive consumer MUST emit both. Those surfaces are not competing alternatives in inclusive mode; they are two observation wrappers with separate loss records.

## §5. Strict mode

Under `--strict-sugar`, if Step 1 (§4.1) produces ZERO candidates for a clause, the runtime MUST emit a refuse-memento for that clause rather than falling through to a comment or to a stringified IR dump. The refuse is a `PluginLoadFailureMemento` (per `2026-05-12-plugin-protocol.md` §8) with `plugin_kind = "sugar"` and `reason_kind = "no-matching-sugar-entry"`.

Without `--strict-sugar`, the runtime MAY fall through to a built-in `comment` sugar (if loaded) that emits the IR formula as a free-form comment with `loss_record_contribution = { "structural_divergence": <machine_uncheckable_prose> }` (the IR carries the truth; the comment is human-readable but solver-opaque). `--allow-comment-fallback` permits this fallback even when other sugars matched but were all refused by strict checks.

## §6. Worked example

Two sugar dicts loaded: a Spring annotation sugar (matches `requires(gt(x, 0))` style clauses on integer formals and emits `@Min(1)`) and a comment sugar (matches anything and emits a free-form comment). The default loss function (`2026-05-12-loss-function-memento.md` §6) is in effect.

### §6.1 Sugar dict 1: Spring (`spring.json`)

The complete `content` payload, as it would appear inside a plugin-memento JSON file's `header.content` field. The bytes below are JCS-canonical (alphabetical key order):

```json
{
  "entries": [
    {
      "emission_template": {
        "kind": "verbatim",
        "surface_locator": "annotation:before-parameter",
        "template": "@Min(${k_plus_one})"
      },
      "loss_record_contribution": {
        "form": "literal",
        "value": {}
      },
      "predicate_pattern": {
        "args": [
          { "args": [], "head": "var", "name": "x" },
          { "args": [], "head": "const", "value": "${k}" }
        ],
        "head": "gt"
      }
    }
  ],
  "sugar_name": "spring",
  "target_language": "java"
}
```

The single entry's `predicate_pattern` matches a `gt(x, k)` shape with two holes (`x` and `k`). The `emission_template` references a derived hole `${k_plus_one}` which is the result of evaluating `k + 1` over the bound `${k}` (the matcher's term-derivation rules per `2026-04-30-ir-formal-grammar.md` §4 cover the arithmetic). The `loss_record_contribution.value` is the empty loss-record `{}`, meaning the rendering is EXACT (no dimension is divergent; `@Min(N)` precisely encodes `gt(x, N-1)` in Spring's bean-validation semantics).

### §6.2 Sugar dict 2: comment (`comment.json`)

```json
{
  "entries": [
    {
      "emission_template": {
        "kind": "verbatim",
        "surface_locator": "comment:above",
        "template": "// requires: ${formula_pretty_print}"
      },
      "loss_record_contribution": {
        "form": "literal",
        "value": {
          "structural_divergence": {
            "args": [],
            "head": "machine_uncheckable_prose"
          }
        }
      },
      "predicate_pattern": {
        "args": [],
        "head": "${any_formula}"
      }
    }
  ],
  "sugar_name": "comment",
  "target_language": "comment"
}
```

The single entry's `predicate_pattern` has a head-hole `${any_formula}` that matches ANY `IrFormula` (the matcher treats a hole at the head position as universally applicable; this is the `comment` sugar's defining shape). The `loss_record_contribution.value` records a non-empty `structural_divergence` dimension with the formula `machine_uncheckable_prose`, declaring that comments are not solver-checkable.

### §6.3 Selection on the canonical clause `requires(gt(x, 0))`

Step 1 (§4.1):
- Spring's entry unifies: `x -> x`, `k -> 0`, `k_plus_one -> 1`. Candidate recorded.
- Comment's entry unifies: `any_formula -> requires(gt(x, 0))`. Candidate recorded.

Step 2 (§4.2):
- Spring's loss-record is `{}`. Under the default lexicographic-preorder loss function (`2026-05-12-loss-function-memento.md` §6), the empty record sorts FIRST (best).
- Comment's loss-record is `{ "structural_divergence": machine_uncheckable_prose }`. Under the same loss function this sorts later (worse).

Step 3 (§4.3): Spring wins.

Step 4 (§4.4): no tie; Step 4 is skipped.

Step 5 (§4.5): Emit `@Min(1)` at surface locator `annotation:before-parameter`. Attach Spring's empty loss-record to the emission's audit trail; the compound that aggregates this emission inherits an empty loss in this dimension.

### §6.4 The same selection under `--strict-sugar` with ONLY the comment sugar loaded

If the user loads ONLY the comment sugar (`--plugin sugar:./comment.json`) and passes `--strict-sugar` while the input is a clause Spring would have handled, the comment sugar matches (head-hole) and emits its comment with the documented loss-record. Strict mode permits this: a match exists. Strict mode refuses only when ZERO entries match.

If `--allow-comment-fallback` is NOT set but Spring is loaded and matches with a refuse on guard (hypothetical future entry with an `applicability_guard` that fails), the comment sugar does NOT pick up the slack. Strict mode is silent-fallback-prevention, not last-resort-permission.

## §7. Plugin memento header (worked example, full bytes)

For Sugar dict 1 (Spring) above, the full plugin memento `header` (per `2026-05-12-plugin-protocol.md` §1) is:

```json
{
  "content": {
    "entries": [
      {
        "emission_template": {
          "kind": "verbatim",
          "surface_locator": "annotation:before-parameter",
          "template": "@Min(${k_plus_one})"
        },
        "loss_record_contribution": {
          "form": "literal",
          "value": {}
        },
        "predicate_pattern": {
          "args": [
            { "args": [], "head": "var", "name": "x" },
            { "args": [], "head": "const", "value": "${k}" }
          ],
          "head": "gt"
        }
      }
    ],
    "sugar_name": "spring",
    "target_language": "java"
  },
  "critical": false,
  "kind": "sugar",
  "protocol_versions": ["pep/1.7.0"],
  "provenance_cid": "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "schemaVersion": "1",
  "version": "1.0.0"
}
```

The `provenance_cid` shown above is the sentinel all-zeros CID per `2026-05-13-compound-contract-memento.md` §4.4; in a real deployment it MUST resolve to a `ProvenanceMemento` recording the build chain of this sugar dict.

The CID of this header is `"blake3-512:" ++ hex(BLAKE3-512(JCS(<header bytes with cid elided>)))` per `2026-05-12-plugin-protocol.md` §6.1. The bytes above are intentionally complete: a reader running JCS + BLAKE3-512 over them MUST produce a determinate CID; this spec does not assert that CID's hex digits because they are derivable mechanically and asserting them invites copy errors. The byte-exact CID-pinning test lives in a follow-up implementation crate alongside the JCS encoder (matching the precedent in `2026-05-13-compound-contract-memento.md` §3.2).

## §8. Cross-references

- The `predicate_pattern` field's `IrFormula` grammar is normative per `2026-04-30-ir-formal-grammar.md`.
- The `loss_record_contribution.value` field's `loss-record` shape is normative per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3.
- The plugin memento envelope, CID rules, load procedure, and registry semantics are NORMATIVE per `2026-05-12-plugin-protocol.md`.
- The loss-function plugin consulted in §4.2 is the LOADED loss function per `2026-05-12-loss-function-memento.md`; the default loss function is defined in §6 of that spec.
- The inverse direction (surface-to-core desugaring) is `2026-05-11-desugaring-and-the-core-compression.md`; a sugar dict and a desugaring set MAY share a name but are different mementos with different CIDs (different `kind` values: `sugar` vs `equation` with `role: "desugaring"`).
- This spec closes the "cosmetic re-sugaring after the core-form realizer" item parked in `implementations/rust/provekit-cli/src/cmd_transport.rs` line 296.

## §9. Out of scope for v1.0.0

- The `"computed"` emission-template kind (only `"verbatim"` is wired).
- The `"formula"` loss-record-contribution form (only `"literal"` is wired).
- Implementation in any runtime. This spec is the WIRE shape and the SELECTION algorithm; the implementation lands in a follow-up PR.
- A built-in sugar dict library (per-language sugars for Spring, JML, JUnit, pydantic, Zod, OpenAPI live in a follow-up; this spec defines the SHAPE they conform to, not the catalog).
- Automatic surface-locator inference (the `surface_locator` is declared per entry; tools that need to attach an emission to a specific source location use the per-locator semantics each surface-locator value documents in its OWN follow-up spec).
