# Rust LSP Shared-Protocol Rebaseline Audit

Date: 2026-05-25
Scope: #1503, reconciles #313 under #1486
Authority: `protocol/specs/2026-05-25-lsp-shared-protocol.md`, including the #1520 boundary tightening.
Mode: audit plus first Rust helper implementation slice. PR #1507 now contains
code for the Rust kit `initialize -> analyzeDocument -> lsp-document-analysis`
route in `implementations/rust/provekit-lsp-rust`.

## Ruling

The Rust LSP surface is partial and legacy relative to the shared LSP protocol.
The target route is:

```text
initialize -> analyzeDocument -> lsp-document-analysis
```

Rust source parsing, source ranges, framework semantics, test semantics,
package semantics, materialize, emit, and check status are Rust kit-owned even
though the ProvekIt CLI is written in Rust. The shared LSP coordinator and
`provekit-linkerd` may route document snapshots, merge normalized facts, query
project state, and convert diagnostics, but they must not parse Rust source or
read kit package proof artifacts as body authority.

`parseFile` is retained only as a migration adapter. For Rust it must invoke the
owning Rust kit helper, or a byte-preserving adapter to that helper, and consume
the same `lsp-document-analysis` shape the coordinator consumes. It must not make
linkerd the Rust parser.

## Current Surfaces

| Surface | Code path | Current status | Shared-protocol verdict |
|---|---|---|---|
| Shared protocol authority | `protocol/specs/2026-05-25-lsp-shared-protocol.md` | Current. Defines `provekit-lsp-shared/1`, `initialize`, `analyzeDocument`, `kind = "lsp-document-analysis"`, stable `provekit.lsp.*` diagnostic codes, statuses, project pins, and coordinator/linkerd prohibitions. | Fixed authority for #1503. |
| Rust kit LSP helper | `implementations/rust/provekit-lsp-rust/src/main.rs` | Kit-owned Rust parser/lifter. Parses live text with `syn`, runs Rust lift adapters, returns the shared `lsp-document-analysis` envelope from `analyzeDocument`, and keeps legacy `parse` as a projection of the same analysis core. Materialize/emit/check/prove are explicit `unknown` kit statuses until backend status RPCs are wired. | First #1503 implementation slice is present. Remaining work is richer normalized entries, real backend status RPCs, coordinator consumption, and linkerd adapter tightening. |
| Rust kit forward-propagator | `implementations/rust/provekit-lsp-rust/src/forward_propagator.rs`, `implementations/rust/provekit-lsp-rust/tests/forward_propagator.rs` | Demo/floor implementation for old #313. It uses a small statement IR and a synthetic `checkPositive` scanner. Diagnostics use old code `implication-failed` while `data.kind` uses `provekit.lsp.implication_failed`. | Child work under #313. It is a diagnostic producer under shared LSP, not the whole LSP. It must consume normalized Rust kit facts and emit stable `provekit.lsp.implication_failed`. |
| Tower LSP coordinator | `implementations/rust/provekit-lsp/src/main.rs`, `implementations/rust/provekit-lsp/src/config.rs` | Advertises editor LSP capabilities and has per-plugin and daemon modes. It also has `LanguageHandle::BuiltinRust`, default `parser = "builtin:rust"`, and falls back to `parser::parse_rust_source` for `.rs` files. | Non-conforming production path. The coordinator must route Rust documents to the Rust kit helper and preserve returned ranges; it must not parse Rust source. |
| Coordinator built-in Rust parser | `implementations/rust/provekit-lsp/src/parser.rs` | Uses `syn` in the coordinator to extract `#[provekit::implement]`, `#[provekit::contract]`, and `#[provekit::verify]` annotations. | Stale. Move this ownership behind the Rust kit helper or remove the production route. |
| Coordinator plugin adapter | `implementations/rust/provekit-lsp/src/plugin.rs` | Speaks `provekit-lsp-plugin/1` with `initialize`, `parse`, and `shutdown`, then decodes an `annotations` array. | Legacy adapter only. Add a shared-protocol client path for `provekit-lsp-shared/1` and `analyzeDocument`; project the result into old annotation views only for migration. |
| Linkerd `parseFile` | `implementations/rust/provekit-linkerd/src/methods.rs` | Accepts `{kitId,file,source}` and calls `lift_source`. The Rust branch uses in-process `provekit_lift::lift_path`; other kits mostly spawn legacy `parse` helpers. | Non-conforming for Rust after #1520. `parseFile` must call the Rust kit helper or a lossless adapter to `lsp-document-analysis`; linkerd must not own Rust parsing. |
| Linkerd project state | `implementations/rust/provekit-linkerd/src/state.rs` | Maintains contract/call-edge streams and project pins in memory. | Useful as project-state producer. The coordinator may merge its pins into `project`, but linkerd does not replace kit analysis. |
| Rust lift helper | `implementations/rust/provekit-lift/src/lib.rs`, `implementations/rust/.provekit/lift/rust/manifest.toml` | Rust-owned source walker and adapter dispatcher. RPC mode exposes `lift`, with `ir-document` and proof-envelope shapes. | Current kit substrate to wrap. It lacks live-document `analyzeDocument`, document CIDs, source ranges for every emitted entry, and LSP statuses. |
| Materialize/check surfaces | `implementations/rust/provekit-cli/src/cmd_materialize.rs` | Same-language and discovery paths include Rust-specific boundary scanning, carrier injection, `use` splicing, and compile checks using `cargo check` or `rustc`. | Do not copy into LSP/coordinator. LSP must surface materialize/check status from a Rust kit status helper, not from coordinator-owned Rust syntax or Cargo decisions. |
| Emit/check surfaces | `implementations/rust/provekit-cli/src/cmd_emit.rs`, `implementations/rust/provekit-cli/src/kit_dispatch.rs` | `dispatch_emit` is kit-dispatch based, but compile-check still hardcodes several target commands and Rust emit/check registration remains tracked by #1493. | LSP status is missing. Rust emit/check availability and results must come from kit RPC/status, not hardcoded coordinator logic. |
| Prove/verify surface | `implementations/rust/provekit-cli/src/cmd_verify.rs` | Emits JSON receipts with `totalClaims`, per-claim status, witness CIDs, and an explicit empty-catalog note when there are zero claims. | LSP prove status must not show zero-claim success as proof success. Map zero-claim/empty-catalog cases to a warning or unknown status, and use `provekit.lsp.vacuous_proof` where a route reports a vacuous green state. |

## Shared Protocol Mapping

| Requirement | Current Rust state | Gap |
|---|---|---|
| `initialize` returns `protocol_version = "provekit-lsp-shared/1"`, `kit_id = "rust"`, and capabilities. | `provekit-lsp-rust` returns name/version/capabilities for legacy `parse`; no shared protocol token. | Add shared initialize response and keep legacy initialize only for adapters. |
| `analyzeDocument` accepts full live text, URI/file, document version, accepted protocol catalog CIDs, and policy CIDs. | Rust helper accepts legacy `parse` params `{path, source}`; coordinator plugin path accepts `{uri,text}` for old annotations. | Add `analyzeDocument` as the primary Rust helper method. |
| Result `kind = "lsp-document-analysis"`, `schema_version = "1"`, `kit_id = "rust"`, `document_cid`, `protocol_catalog_cid`, `entries`, `diagnostics`, `statuses`, `project`. | No current Rust helper returns this envelope. | Build a lossless envelope around Rust lift output, diagnostics, statuses, and optional linkerd pins. |
| Entries include bind lift entries, library-sugar binding entries, call edges, concept sites, proof sites, and source ranges. | Rust lift emits declarations and call-edge data in other shapes. Forward-prop has ranges for a synthetic fixture only. | Map existing Rust lift/lsp outputs into shared entries with 1-based lines and 0-based columns, backed by Rust kit parsing. |
| Diagnostics use stable `provekit.lsp.*` codes. | Rust forward-prop code is `implication-failed`; daemon mapping emits `provekit:<kind>`. | Normalize to `provekit.lsp.implication_failed`, `provekit.lsp.unresolved_symbol`, `provekit.lsp.unprovable_obligation`, parse/lift-gap codes, and vacuous-proof warning. |
| Statuses cover materialize, emit, check, prove, concept, and link state. | No Rust LSP helper status envelope exists. | Add Rust kit-owned status helper or extend `analyzeDocument` to report availability/refusal/passed/failed/stale state. |
| Coordinator never parses host source. | `provekit-lsp` has built-in Rust parsing and default `.rs` fallback. | Remove production built-in Rust parse route; all Rust parsing goes through the Rust kit helper. |
| Linkerd legacy `parseFile` is adapter-only. | `provekit-linkerd` directly calls Rust lift in-process. | Rework `parseFile` to call Rust helper `analyzeDocument` or a lossless adapter, then map normalized entries into linker streams. |

## Child Implementation Tasks

1. **Rust helper shared protocol**
   - Modify: `implementations/rust/provekit-lsp-rust/src/main.rs`
   - Create or modify tests: `implementations/rust/provekit-lsp-rust/tests/shared_protocol.rs`
   - Work: add shared `initialize` metadata, `analyzeDocument`, `document_cid`, `protocol_catalog_cid`, `kind = "lsp-document-analysis"`, and deterministic response ordering.
   - Status: implemented as the first thin route in PR #1507. The route wraps Rust lift output, Rust-owned sugar/test sites, normalized diagnostics, source ranges, and explicit kit status placeholders.
   - Acceptance: a fixture drives `initialize -> analyzeDocument` over NDJSON and asserts `kit_id = "rust"`, `kind = "lsp-document-analysis"`, nonempty `document_cid`, stable diagnostic codes, source ranges inside the submitted text, and materialize/emit/check/prove status rows.

2. **Legacy parse projection**
   - Modify: `implementations/rust/provekit-lsp-rust/src/main.rs`
   - Work: make legacy `parse` call the same analysis core as `analyzeDocument` and project the old `declarations`/`warnings`/`diagnostics` shape from the shared result.
   - Acceptance: existing `provekit-lsp-rust` tests pass, and no second Rust source walker is introduced for legacy parse.

3. **Coordinator Rust parser removal**
   - Modify: `implementations/rust/provekit-lsp/src/main.rs`, `implementations/rust/provekit-lsp/src/config.rs`, `implementations/rust/provekit-lsp/src/plugin.rs`
   - Delete or retire from production: `implementations/rust/provekit-lsp/src/parser.rs`
   - Work: route `.rs` documents to an external Rust helper speaking `provekit-lsp-shared/1`; convert shared diagnostics/statuses to editor LSP objects.
   - Acceptance: `rg 'BuiltinRust|builtin:rust|parser::parse_rust_source|parse_rust_source' implementations/rust/provekit-lsp/src` has no production path, and focused `provekit-lsp` tests still cover hover/code lens/diagnostic conversion from shared entries.

4. **Linkerd parseFile adapter tightening**
   - Modify: `implementations/rust/provekit-linkerd/src/methods.rs`
   - Work: replace Rust in-process lift with a call to `provekit-lsp-rust analyzeDocument` or a lossless in-process adapter that returns the exact shared result. Map only normalized `entries` into `LinkerContract` and `LinkerCallEdge`.
   - Acceptance: `parseFile` remains wire-compatible, but `rg 'provekit_lift::lift_path|lift_rust_source' implementations/rust/provekit-linkerd/src` does not find a Rust-source ownership path.

5. **Rust LSP statuses**
   - Modify: `implementations/rust/provekit-lsp-rust/src/main.rs`
   - Consider creating: `implementations/rust/provekit-lsp-rust/src/status.rs`
   - Work: add materialize, emit, check, prove, concept, and link statuses from Rust kit-owned helpers and linkerd project pins. Do not hardcode Cargo or Rust package rules in the coordinator.
   - Acceptance: a Rust fixture with one concept or contract site returns at least one status row with a source range and `producer` set to the owning component.

6. **#313 forward-propagation child rebaseline**
   - Modify: `implementations/rust/provekit-lsp-rust/src/forward_propagator.rs`
   - Work: consume normalized entries/call edges from `lsp-document-analysis`, stop scanning live Rust text for synthetic calls, and emit `provekit.lsp.implication_failed`.
   - Acceptance: old #313 tests are rewritten around normalized document facts; top-fallback suppression remains covered.

7. **Proof/vacuous-status mapping**
   - Modify: Rust helper status code from task 5 and coordinator diagnostic conversion.
   - Work: expose verifier receipts as `prove` statuses only for nonzero real claims. Empty catalogs or zero-claim routes are `unknown`/warning, not proof success.
   - Acceptance: a zero-claim fixture yields no `state = "passed"` prove status and emits `provekit.lsp.vacuous_proof` if any producer reports a green vacuous route.

## Prohibitions For This Slice

- Do not add Rust parsing to the shared coordinator.
- Do not add Rust parsing to `provekit-linkerd`.
- Do not treat kit package proof artifacts as body authority in the coordinator
  or linkerd.
- Do not hardcode Cargo, rustc, test-framework, package, materialize, or emit
  semantics in the LSP coordinator.
- Do not treat #313 forward propagation as the whole Rust LSP. It is one
  diagnostic producer under the shared protocol.
