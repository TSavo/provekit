# Body Template Memento (`pep/1.7.0`, kind = `"body-template"`)

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- `2026-05-12-plugin-protocol.md` (the protocol this spec consumes)
- `2026-05-12-sugar-dict-memento.md` (sibling: contract-clause sugar; this spec is method-body sugar)
- `2026-05-12-loss-function-memento.md` (loss function consulted at selection time)
- `implementations/rust/provekit-cli/src/cmd_transport.rs` (the consumer; closes `bind-stub-body-emitted` for templated concepts)

## §1. Purpose

`provekit bind --rewrite=canonical --target-language=<L>` emits a function in language `L` for every binding. When no real lifted term graph is available, today's substrate falls through to a language-idiomatic stub (`throw new UnsupportedOperationException(...)` for Java, `raise NotImplementedError(...)` for Python, etc.). The stub is honest under Supra omnia rectum but is the dominant entry in the trinity round-trip's loss-record set (`bind-stub-body-emitted`).

A `body-template` plugin is a content-addressed dictionary mapping `(target_language, concept_name)` to a function-body template renderable from the function's signature alone. Multiple body-template plugins MAY load simultaneously; per-binding selection is loss-minimizing across loaded body-templates.

### §1.1 What a body-template is NOT

- Not a desugaring rule (those go core→source one clause at a time, per `2026-05-11-desugaring-and-the-core-compression.md`).
- Not a `sugar` plugin (those render contract CLAUSES per `2026-05-12-sugar-dict-memento.md` §1.1: "Not a code generator"). This spec IS a code generator for method bodies, the inverse complement.
- Not a discharge backend.
- Not a contract lifter; templates render bodies, not contracts.

### §1.2 Trichotomy mapping

| Outcome | Condition for a single binding |
|---|---|
| `exact` | Selected entry's `loss_record_contribution` is empty (no dimension has a non-`false` formula). |
| `loudly-bounded-lossy` | Selected entry has a non-empty `loss_record_contribution`; loss-record is carried forward. |
| `refuse` | Under `--strict-body-template`, no entry in any LOADED body-template plugin matched. Emit refuse-memento, NOT stub fallback. |

## §2. The `content` payload

```cddl
; Imports:
;   loss-record   ; from 2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.3
;   cid           ; "blake3-512:" tstr

; Locked JCS key order: entries, target_language, template_name
body-template-content = {
  entries:         [+ body-template-entry],
  target_language: tstr,
  template_name:   tstr
}

; Locked JCS key order: composition_point, concept_name, emission_template,
; loss_record_contribution, mode, signature_guard
body-template-entry = {
  ? composition_point:       composition-point,
  concept_name:              tstr,
  emission_template:         body-emission-template,
  loss_record_contribution:  loss-record-contribution,
  ? mode:                    observation-mode,
  ? signature_guard:         signature-guard
}

composition-point = "before" / "after-return" / "after-throw" / "around"
observation-mode  = "witness" / "monitor" / "emitter" / "gate"

; Locked JCS key order: kind, template
body-emission-template = {
  kind:      "verbatim",
  template:  tstr
}

; Locked JCS key order: form, value
loss-record-contribution = {
  form:  "literal",
  value: any
}

; Locked JCS key order: max_params, min_params, requires_param_types, requires_return_type
signature-guard = {
  ? max_params:            uint,
  ? min_params:            uint,
  ? requires_param_types:  [+ tstr],
  ? requires_return_type:  tstr
}
```

### §2.1 Field semantics

| Field | Required | Meaning |
|---|---|---|
| `entries` | yes | One or more entries. MUST be sorted ascending by the JCS-canonical bytes of `concept_name` at JCS time. |
| `target_language` | yes | Target language identifier (`"java"`, `"python"`, `"rust"`, `"csharp"`, etc.). |
| `template_name` | yes | Free-form label (e.g., `"java-canonical-bodies"`). Part of the plugin CID. |
| `concept_name` | yes | The canonical concept name this entry covers (e.g., `"concept:identity"`, `"concept:bool-cell"`). Exact-string match against the binding's resolved concept name; no pattern matching. |
| `mode` | no | Runtime observation mode this entry covers when `concept_name == "concept:contract-observation"`. Absent means the entry is mode-agnostic and remains v1.0.0-compatible. |
| `composition_point` | no | Wrapper insertion point for observation-body entries. Values are `before`, `after-return`, `after-throw`, or `around`. Absent means the entry is an ordinary function body template. |
| `emission_template.template` | yes | Surface-syntax template. `${param0}`, `${param1}`, ... bind to parameter names in order. `${return_type}` binds to the target-language return type after `map_source_type` resolution. Unbound placeholders MUST cause the entry to refuse-match (treated as non-applicable). |
| `loss_record_contribution` | yes | Loss-record incurred when selected. v1.0.0 form MUST be `"literal"`. |
| `signature_guard` | no | If present, an entry MAY refuse-match when the binding's signature violates the guard (e.g., a 2-arg entry MUST NOT match a 1-arg binding). Used to bound entry applicability beyond bare concept-name match. |

### §2.1.1 Observation entries

`concept:contract-observation(callsite_cid, contract_cid, mode)` is the
runtime-observation hub concept. Body-template entries for this concept SHOULD
set both `mode` and `composition_point`. The mode is part of applicability, not
metadata; a `mode = "witness"` entry MUST NOT match a `gate` request unless a
separate entry declares that mode.

Observer effects belong to the emitted wrapper and any
`ObservationWrapperMemento`, not to the wrapped object FCM. A body-template cell
MAY record runtime-dependency or surface-enforcement loss when the target
language/library cannot express the selected observation mode exactly.

### §2.2 Selection

For each binding with resolved `concept_name = C`:

1. Look up entries where `entry.concept_name == C` across all LOADED body-template plugins.
2. Drop candidates whose `mode` is present unless the binding carries the same observation mode.
3. For each candidate, check `signature_guard` (if present) against the binding's signature; drop on mismatch.
4. Of the remaining candidates, select the one whose `loss_record_contribution` minimizes against the loaded loss function (`2026-05-12-loss-function-memento.md`).
5. Render `emission_template.template` with the binding's parameter and type bindings.
6. If no candidate remains, fall through to the language stub (`cmd_transport.rs::stub_body_for`) under default mode, or refuse under `--strict-body-template`.

### §2.3 Template substitution

The renderer substitutes:
- `${param0}`, `${param1}`, ... `${paramN-1}` → parameter names in declaration order.
- `${param_count}` → number of parameters as a decimal string.
- `${return_type}` → mapped target-language return type.
- `${param_type_0}`, ... `${param_type_N-1}` → mapped target-language parameter types.

Any other `${...}` placeholder is unbound and causes refuse-match per §2.1.

## §3. CLI surface

Per `2026-05-12-plugin-protocol.md` §3 and §7:

```
--plugin body-template:<source>      # canonical
--body-template <source>             # per-kind alias
```

Repeated loads compose. Order is preserved into the registry's `load_order` for tie-breaking.

Body-template-specific flags:

| Flag | Effect |
|---|---|
| `--strict-body-template` | When no entry matches a binding, refuse instead of falling through to the language stub. |
| `--no-default-body-templates` | Suppress built-in body-template plugin registration (§7 of plugin-protocol). |

## §4. Default loading

The substrate registers ONE default body-template per target language at `cmd_transport` startup, loaded from:

```
menagerie/<lang>-language-signature/specs/body-templates/<lang>-canonical-bodies.json
```

These defaults MUST be content-addressed (a `cid` in the `header` block) and signed per the plugin-protocol authentication rules. The set may be suppressed with `--no-default-body-templates`.

## §5. Compatibility

A binding for which no body-template matches falls through to the existing `stub_body_for` emission (Supra omnia rectum: the substrate must still produce a compilable artifact). The fall-through emits a `bind-stub-body-emitted` gap entry naming the affected concept(s); when ALL concepts in a bind run have matching body-templates, the gap entry is omitted entirely.

## §6. Open follow-ups

- v1.1.0: `kind: "computed"` emission templates that evaluate an `ir-formula` to a string.
- v1.1.0: cross-language template inheritance (`extends: "<other-plugin-cid>"`).
- v1.1.0: multi-cell wrapper composition where one operation body composes with several observation-mode body-template cells at explicit before/after-return/after-throw/around points.
