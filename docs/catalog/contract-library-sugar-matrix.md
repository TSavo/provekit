# Contract-Library Sugar Matrix

**Status:** catalog matrix for issue #932. Partial deliverable: matrix document only.

**Scope:** known contract-library sugar cells and the current implementation state visible from this worktree. A row is a `(language, library_tag)` cell. Existing sugar JSON files under `menagerie/*-language-signature/specs/sugar/` are listed as shipped rows. Libraries named by #932 but not backed by a sugar JSON file are listed as in-progress or planned, with `sugar spec` left empty.

`menagerie/concept-shapes/cids.tsv` currently has no `kind = sugar` rows. The JSON sugar mementos carry `header.cid`, but this matrix leaves `sugar CID` as `not listed in cids.tsv` unless the registry source lists it.

Observation modes use the #880 vocabulary: `witness`, `monitor`, `emitter`, and `gate`. The local git evidence for the concept hub and mode-scoped Java sugars appears in #881 and #883.

## Matrix

| language | library tag | status | sugar spec | sugar CID | concepts covered | observation modes | emitted surface | loss dimensions | relift behavior | PR / issue refs |
|---|---|---|---|---|---|---|---|---|---|---|
| C | canonical | shipped | `menagerie/c-language-signature/specs/sugar/c-canonical.json` | not listed in cids.tsv | `concept:gt`, `concept:ge`, `concept:lt`, `concept:le`, `concept:eq`, `concept:ensures_gt`, `concept:ensures_eq` | `witness`, `monitor`, `emitter`, `gate` by mode-agnostic clause rendering | comment annotation before function | none declared | exact for listed predicate templates; refuse on unmatched formulas | sugar dict framework #740; current file |
| Java | bean-validation | shipped | `menagerie/java-language-signature/specs/sugar/java-bean-validation.json` | not listed in cids.tsv | `concept:ge`, `concept:gt`, `concept:le`, `concept:lt`, `concept:neq` | `gate` | annotation before parameter | none declared | exact for supported integer bounds and non-null; refuse on unsupported formulas | #891, #883 |
| Java | canonical | shipped | `menagerie/java-language-signature/specs/sugar/java-canonical.json` | not listed in cids.tsv | `concept:gt`, `concept:ge`, `concept:lt`, `concept:le`, `concept:eq`, `concept:ensures_gt`, `concept:ensures_eq` | `witness`, `monitor`, `emitter`, `gate` by mode-agnostic clause rendering | comment annotation before method | none declared | exact for listed predicate templates; refuse on unmatched formulas | #926, #923, sugar dict framework #740 |
| Java | function-comment | shipped | `menagerie/java-language-signature/specs/sugar/java-function-comment.json` | not listed in cids.tsv | any formula as `${any_formula}` | `witness`, `monitor`, `emitter`, `gate` by mode-agnostic fallback | comment above function | `structural_divergence: machine_uncheckable_prose` | bounded-lossy; exact only if replaced by liftable contract-comment payload | #934, #935 |
| Java | junit5 | shipped | `menagerie/java-language-signature/specs/sugar/java-junit5.json` | not listed in cids.tsv | `concept:neq` | `witness` | test harness assertion | `domain_narrowing: witness_requires_test_execution`; `structural_divergence: witness_skeleton_requires_concrete_values` | bounded-lossy until concrete example values execute and relift as witness evidence | #737 per #932 prompt; local git also shows #485, #531, #883 |
| Python | canonical | shipped | `menagerie/python-language-signature/specs/sugar/python-canonical.json` | not listed in cids.tsv | `concept:gt`, `concept:ge`, `concept:lt`, `concept:le`, `concept:eq`, `concept:ensures_gt`, `concept:ensures_eq` | `witness`, `monitor`, `emitter`, `gate` by mode-agnostic clause rendering | comment annotation before function | none declared | exact for listed predicate templates; refuse on unmatched formulas | #936 for Python relift path; current file |
| Java | spring | in-progress | none | none | `concept:neq`, `concept:ge`, `concept:le`, size-bound predicates, opaque pre/post evidence | primarily `gate`; `monitor` when wrapped by observation policy | annotation | TBD; expected structural divergence for framework binding and opaque expression cases | bounded-lossy until a sugar JSON declares exact cells; refuse when annotation semantics cannot be mapped | #735 per #932 prompt; local git shows Spring native surface #727 |
| Python | pydantic | in-progress | none | none | `concept:ge`, `concept:gt`, `concept:le`, `concept:lt`, length bounds, field-shape predicates | `gate`, with witnessable validation outcomes under policy | field metadata / runtime validator declaration | TBD; expected domain narrowing for runtime validator semantics and structural divergence for unsupported constraints | bounded-lossy for current relift paths; exact only after a minted sugar JSON and relift tests prove byte-identical recovery | #932, #936, #727 |
| Python | icontract | planned | none | none | pre/post/invariant predicates over decorators | `gate`, `monitor`, `witness` planned | decorator | TBD | refuse until sugar JSON and relift tests exist | #932 |
| Python | deal | planned | none | none | pre/post/raises/has predicates over decorators | `gate`, `monitor`, `witness` planned | decorator | TBD; opaque lambdas likely structural divergence until parsed | refuse until sugar JSON and relift tests exist | #932; native v0 recognition appears in #727 |
| Python | pytest | planned | none | none | assertion-derived witnesses and parameterized input examples | `witness` | test harness | TBD; expected domain narrowing over sampled inputs | refuse until sugar JSON and relift tests exist | #932 |
| TypeScript | zod | planned | none | none | schema predicates for numeric bounds, string bounds, object field shape, kind checks | `gate`, `monitor`, `witness` planned | runtime call / schema declaration | TBD; unsupported refinements likely structural divergence | refuse until sugar JSON and relift tests exist | #932; native v0 surface #727 |
| TypeScript | decorators | planned | none | none | decorator-carried validation predicates such as class-validator style field constraints | `gate`, `monitor` planned | decorator | TBD | refuse until sugar JSON and relift tests exist | #932 |
| TypeScript | jest-vitest | planned | none | none | assertion-derived witnesses and test cases | `witness` | test harness assertion | TBD; expected domain narrowing over sampled test inputs | refuse until sugar JSON and relift tests exist | #932 |
| multi-language | asserts | planned | none | none | host `assert` predicates and assertion macros | `witness`, `gate` planned | assertion / runtime call | TBD; expected build-mode and runtime-enable domain narrowing | refuse until sugar JSON and relift tests exist | #932 |
| multi-language | witness-runtime-helpers | planned | none | none | runtime helper calls that emit witnessed contract observations | `witness`, `monitor`, `emitter` planned | runtime call | TBD; expected observer-effect and sample-coverage dimensions | refuse until sugar JSON and relift tests exist | #932 |

## Required Cell Shape

Each cell in this catalog is admissible only when it can be described with the following fields:

1. `language`: the host language where the surface is authored or emitted.
2. `library_tag`: the stable library selector used by dispatch and policy.
3. `concepts`: the `concept:*` operations or predicate families the cell can carry.
4. `observation_modes`: the subset of `witness`, `monitor`, `emitter`, and `gate` admitted for the cell.
5. `emitted_surface`: the concrete host surface: annotation, decorator, assertion, runtime call, comment, docstring, or test harness.
6. `composition_point`: where the cell attaches: parameter, method, function, field, callsite, schema, test case, or helper call.
7. `supported_formula_fragment`: the formula fragment the cell matches exactly. Anything outside this fragment must carry named loss or refuse.
8. `named_loss_dims`: the loss dimensions declared by the sugar entry or by the planned cell.
9. `relift_behavior`: one of `exact`, `bounded-lossy`, or `refuse`.
10. `tests_prove_fail_closed`: tests must show unsupported formulas, malformed payloads, missing CIDs, and policy refusal do not silently emit trusted evidence.

The matrix is not allowed to use a blank loss record as optimism. Empty loss means the cell has an exact mapping for that row's named fragment. A broader library surface still needs additional rows or a refusal path.

## Policy Selection Note

Sugar cells are selected at realize time by policy, not by local defaults. Issue #889 names the sugar-selection-policy memento lane: it owns candidate scoring, loss admission, and whether a candidate may be emitted for a requested mode. PR #948 adds `PolicyProfileMemento`, which bundles witness-consensus, sugar-selection, and emission-gating decisions into one content-addressed profile CID.

The selection path is therefore:

1. load candidate sugar cells by `(language, library_tag)`;
2. filter by requested observation mode;
3. score declared loss under the sugar-selection policy;
4. apply the active policy profile's emission gate; and
5. emit only the selected surfaces, with the policy CID and loss facts recoverable from the audit trail.

## Non-goals

This catalog does not implement new sugar.

This catalog does not modify existing sugar JSON, CID registries, lifters, realizers, tests, or policy files.

This catalog does not claim that a source library is semantically correct. It records what ProvekIt can lift, emit, relift, or refuse for that library surface.

This catalog does not turn planned cells into shipped cells. A planned row becomes shipped only when a sugar JSON exists, its spec path resolves, loss dimensions are declared, and fail-closed tests link to it.

This catalog does not select a default policy. Selection belongs to the sugar-selection policy and the policy profile cited by the run.

## How To Add A New Contract Library

1. Mint a sugar JSON under `menagerie/<lang>-language-signature/specs/sugar/` with stable `target_language`, `sugar_name`, entries, mode filters, and surface locators.
2. Declare every loss dimension the cell can incur. If the mapping is exact only for a fragment, name the fragment and refuse the rest.
3. Write a relift path when the emitted surface is intended to round trip. The relift must prove exact recovery or emit bounded-lossy evidence with named loss.
4. Register the cell in this matrix with status, spec path, CID registry state, observation modes, emitted surface, loss dimensions, relift behavior, and PR or issue refs.
5. Link tests that prove fail-closed behavior for malformed payloads, unsupported formulas, missing CIDs, disabled runtime modes, and policy refusal.
