# Trinity body-template completeness audit (post-floor)

Date: 2026-05-18
Source: TSavo/sugar @ ffbea3064 (post Trinity floor-completion landings #1150, #1151, #1152)
Worktree: `/Users/tsavo/sugar-worktrees/audit-2026-05-18-body-templates`
Branch: `kit/audit-body-templates`

## 0. Scope and method

The Trinity floor-completion work today made the classifier report `absent=0` for Java, Rust, and Python. The classifier confirms 16 boundary realizations for Java, 16 for Python, and 7 for Rust (see `docs/audits/2026-05-17-realization-tag-classification.md`, section 3).

This audit answers a downstream question: for each declared boundary realization in those three languages, does the kit's lower path actually emit an executable body, or is the boundary declared but not wired to emission code?

Inputs read:

- `deleted concept-shapes catalog/realizations/*.json`: declarative realization records
- `menagerie/<lang>-language-signature/specs/body-templates/*.json`: emission templates
- `implementations/python/sugar-realize-python-core/src/sugar_realize_python_core/realizer.py`
- `implementations/rust/sugar-realize-rust-core/src/lib.rs`
- `implementations/java/sugar-realize-java-core/src/main/java/com/sugar/realize/*.java`
- `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs`
- `implementations/rust/sugar-cli/tests/verb_composition.rs`
- `implementations/rust/sugar-cli/tests/library_tag_dispatch_test.rs`

The audit writes no substrate. Only this document is produced.

## 1. Summary

| language | declared boundary realizations | wired body templates | wired programmatic | placeholder (no wiring) | dispatch confidence |
| --- | --- | --- | --- | --- | --- |
| java | 16 | 9 | 0 | 7 | high |
| rust | 7 | 7 | 0 | 0 | high |
| python | 16 | 9 | 0 | 7 | high |

Headline finding: Java and Python both have an identical 7-concept gap between their declared boundary realizations and their body-template emission code. Rust is fully wired across its (smaller) 7-concept boundary set.

The 7-concept symmetric gap:

- `concept:closure`
- `concept:double-dispatch`
- `concept:dynamic-dispatch`
- `concept:exception`
- `concept:generic-instantiation`
- `concept:iterator`
- `concept:reference`

These seven concepts have realization records in the concept-shapes catalog (so the floor-completion classifier reports them) but no matching `concept_name` entry in any `<lang>-canonical-bodies*.json` body-template file, and no programmatic emission branch in the kit's `realizer.py` or `lib.rs`.

## 2. Body-template inventory (factual)

### 2.1 Python body-template files

Path: `menagerie/python-language-signature/specs/body-templates/`

| file | template_name | entries |
| --- | --- | --- |
| `python-canonical-bodies.json` | python-canonical-bodies | 27 |
| `python-canonical-bodies-aiosqlite.json` | python-canonical-bodies-aiosqlite | 2 |
| `python-canonical-bodies-blake3.json` | python-canonical-bodies-blake3 | 5 |
| `python-canonical-bodies-libsugar.json` | python-canonical-bodies-libsugar | 4 |
| `python-canonical-bodies-requests.json` | python-canonical-bodies-requests | 4 |
| `python-canonical-bodies-rust-runtime.json` | python-canonical-bodies-rust-runtime | 31 |
| `python-canonical-bodies-sqlite3.json` | python-canonical-bodies-sqlite3 | 2 |

Each entry has the shape `{concept_name, emission_template:{kind, template}, loss_record_contribution, signature_guard}`.

### 2.2 Rust body-template files

Path: `menagerie/rust-language-signature/specs/body-templates/`

| file | template_name | entries |
| --- | --- | --- |
| `rust-canonical-bodies.json` | rust-canonical-bodies | 15 |

### 2.3 Java body-template files

Path: `menagerie/java-language-signature/specs/body-templates/`

| file | template_name | entries |
| --- | --- | --- |
| `java-canonical-bodies.json` | java-canonical-bodies | 22 |

## 3. Dispatch logic (how the lower path picks a template)

### 3.1 Python core realizer

File: `implementations/python/sugar-realize-python-core/src/sugar_realize_python_core/realizer.py`

The realizer module loads body-template files via three `lru_cache`-backed loaders:

- `entries()` loads the union of `python-canonical-bodies.json`, `python-canonical-bodies-rust-runtime.json`, `python-canonical-bodies-blake3.json`.
- `libsugar_entries()` loads `python-canonical-bodies-libsugar.json`.
- Library-tagged kits (`sugar-realize-python-requests`, `sugar-realize-python-aiosqlite`, `sugar-realize-python-sqlite3`) load their own bodies file.

Dispatch flow (per `term_body_for_term_shape`, `_lower_term_expression`, `_body_template_expression_for_candidates`):

1. Concept name is normalized to two candidate keys: `concept:foo` and the bare `foo` suffix.
2. `_body_template_for_entries(entries(), candidates, ...)` walks the loaded entries, returns the first whose `concept_name` matches a candidate AND whose `signature_guard` (min/max params, optional `requires_param_types`, `requires_return_type`) accepts the call site.
3. If no entry matches, the realizer either falls back to programmatic shape lowering (`_lower_shape_body` for `concept:seq`, `concept:conditional`, etc.) or returns `None` and the caller raises `MissingTemplateError`.

There is no fallback `raise NotImplementedError("trinity lower")` emission in this file. That string is only present in the test-installed stub (see section 6).

### 3.2 Rust core realizer

File: `implementations/rust/sugar-realize-rust-core/src/lib.rs`

`emit_stub_with_mode` calls `operator_body_template_for` first, then `body_template_for`, then `emit_sugar_carrier`, and finally falls back to `stub_body_for(concept_name)` which emits `panic!("sugar-bind canonical: <concept_name>")` (line 828-831). The stub is the explicit unwired path.

Body-template lookup loads `rust-canonical-bodies.json` only; there is no language-tag dimension on the Rust side (no library-tagged bodies file).

### 3.3 Java core realizer

Package: `implementations/java/sugar-realize-java-core/src/main/java/com/sugar/realize/`

Key files: `SugarRealizer.java` (template-binding), `RealizerPlan.java`, `JavaNullBoundaryRealizer.java`, `RpcServer.java`.

Java loads `java-canonical-bodies.json` via the SugarRealizer path. Template binding is keyed on `concept_name`; mismatches fall through to a sugar-carrier comment or RealizerPlan default (the Java side does not panic, it emits a comment-only stub).

### 3.4 Library-tag dispatch (kit selection upstream of body emission)

File: `implementations/rust/sugar-cli/src/kit_dispatch.rs`

`dispatch_realize(target_lang, library_tag, request)` resolves a `kind = "realize"` plugin via convention. For Python the workspace can register multiple kits (default `sugar-realize-python-core`, plus `python-requests`, `python-aiosqlite`, `python-sqlite3`), each binding its own body-template file. Test in `library_tag_dispatch_test.rs` shows the routing works for `concept:sql-query` across `python-sqlite3` vs `python-aiosqlite`.

This means the body-template question is two-dimensional:

- Boundary concept `concept:foo` may be wired in `python-canonical-bodies.json` (kit `python-core`).
- The same concept may also need a wiring in `python-canonical-bodies-requests.json` if it appears at HTTP boundaries.

For the seven gap concepts identified below, none of them appear in any of the seven Python body-template files, so the gap is unambiguous.

## 4. Per-language detail

### 4.1 Java (16 declared boundary realizations)

Source: classifier output `docs/audits/2026-05-17-realization-tag-classification.md`, java section, filtered to `tag-kind=boundary`.

| concept | realization_target (declared) | body template (in java-canonical-bodies.json) | status |
| --- | --- | --- | --- |
| concept:assert | assert-statement | yes (verbatim, 148 chars) | WIRED |
| concept:bool-cell | atomic-boolean | yes (verbatim, 54 chars) | WIRED |
| concept:closure | lambda-invokedynamic | no entry | PLACEHOLDER |
| concept:double-dispatch | visitor-itab-pair | no entry | PLACEHOLDER |
| concept:dynamic-dispatch | virtual-method | no entry | PLACEHOLDER |
| concept:exception | try-catch | no entry | PLACEHOLDER |
| concept:generic-instantiation | type-erasure | no entry | PLACEHOLDER |
| concept:identity | function-identity | yes (verbatim, 53 chars) | WIRED |
| concept:iterator | iterable-iterator | no entry | PLACEHOLDER |
| concept:list | array-backed-list | yes (verbatim, 99 chars) | WIRED |
| concept:option-bind | optional-flat-map | yes (verbatim, 130 chars) | WIRED |
| concept:reference | object-reference | no entry | PLACEHOLDER |
| concept:result | sealed-interface | yes (verbatim, 89 chars) | WIRED |
| concept:result-bind | result-bind-switch | yes (verbatim, 132 chars) | WIRED |
| concept:tagged-union | sealed-interface | yes (verbatim, 121 chars) | WIRED |
| concept:unit | void-return | yes (verbatim, 43 chars) | WIRED |

WIRED: 9. PLACEHOLDER: 7.

Note: `java-canonical-bodies.json` contains 22 total entries; the 13 not listed above are non-Trinity-boundary concepts (`concept:contract-observation`, `concept:log-emit`, `hello-world`, `recursive-factorial`, `arithmetic-add`, `control-flow-if`, `http-request`, `http-response`, `pair`, `retry-loop`).

Realization file paths for the 7 PLACEHOLDER cases (declared but unwired):

- `concept:closure->java:lambda-invokedynamic.blake3-512:...json`
- `concept:double-dispatch->java:visitor-itab-pair.blake3-512:...json`
- `concept:dynamic-dispatch->java:virtual-method.blake3-512:...json`
- `concept:exception->java:try-catch.blake3-512:...json`
- `concept:generic-instantiation->java:type-erasure.blake3-512:...json`
- `concept:iterator->java:iterable-iterator.blake3-512:...json`
- `concept:reference->java:object-reference.blake3-512:...json`

### 4.2 Rust (7 declared boundary realizations)

Realization filenames use the inverted convention `rust:<target>->concept:<name>.blake3-512:...json`.

| concept | realization_target | body template (in rust-canonical-bodies.json) | status |
| --- | --- | --- | --- |
| concept:identity | identity | yes (verbatim, 45 chars) | WIRED |
| concept:list | Vec | yes (verbatim, 101 chars) | WIRED |
| concept:option | Option | yes (verbatim, 100 chars) | WIRED |
| concept:option-bind | Option::and_then | yes (verbatim, 174 chars) | WIRED |
| concept:pair | tuple | yes (verbatim, 58 chars) | WIRED |
| concept:result | Result | yes (verbatim, 103 chars) | WIRED |
| concept:unit | () | yes (verbatim, 38 chars) | WIRED |

WIRED: 7. PLACEHOLDER: 0.

Rust is fully wired for its declared boundary set. Note however that Rust's boundary set is narrower than Python's or Java's; the floor work for Rust did NOT declare `concept:exception`, `concept:closure`, `concept:iterator`, etc. as boundaries. That asymmetry is upstream of this audit.

### 4.3 Python (16 declared boundary realizations)

| concept | realization_target | body template (any python-canonical-bodies*.json) | status |
| --- | --- | --- | --- |
| concept:assert | assert-statement | yes (`assert` in `python-canonical-bodies.json`, 141 chars) | WIRED |
| concept:bool-cell | mutable-list-cell | yes (`bool-cell`, 56 chars) | WIRED |
| concept:closure | native-closure | no entry in any file | PLACEHOLDER |
| concept:double-dispatch | match-type-pair | no entry | PLACEHOLDER |
| concept:dynamic-dispatch | mro-dict-lookup | no entry | PLACEHOLDER |
| concept:exception | try-except | no entry | PLACEHOLDER |
| concept:generic-instantiation | duck-typing | no entry | PLACEHOLDER |
| concept:identity | lambda | yes (`identity`, 52 chars) | WIRED |
| concept:iterator | iter-next-protocol | no entry | PLACEHOLDER |
| concept:list | list | yes (`list`, 90 chars) | WIRED |
| concept:option-bind | optional-and-then | yes (`option-bind`, 124 chars) | WIRED |
| concept:reference | name-binding | no entry | PLACEHOLDER |
| concept:result | dataclass-tagged-union | yes (`result`, 95 chars) | WIRED |
| concept:result-bind | result-bind-if-ok | yes (`result-bind`, 132 chars) | WIRED |
| concept:tagged-union | dataclass-discriminated-union | yes (`tagged-union`, 111 chars) | WIRED |
| concept:unit | none-singleton | yes (`unit`, 47 chars) | WIRED |

WIRED: 9. PLACEHOLDER: 7. Identical gap set to Java.

Programmatic fallback check: greps of `realizer.py` for each of the seven placeholder concept names (`closure`, `exception`, `iterator`, `reference`, `dynamic-dispatch`, `double-dispatch`, `generic-instantiation`) return zero hits. There is no hand-coded emission branch for any of these in the Python core realizer.

## 5. Note on `concept:http-request` (not in scope, but worth disambiguating)

The task brief cites the Python lower emitting `raise NotImplementedError("trinity lower")` for `concept:http-request`. `concept:http-request` is NOT in the Trinity boundary realization set for any of the three languages. It is an API-tier concept handled separately:

- Python: wired in `python-canonical-bodies.json` as `http-request` (urllib path, 138 chars) AND in `python-canonical-bodies-requests.json` as `concept:http-request` (requests-library path, two arity variants).
- Rust: wired in `rust-canonical-bodies.json` as `http-request` (reqwest path, 125 chars).
- Java: wired in `java-canonical-bodies.json` as `http-request` (java.net.http.HttpClient path, 601 chars).

`concept:http-request` is fully wired across the Trinity for both library-default and library-specific kits. The `NotImplementedError` text observed in the trinity_roundtrip test scenario originates elsewhere (section 6).

## 6. Why the trinity_roundtrip test still fails (separate question from the audit)

File: `implementations/rust/sugar-cli/tests/trinity_roundtrip_test.rs`, lines 95-110.

The test does NOT invoke `sugar-realize-python-core`. It writes a stub Python script `trinity-lower-python.py` directly into the test temp directory:

```
source = f"# concept: {params.get('concept_name', '')}\ndef {params.get('function', 'f')}({', '.join(params.get('params', []))}):\n    raise NotImplementedError(\"trinity lower\")\n"
```

That stub is registered as the `realize/python` plugin via `.sugar/realize/python/manifest.toml` and emits the placeholder for every concept request. The assertion at line 196 (`assert!(py.contains("# concept: concept:http-request"), "{py}")`) only verifies the comment annotation survived the round-trip, NOT that an executable body was emitted.

The `verb_composition.rs` test follows the same pattern (line 103, line 240).

Operational implication: any failures the user observes in `trinity_roundtrip` or `verb_composition` tests are NOT caused by missing body templates in the real kit. They are caused by the test scaffold installing a placeholder lower by design. Wiring the seven gap concepts in `python-canonical-bodies.json` will not change those test outcomes.

This is a separate diagnostic. The audit's body-template completeness finding stands independently.

## 7. Identified gaps

Concrete (kit, concept, library) triples where wiring is incomplete:

| kit | concept | declared realization target | gap kind |
| --- | --- | --- | --- |
| sugar-realize-java-core | concept:closure | java:lambda-invokedynamic | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:double-dispatch | java:visitor-itab-pair | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:dynamic-dispatch | java:virtual-method | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:exception | java:try-catch | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:generic-instantiation | java:type-erasure | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:iterator | java:iterable-iterator | no template entry, no programmatic branch |
| sugar-realize-java-core | concept:reference | java:object-reference | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:closure | python:native-closure | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:double-dispatch | python:match-type-pair | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:dynamic-dispatch | python:mro-dict-lookup | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:exception | python:try-except | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:generic-instantiation | python:duck-typing | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:iterator | python:iter-next-protocol | no template entry, no programmatic branch |
| sugar-realize-python-core | concept:reference | python:name-binding | no template entry, no programmatic branch |

Total: 14 triples (7 Java, 7 Python). Rust: zero gap triples.

## 8. Recommended next sub-issues under #978/#1068

The gap is symmetric across Java and Python, with parallel declared realization targets. Recommended dispatch order:

### Phase B1 (parallel-dispatchable, mechanical-leaning)

Each of the seven concepts gets one issue per affected kit (14 issues total). Each issue:

- Adds one entry to the kit's canonical-bodies JSON keyed on the concept name.
- Body template encodes the declared realization target as a verbatim emission (or `seq` shape if the realization is multi-statement).
- Tests: positive emission, signature-guard rejection, byte-identical re-emission across runs.

Concrete proposed issues (suggest dispatch order matches Trinity-floor work's natural pairing, Java then Python):

1. `feat(java-kit): wire concept:closure body template (java:lambda-invokedynamic)`
2. `feat(java-kit): wire concept:exception body template (java:try-catch)`
3. `feat(java-kit): wire concept:iterator body template (java:iterable-iterator)`
4. `feat(java-kit): wire concept:reference body template (java:object-reference)`
5. `feat(java-kit): wire concept:dynamic-dispatch body template (java:virtual-method)`
6. `feat(java-kit): wire concept:double-dispatch body template (java:visitor-itab-pair)`
7. `feat(java-kit): wire concept:generic-instantiation body template (java:type-erasure)`
8-14. Same seven for `python-kit`, parallel-dispatchable once Java seven are done (or fully parallel if dispatcher can hold 14 isolated worktrees).

Discrimination-tests-per-variant rule from `feedback_discrimination_tests_per_variant.md`: each issue ships at least three tests per emission (positive, structural, discrimination), not a single positive case.

### Phase B2 (architect-call, not mechanical)

Before merging Phase B1, decide:

- Whether Rust's narrower boundary set is correct or whether Rust ALSO needs `concept:exception`, `concept:closure`, `concept:iterator`, `concept:reference`, etc. declared as boundaries. The classifier currently marks these `absent` or `sugar-carrier` for Rust, but Rust definitely has closures, iterators, and references at substrate level. If declared, Rust gains 4+ more boundary realizations and 4+ more body templates.
- Whether to upgrade the stub lower in `trinity_roundtrip_test.rs` and `verb_composition.rs` to invoke the real `sugar-realize-python-core` kit, so those tests actually exercise the body-template path. Currently they validate only the lift-bind-lower wire shape, not body emission.

Both items are Sir's call, not dispatcher-mechanical.

### Phase B3 (library-tagged kits)

For each of the seven Python placeholder concepts, decide whether the concept needs a wired body in any library-tagged kit (e.g. `sugar-realize-python-requests`). `concept:exception` and `concept:iterator` plausibly do; `concept:closure` and `concept:reference` plausibly do not. This decision should be made per concept after Phase B1 lands, not blocked on it.

## 9. Confidence and replayability

Classification CID for the upstream tag-kind audit: `blake3-512:c97c3d8287516ec326a5ae374185cf0a4df83b04b97be7f4c5a71cfa2ccd759c0d2081d29f2569fb7123fd109ce0d622858614182bd373825d063dc9eb4daf44` (from `2026-05-17-realization-tag-classification.md`).

This audit was produced by reading the named source files directly. The 9-wired / 7-placeholder split for Java and Python is empirically verifiable by:

```
python3 -c "import json; [print(e['concept_name']) for e in json.load(open('menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json'))['header']['content']['entries']]"
```

cross-referenced against the boundary concept list in section 3 of `docs/audits/2026-05-17-realization-tag-classification.md`.

Dispatch-confidence rating: high for all three. The dispatch logic is straightforward keyed lookup against `concept_name` with deterministic loaders (`@lru_cache(maxsize=1)`); there is no hidden multi-stage resolution that could route a missing entry to a programmatic emitter unobserved.
