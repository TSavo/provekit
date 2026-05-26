# Go LSP shared protocol rebaseline

Date: 2026-05-25

Authority: `protocol/specs/2026-05-25-lsp-shared-protocol.md`

Related issues: #1502, #314, #1486

Scope: audit plus first implementation slice. This note classifies the current
Go LSP surfaces against the shared LSP protocol after the #1520 boundary
tightening. PR #1506 now also adds the Go-kit `analyzeDocument` route in
`implementations/go/cmd/provekit-lsp-go/main.go`; the remaining tasks below are
the follow-on parity work, not a substitute for code.

## Boundary ruling

The shared target for Go is:

```text
initialize -> analyzeDocument -> lsp-document-analysis
```

The LSP coordinator is language-agnostic. It owns editor wiring, routing, cache
keys, conversion to editor diagnostics, and merging kit/linkerd/verifier facts.
It must not parse Go source, derive Go source ranges, own Go package or test
semantics, or read Go kit shim `*.proof` or body-template files as runtime body
authority.

Go parsing, comments, source ranges, package semantics, cgo resolution, test
framework semantics, materialize routing, emit availability, and check status
belong to the Go kit. Linkerd `parseFile` is a legacy adapter surface only. If a
Go document reaches linkerd through `parseFile`, that adapter must invoke the Go
kit helper, or a byte-preserving adapter to the same `lsp-document-analysis`
shape. Linkerd must not become a Go parser.

## Current Go surfaces

| Surface | Current code path | Status vs shared LSP | Notes |
|---|---|---|---|
| Go LSP helper | `implementations/go/cmd/provekit-lsp-go/main.go` | First shared slice implemented | Speaks NDJSON `initialize`, legacy `parse`, shared `analyzeDocument`, and `shutdown`. `analyzeDocument` returns `kind = "lsp-document-analysis"` with Go-owned ranges, entries, diagnostics, and explicit statuses. |
| Legacy Go LSP parsing | `implementations/go/cmd/provekit-lsp-go/main.go` functions `walkSource`, `walkCallEdges`, `scanAnnotations`, `findAheadFnSignature` | Go-owned but stale envelope | Uses Go-owned `go/parser`, `go/token`, `go/ast`, and line scanning. This is correctly kit-owned, but entries do not carry shared `source-range` wrappers. |
| Legacy forward propagation | `implementations/go/cmd/provekit-lsp-go/forward_propagator.go` | Demo-only child producer | Implements the older #314 floor around `checkPositive`. It emits LSP-shaped diagnostics with top-level code `implication-failed`; the shared code is `provekit.lsp.implication_failed`. Forward propagation remains child diagnostic work, not the whole LSP. |
| Go lift helper | `implementations/go/provekit-lift-go/rpc.go`, `implementations/go/provekit-lift-go/lift.go` | Useful owner, missing LSP wrapper | Speaks `initialize`, `lift`, `compile`, `shutdown` and returns `kind = "ir-document"`. It already owns Go parsing with `parser.ParseFile(..., parser.ParseComments)`, type info, diagnostics, refusals, and `SourceUnit` data. It should be wrapped for `analyzeDocument` rather than forked. |
| Go authoring annotations | `implementations/go/provekit-lift-go/annotation.go` | Current kit authority | The current authoring surface is `//provekit:boundary(...)` and `//provekit:sugar(...)`. The legacy LSP helper still scans older `//provekit:contract` and `//provekit:implement` comments, so it is stale against the Go kit's current declaration model. |
| Go lift registration | `implementations/go/.provekit/lift/go-source/manifest.toml` | Current lift config, not LSP config | Declares `protocol_version = "pep/1.7.0"`, `kind = "lift"`, and the `go-source` authoring surface. There is no shared LSP helper registration for `provekit-lsp-shared/1`. |
| Go materialize | `implementations/go/provekit-realize-go-core/rpc.go`, `implementations/go/provekit-realize-go-core/realizer.go` | Kit-owned, status gap | Speaks `provekit.plugin.invoke` for Go realization and returns explicit missing-template refusals. The LSP path does not yet expose materialize `available`, `unavailable`, or `refused` statuses for source sites. |
| Go emit | `implementations/go/provekit-emit-go-testing/rpc.go`, `implementations/go/provekit-emit-go-testing/emitter.go` | Kit-owned emitter exists, LSP status gap | Emits native Go `testing` artifacts from neutral predicates. LSP does not yet report emit status per source site. |
| Go check semantics | `implementations/rust/provekit-cli/src/cmd_emit.rs` | Leakage outside LSP, tracked by #1484 | The CLI still has a hardcoded `go test ./...` path. The shared LSP coordinator must not copy that pattern; LSP check status must come from a Go kit status helper or a normalized RPC result. |
| Coordinator plugin route | `implementations/rust/provekit-lsp/src/main.rs`, `implementations/rust/provekit-lsp/src/config.rs` | Legacy coordinator route | The coordinator can spawn external helpers through `initialize`/`parse`/`shutdown`. It does not target `analyzeDocument` for Go yet. |
| Linkerd route | `implementations/rust/provekit-linker/src/lib.rs`, `implementations/rust/provekit-lsp/src/main.rs` daemon mode | Legacy adapter only | Current docs and code talk about `parseFile` streams. Under the shared spec, a Go `parseFile` path must adapt from the Go kit's shared analysis result, not parse Go in linkerd. |

## Shared protocol gaps for Go

1. **Shared method set.** Implemented in PR #1506: `initialize` advertises
   `protocol_version = "provekit-lsp-shared/1"`, `kit_id = "go"`, and
   `analyzeDocument`; `analyzeDocument` returns `kind =
   "lsp-document-analysis"`. Remaining work is coordinator/linkerd consumption.

2. **Entry envelope.** The legacy `parse` result returns raw `declarations` and
   `callEdges`. The shared result needs `entries` containing
   `bind-lift-entry`, `library-sugar-binding-entry`, `call-edge`,
   `concept-site`, or `proof-site` entries, each with Go-owned source ranges.

3. **Source ranges.** The current forward-propagation diagnostic range is an
   LSP-style 0-based range. The shared kit result uses 1-based lines and
   0-based columns; the coordinator converts that to editor-native LSP
   positions. Go must compute ranges from the submitted document snapshot and
   preserve them through entries, diagnostics, and statuses.

4. **Annotation model.** Implemented for `analyzeDocument`: the shared route
   delegates to `provekit-lift-go` with `AnnotatedOnly` and emits
   `provekit.lsp.lift_gap` for old `//provekit:contract` /
   `//provekit:implement` comments. The legacy `parse` method still preserves
   its old behavior for migration clients.

5. **Diagnostic codes.** Forward propagation must emit the stable shared code
   `provekit.lsp.implication_failed`. Parse errors, lift gaps, materialize
   gaps, emit gaps, check failures, unresolved symbols, unprovable obligations,
   and vacuous proof results must use the stable `provekit.lsp.*` codes from
   the shared spec.

6. **Statuses.** Go LSP does not yet return `statuses` for materialize, emit,
   check, prove, concept, or link state. Materialize status should route to the
   Go realizer. Emit/check status should route through the Go testing emitter
   and Go-owned check helper, not through coordinator-owned `go test` logic.
   Prove status must distinguish nonzero proof receipts from vacuous success.

7. **Linkerd migration.** The Go path has no adapter from
   `lsp-document-analysis` into linkerd project facts. During migration,
   linkerd `parseFile` may remain a wire method, but its Go implementation must
   consume the Go helper's shared analysis or an exact adapter to it.

8. **Conformance fixture.** There is no Go fixture proving
   `initialize -> analyzeDocument -> lsp-document-analysis` with one Go source
   site, one source range, and one non-vacuous status or diagnostic.

## Reconciliation of #314

#314 remains valid only as child diagnostic work. The old issue asks for a
thin Go forward-propagation loop that accumulates caller postconditions and
emits callsite diagnostics. That producer should consume normalized Go facts
from `analyzeDocument` and linkerd/verifier state. It should not define the Go
LSP architecture by itself, own Go parsing outside the Go kit helper, or report
proof success on a zero-claim route.

The current `implementations/go/cmd/provekit-lsp-go/forward_propagator.go`
implementation is therefore a demo-only seed. It is useful test material for
the future producer, but it is not the shared LSP target for #1502.

## Implementable child tasks

### Task 1: Add a Go shared LSP helper adapter

Files:

- Modify: `implementations/go/cmd/provekit-lsp-go/main.go`
- Reuse: `implementations/go/provekit-lift-go/rpc.go`
- Reuse: `implementations/go/provekit-lift-go/lift.go`
- Test: `implementations/go/cmd/provekit-lsp-go/main_test.go`

Acceptance:

- Done in PR #1506: `initialize` returns `protocol_version = "provekit-lsp-shared/1"` and
  `kit_id = "go"` while preserving migration support for legacy clients.
- Done in PR #1506: `analyzeDocument` accepts `kit_id`, `uri`, `file`, `text`,
  `document_version`, `workspace_root`, `accepted_protocol_catalog_cids`, and
  `policy_cids`.
- Done in PR #1506: The result has `kind = "lsp-document-analysis"`, `schema_version = "1"`,
  `kit_id = "go"`, `uri`, `file`, `document_cid`,
  `protocol_catalog_cid`, `entries`, `diagnostics`, `statuses`, and `project`.
- Done in PR #1506: The adapter delegates Go parsing to `provekit-lift-go` code or a lossless
  wrapper around it. It does not add a second source walker in coordinator or
  linkerd code.
- Focused test: `cd implementations/go && go test ./cmd/provekit-lsp-go`.

### Task 2: Replace legacy Go LSP annotation scanning

Files:

- Modify: `implementations/go/cmd/provekit-lsp-go/main.go`
- Reuse: `implementations/go/provekit-lift-go/annotation.go`
- Test: `implementations/go/cmd/provekit-lsp-go/main_test.go`
- Test: `implementations/go/provekit-lift-go/annotation_test.go`

Acceptance:

- `//provekit:boundary(...)` and `//provekit:sugar(...)` are the source of
  LSP entries for Go authoring declarations.
- Older `//provekit:contract` or `//provekit:implement` comments either map
  through an explicit legacy adapter or produce `provekit.lsp.lift_gap`
  diagnostics. They are not silently treated as the current authoring model.
- Malformed Go authoring directives produce source-local diagnostics with
  Go-owned ranges.
- Focused test: `cd implementations/go && go test ./cmd/provekit-lsp-go ./provekit-lift-go`.

### Task 3: Add source-range conformance

Files:

- Modify: `implementations/go/cmd/provekit-lsp-go/main.go`
- Modify: `implementations/go/provekit-lift-go/types.go` if shared range types
  are placed in the lift package
- Test: `implementations/go/cmd/provekit-lsp-go/main_test.go`

Acceptance:

- Every returned entry, diagnostic, and status tied to source has a shared
  `source-range` with 1-based lines and 0-based columns.
- Ranges point into the exact `text` bytes submitted to `analyzeDocument`.
- Existing LSP client conversion remains coordinator-owned.
- Focused test: `cd implementations/go && go test ./cmd/provekit-lsp-go`.

### Task 4: Add Go materialize, emit, check, and prove statuses

Files:

- Modify: `implementations/go/cmd/provekit-lsp-go/main.go`
- Reuse: `implementations/go/provekit-realize-go-core/rpc.go`
- Reuse: `implementations/go/provekit-emit-go-testing/rpc.go`
- Reuse: `implementations/go/provekit-emit-go-testing/emitter.go`
- Test: `implementations/go/cmd/provekit-lsp-go/main_test.go`

Acceptance:

- Source sites return `materialize` statuses from Go realizer availability or
  refusal.
- Source sites return `emit` statuses from the Go testing emitter.
- Source sites return `check` statuses only from a Go-owned check helper or
  normalized kit RPC result.
- Source sites return `prove` statuses only for nonzero proof receipts or an
  explicit vacuous-proof diagnostic.
- No coordinator code shells out to `go test` for this LSP status.

### Task 5: Add the Go linkerd migration adapter

Files:

- Modify: `implementations/rust/provekit-lsp/src/main.rs`
- Modify: `implementations/rust/provekit-lsp/src/config.rs`
- Modify: `implementations/rust/provekit-linker/src/lib.rs` only if the
  project-fact input shape needs a documented shared adapter boundary
- Test: `implementations/rust/provekit-lsp/tests/daemon_routed.rs`

Acceptance:

- The Go coordinator route calls the Go shared helper or consumes a stored
  `lsp-document-analysis` result.
- Linkerd accepts normalized Go facts without parsing Go.
- Existing `parseFile` remains a legacy wire adapter, not an ownership claim.
- Diagnostics preserve stable shared codes after coordinator conversion.

### Task 6: Add a Go shared LSP conformance fixture

Files:

- Create: `tests/lsp/shared-fixture/go.json` or the closest existing fixture
  path chosen by the conformance harness.
- Modify: the focused LSP fixture test that validates shared helper output.

Acceptance:

- The fixture drives `initialize -> analyzeDocument`.
- The analyzed Go document contains one current Go authoring site, one precise
  range, and at least one non-vacuous materialize, emit, check, prove, link, or
  diagnostic fact.
- The expected result has `kind = "lsp-document-analysis"`.
- The fixture fails if Go source is parsed by coordinator/linkerd code.

## Close criteria for #1502

#1502 can close when this audit is linked from the PR and the child tasks above
are opened or scheduled as implementation work. It should not close #314. #314
should be retitled or reworded as the Go forward-propagation diagnostic
producer under the shared LSP protocol.

#1486 should keep Go LSP marked as a gap until the shared method set,
Go-owned ranges, statuses, and linkerd adapter are implemented.
