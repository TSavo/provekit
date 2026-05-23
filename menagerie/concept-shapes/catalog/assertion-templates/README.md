# Assertion Templates

Catalog family minted by PR-5 of the predicate-emitter plugin substrate (issue
[#1401](https://github.com/TSavo/provekit/issues/1401)).

Each entry maps a ProvekIt predicate concept (with its argument shape) to a
target-language native-test-framework assertion. This first batch covers
JUnit5; subsequent PRs (PR-6 python pytest, PR-7 rust core, PR-8 ts vitest)
will follow the same shape.

## Naming

```
<concept-name>->junit5:<assertion>.<cid>.json
```

For example:
- `concept:eq->junit5:assertEquals.<cid>.json`
- `concept:lt->junit5:assertTrue.<cid>.json`

## Schema

Defined in `menagerie/concept-shapes/specs/assertion-template_shape.spec.json`.
The memento envelope follows the existing realization convention (memento + cid
+ signature). The memento payload carries `kind: "assertion-template"` with
these fields:

- `predicate_concept`: references an existing concept hub (must be minted
  under `catalog/abstractions/` or `catalog/algorithms/`).
- `target`: `{framework, assertion}`, e.g. `{junit5, assertNotNull}`.
- `formals` + `formal_sorts`: ordered parameter names and their sorts.
- `emit_template`: native source string with `{formal}` placeholders.
- `harvest_pattern`: primary native source pattern to recognise.
- `harvest_pattern_alt`: additional surface patterns the harvester matches
  to the same predicate.
- `loss_record`: structural divergence between the JUnit form and the
  concept (per first principle: exact, loudly-bounded-lossy, or refuse).
- `discharge_receipt`: `deferred:pending-pk-1401` until the per-language
  predicate-emitter plugin lands and runs the discharge.

## Bidirectional by design

The same table serves both directions of the harvester-emitter cycle:

- **HARVESTER direction**: a JUnit assertion in test source matches
  `harvest_pattern` (or any of `harvest_pattern_alt`), and the harvester
  identifies the predicate concept the assertion discharges.
- **EMITTER direction**: a predicate concept from the catalog looks up its
  template, substitutes formal args into `emit_template`, and emits the JUnit
  assertion call in the target source.

Because some comparison ops (`lt`/`gt`/`le`/`ge`) all map to the same JUnit
assertion name (`assertTrue`), the harvester must disambiguate on the inline
operator carried inside `harvest_pattern`. The patterns encode the operator
literally for exactly this reason.

## Consumers

- **PR-6**: python pytest predicate-emitter plugin (reads these to identify
  pytest-equivalent assertions for cross-language traversal).
- **PR-7**: rust core predicate-emitter plugin (reads these for the
  rust-junit cycle).
- **PR-8**: ts vitest predicate-emitter plugin.

## This batch (PR-5)

Seven JUnit5 assertion templates were minted, one for each predicate concept
whose hub already exists in the catalog:

| Predicate concept       | JUnit5 assertion    | Template                       |
| ----------------------- | ------------------- | ------------------------------ |
| `concept:option-is-some` | `assertNotNull`    | `assertNotNull({x})`           |
| `concept:eq`            | `assertEquals`      | `assertEquals({a}, {b})`       |
| `concept:ne`            | `assertNotEquals`   | `assertNotEquals({a}, {b})`    |
| `concept:lt`            | `assertTrue`        | `assertTrue({a} < {b})`        |
| `concept:gt`            | `assertTrue`        | `assertTrue({a} > {b})`        |
| `concept:le`            | `assertTrue`        | `assertTrue({a} <= {b})`       |
| `concept:ge`            | `assertTrue`        | `assertTrue({a} >= {b})`       |

### Skipped (follow-up PRs)

The following rows from the original PR-5 brief were skipped because the
predicate concept hub does not yet exist; they will be minted once the hub
lands:

- `concept:option-is-none` (no hub yet)
- `concept:list-empty`, `concept:list-nonempty` (no hubs yet)
- `concept:throws` / `concept:fallible-err` (no hub yet; `concept:throw`
  exists but models the throw statement, not the predicate that a body
  throws, so the harvester would misidentify any source-level `throw` as
  an assertion call)
- `concept:bool-true`, `concept:bool-false` (no hubs yet)

## Reproduce

```sh
python3 menagerie/concept-shapes/scripts/mint_junit5_assertion_templates.py
```

The script is idempotent: a second run produces byte-identical files and
appends nothing new to `cids.tsv`.

T Savo
