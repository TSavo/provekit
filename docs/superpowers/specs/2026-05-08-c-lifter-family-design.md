# C Lifter Family Design

## Summary

Build a family of C contract lifters over a shared C lifting core. The Linux kernel is the first serious corpus and composition target, but it is not a single semantic lifter and it is not a `--dialect=kernel` mode.

The reusable part is `libprovekit-c-lift-core`: parsing, source loci, facts, diagnostics, opacity reporting, IR helpers, and merge-friendly output. Each contract language used by C projects is a separate lifter with its own manifest, capabilities, tests, and semantic vocabulary.

The core exposes parser-backend provenance. The temporary regex backend remains the conservative default for existing lifters, while an opt-in libclang AST backend can populate the same fact model when a caller supplies enough compiler context. Missing or unavailable AST context is an extraction visibility issue, not a semantic refusal.

## Goals

- Provide a reusable C lifting substrate for ordinary C projects and Linux-kernel-scale code.
- Model kernel contract expression as a composition of separate C lifters.
- Keep each semantic surface independently testable and independently discoverable.
- Emit deterministic declarations, call edges, diagnostics, opacity reports, and refusals.
- Preserve a load-bearing distinction between opacity and refusal.
- Start with small fixtures, not a whole-kernel checkout.

## Non-Goals

- Do not build a monolithic Linux kernel lifter.
- Do not add `--dialect=kernel` as the semantic boundary.
- Do not require a full kernel build, `compile_commands.json`, or BTF/DWARF integration in the first slice.
- Do not use angr as a source lifter. angr is a later binary witness backend over contracts emitted by these lifters.
- Do not silently infer kernel semantics from weak cues. Unsupported recognized cases must become refusals; extraction visibility limits must become opacity entries.

## Architecture

The architecture has one shared C lift core and many standalone C-family lifters.

```text
libprovekit-c-lift-core
  source input
  source loci
  parser/fact model
  function, parameter, call, macro, attribute, and comment facts
  diagnostics
  opacity report helpers
  refusal helpers
  IR/call-edge emission helpers

standalone lifters
  provekit-lift-c-sparse
  provekit-lift-c-kernel-doc
  provekit-lift-c-lockdep
  provekit-lift-c-rcu
  provekit-lift-c-errno
  provekit-lift-c-kunit
  provekit-lift-c-assertions
```

Each lifter consumes core facts and emits only the claims it owns. A kernel-oriented command can later orchestrate these lifters, but that command is only a convenience bundle. It does not own semantics.

## Components

### `libprovekit-c-lift-core`

Shared library used by all C-family lifters.

Responsibilities:

- Accept source text or source paths.
- Preserve stable source loci: path, line, column, and optional byte spans.
- Extract reusable C facts: function declarations/definitions, parameters, return types when available, call sites, macro invocations, attributes, comments, and basic block/function boundaries.
- Record parser backend provenance, including compile-command context when the AST backend is used.
- Attach nearby comments to declarations where this can be done deterministically.
- Report extraction visibility limits in `opacityReport`.
- Provide helpers for stable JSON emission, IR declaration construction, call-edge construction, diagnostics, and refusals.

The core does not decide that `__user` means user-address-space or that `KUNIT_EXPECT_EQ` is a witness. Those meanings belong to semantic lifters.

### `provekit-lift-c-sparse`

Lifts sparse-style annotations and related C address-space facts:

- `__user`
- `__kernel`
- `__iomem`
- `__rcu`
- `__must_hold`
- `__acquires`
- `__releases`

It emits contracts about address space, RCU pointer discipline, and lock-holding requirements when the annotation is visible in source facts.

### `provekit-lift-c-kernel-doc`

Lifts kernel-doc comments as contracts when they are deterministically attached to a function or type.

Initial fields:

- `Context:`
- `Return:`
- parameter descriptions
- brief function summary when it contains a recognized contract phrase

Malformed or unattached kernel-doc produces diagnostics. Missing comment context is opacity only when the source layout prevents deterministic attachment.

### `provekit-lift-c-lockdep`

Lifts lock discipline from lockdep and common lock primitives.

Initial vocabulary:

- `lockdep_assert_held`
- `lockdep_assert_held_once`
- `spin_lock`, `spin_unlock`
- `mutex_lock`, `mutex_unlock`
- acquire/release balance inside a single visible function

Non-local lock transfer, function-pointer mediated transfer, or ownership crossing an unmodeled callback boundary is a refusal when recognized.

### `provekit-lift-c-rcu`

Lifts RCU read-side and pointer-access contracts.

Initial vocabulary:

- `rcu_read_lock`
- `rcu_read_unlock`
- `rcu_dereference`
- `rcu_assign_pointer`
- `synchronize_rcu`
- `call_rcu`
- `__rcu` facts supplied by the sparse lifter when outputs are composed

The first slice should stay local and bounded. Grace-period and cross-thread claims require later witness backends or stronger models.

### `provekit-lift-c-errno`

Lifts kernel-style return conventions:

- negative `-errno`
- `NULL`
- `ERR_PTR`
- `IS_ERR`
- `IS_ERR_OR_NULL`
- `PTR_ERR`
- checked and unchecked error-pointer paths

Mixed `NULL` and `ERR_PTR` conventions are declarations only when the contract is explicit; otherwise the lifter emits diagnostics or refusals depending on what it can recognize.

### `provekit-lift-c-kunit`

Lifts KUnit assertions and expectations as test witness declarations:

- `KUNIT_EXPECT_*`
- `KUNIT_ASSERT_*`
- test case functions
- suite registration facts

KUnit is a witness surface, not the main place kernel contracts live. It should emit witness-shaped claims over observed assertions.

### `provekit-lift-c-assertions`

Lifts general C and kernel assertion macros:

- `assert`
- `BUILD_BUG_ON`
- `WARN_ON`
- `WARN_ON_ONCE`
- `BUG_ON`

`BUG_ON` is treated carefully as a hard-failure witness. It is not blindly converted into a general precondition unless the surrounding contract surface justifies that interpretation.

## Output Shape

Each lifter returns a deterministic result object:

```json
{
  "declarations": [],
  "callEdges": [],
  "diagnostics": [],
  "opacityReport": [],
  "refusals": []
}
```

Fields:

- `declarations`: positive contracts or witness declarations emitted by the lifter.
- `callEdges`: canonical call-edge records when the lifter can emit them soundly.
- `diagnostics`: ordinary warnings and errors about malformed source, malformed annotations, or user-actionable problems.
- `opacityReport`: extraction visibility limits.
- `refusals`: recognized semantic cases where the lifter saw enough to know the case but intentionally emitted no proof claim.

Outputs from multiple C-family lifters must be mergeable by canonical bytes and CID. Merge logic must preserve opacity and refusals as distinct streams.

## Opacity

Opacity means the lifter could not see a region or fact clearly enough. It is not a refusal.

Core opacity examples:

- parse recovery region
- missing include context
- unexpanded macro
- partially expanded macro
- inactive preprocessor branch
- inline assembly visibility limit
- generated-source uncertainty
- missing function body
- comment attachment ambiguity

Example:

```json
{
  "kind": "unexpanded-macro",
  "locus": {"path": "drivers/x/foo.c", "line": 42, "column": 9},
  "reason": "macro body unavailable in source snapshot",
  "affectedSurface": "lockdep"
}
```

Opacity policy is consumer-controlled. High-assurance policy may reject any opacity entry in selected files or surfaces; lower-assurance policy may permit opacity while recording it.

## Refusals

Refusal means the lifter recognized a semantic case and intentionally made no proof claim because the case is unsupported or unsound under the current model.

Example:

```json
{
  "kind": "unsupported-lock-transfer",
  "locus": {"path": "drivers/x/foo.c", "line": 80, "column": 5},
  "surface": "lockdep",
  "reason": "lock acquired in caller and released through function pointer"
}
```

Refusals are not extraction gaps. They are explicit boundaries in semantic coverage.

## Data Flow

1. A lifter receives source text or source paths through the existing JSON-RPC style.
2. The lifter calls `libprovekit-c-lift-core` to build stable source facts and core opacity.
3. The semantic lifter filters only facts it owns.
4. The semantic lifter emits declarations, call edges, diagnostics, opacity entries, and refusals.
5. A later orchestration layer can run multiple C-family lifters over the same corpus and merge their outputs deterministically.

## Contract Strength

Use three categories:

- **Contracts**: source syntax directly expresses an obligation, such as `__user`, `__must_hold`, `Context:`, or `Return:`.
- **Witnesses**: execution or test evidence says something held, such as KUnit assertions, later kselftest output, later lockdep reports, or later angr bounded symbolic evidence.
- **Refusals**: recognized semantic constructs where no sound claim is emitted.

Opacity is outside this strength ladder. It is about visibility, not claim strength.

## Testing Strategy

The first implementation should use focused fixtures rather than a full Linux kernel checkout.

Core tests:

- function, parameter, call, macro, attribute, and comment fact extraction
- stable source loci
- parser diagnostics
- opacity entries for missing include context, unexpanded macros, inactive branches, inline assembly, generated-source uncertainty, and missing bodies

Semantic lifter tests:

- sparse fixtures for `__user`, `__rcu`, `__must_hold`, `__acquires`, `__releases`
- kernel-doc fixtures for `Context:` and `Return:`
- lockdep fixtures for local lock assertions and acquire/release balance
- RCU fixtures for read-side critical sections and pointer access
- errno fixtures for `ERR_PTR`, `IS_ERR`, `IS_ERR_OR_NULL`, `PTR_ERR`, `NULL`, and `-errno`
- KUnit fixtures for expectation/assertion macros
- assertion fixtures for `assert`, `BUILD_BUG_ON`, `WARN_ON`, and `BUG_ON`

Composition tests:

- run two lifters over the same fixture
- merge declarations deterministically
- preserve opacity and refusals separately
- prove ordinary C files with no recognized surface emit empty declarations without fake failures

Full-kernel smoke tests are explicitly later work.

## First Implementation Slice

The first slice should be small enough to land safely:

1. Create the shared C lift core package with source input, fact structs, stable loci, result structs, and deterministic JSON helpers.
2. Port the current regex C LSP parser logic into the core as a temporary parser backend, preserving existing behavior.
3. Add opacity support to the core result shape.
4. Add refusal support to the core result shape.
5. Implement one semantic lifter first, preferably `provekit-lift-c-sparse`, because sparse annotations are the closest thing to native C/kernel contract syntax.
6. Add a composition fixture showing sparse plus assertions or sparse plus kernel-doc can coexist without sharing semantic code.
7. Add an opt-in libclang AST backend behind the same fact API. Keep the regex backend available for fallback and small fixtures, but make the backend choice visible on `pk_c_source_facts`.

The temporary regex backend can be retired later behind the same core fact model as libclang coverage becomes sufficient. Tree-sitter remains a possible lightweight fallback for editor-oriented facts.

## Decisions For Implementation

- Every standalone C-family lifter gets its own `.provekit/lift/<name>/manifest.toml` from the start, even when only one semantic lifter exists. The manifest is part of the lifter's identity.
- The first implementation emits `opacityReport` and `refusals` as first-class fields in the lifter result object. If the broader protocol later chooses a different catalog field, an adapter can translate without changing lifter semantics.
- KUnit source assertions and KUnit execution results are separate lifters. `provekit-lift-c-kunit` handles source-level KUnit assertion syntax; a future KUnit result lifter handles executed test output as a witness feed.
