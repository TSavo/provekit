# Java LSP Rebaseline Against Shared Protocol

Date: 2026-05-25
Issues: #1500, #1486
Authority: `protocol/specs/2026-05-25-lsp-shared-protocol.md` plus the
boundary tightening merged in #1520.
Mode: documentation/audit only. No runtime implementation is changed here.

## 1. Boundary Ruling

The Java LSP target is the shared route:

```text
initialize -> analyzeDocument -> lsp-document-analysis
```

The CLI/LSP coordinator remains language-agnostic. It routes document snapshots,
converts normalized diagnostics to editor diagnostics, asks linkerd/verifier for
project facts, and merges kit/linkerd/verifier state. It MUST NOT parse Java
source, infer Java source ranges, own Maven/JUnit/package semantics, or read kit
shim `.proof` files or body-template files as runtime body authority.

Java parsing, source-position mapping, concept/comment/package semantics,
framework extractors, materialize/refusal decisions, emitted JUnit artifacts,
and Java check status are Java kit-owned. Any linkerd `parseFile` route is a
legacy adapter only: it must call the owning Java kit helper, or a lossless
adapter to the Java helper, and must not make linkerd a Java parser.

No coordinator-owned sugar proof field, kit-shim proof-body lookup, or
coordinator body-template projection is part of this Java LSP slice.

## 2. Current Java Surfaces

| Surface | Current state | Evidence | Shared LSP fit |
|---|---|---|---|
| Java bind lift RPC | Current lift helper, not an LSP helper | `implementations/java/provekit-lift-java-source/src/main/java/com/provekit/lift/java_source/BindRpcServer.java` speaks `initialize`, `lift`, `shutdown` and returns a PEP 1.7 `ir-document`. | Useful delegate for `analyzeDocument`; missing `provekit-lsp-shared/1`, document CID, normalized `entries`, LSP diagnostics, statuses, and optional project state. |
| Java bind source walker | Current kit-owned Java parser/range source | `JavaBindLifter.java` uses the javac compiler API, emits `bind-lift-entry`, concept citations, `library-sugar-binding-entry`, diagnostics, method line data, and `body_source.span` for sugar entries. | Best input for shared LSP entries. Needs an adapter that wraps each current payload as `{kind, entry, range}` and fills missing ranges from kit-owned positions. |
| Java source-unit lifter | Current but not editor-shaped | `JavaSourceLifter.java` uses javac source positions, emits `function-contract` mementos, refusals, line-local locus, and parse diagnostics. | Useful for `concept-site`, `proof-site`, or lift-gap diagnostics where bind lift cannot classify a source site. Missing shared range shape and stable `provekit.lsp.*` codes. |
| Java legacy parse RPC | Stale/demo-only for shared LSP | `implementations/java/provekit-lift-java-core/src/main/java/com/provekit/lift/RpcServer.java` names itself `provekit-lsp-java`, exposes `parse` and `lift`, and returns `{declarations, callEdges, implications, warnings}` through `LiftHandler.parseSource`. | May remain a migration adapter, but it is not the target. It does not implement `analyzeDocument`, does not return `lsp-document-analysis`, and its warning/diagnostic shape is not shared LSP. |
| Java framework extractors | Current kit-owned semantics | `implementations/java/provekit-lift-java-{bean-validation,junit,jpa,hibernate,spring-web,spring-security,swagger,cofoja}/` provide Java/framework-specific extractors behind Java kit code. | Must remain below the Java helper boundary. Coordinator/linkerd must consume their normalized output only. |
| Java emit/materialize/check | Partial, kit-owned | `implementations/java/provekit-realize-java-core/` and `implementations/java/provekit-emit-java-junit/` own Java realization and JUnit emission. | Shared LSP needs statuses for `materialize`, `emit`, `check`, and `prove`; coordinator must not hardcode Maven/JUnit or Java package proof loading. |
| Historical Java LSP research | Stale as implementation guidance | `docs/research/2026-05-05-java-lsp-forward-propagator.md` says the Java LSP target is old `initialize` + `parse`. | Keep as history only. #1500 supersedes it with `analyzeDocument` and the shared result shape. |

## 3. Current Coordinator And Linkerd State

The Rust LSP coordinator in `implementations/rust/provekit-lsp/` is
language-agnostic in intent, but still speaks the older helper protocol:

- `src/plugin.rs` initializes helpers with `protocol_version =
  "provekit-lsp-plugin/1"` and calls `parse`, expecting `{annotations: [...]}`.
- `src/main.rs` documents the per-plugin route as `initialize` + `parse` and
  the daemon route as linkerd `parseFile`.
- Daemon diagnostics currently attach at `(0,0)..(0,1)` when linkerd lacks
  source locus, so source ranges are not yet preserved through the daemon path.

That coordinator state is a shared migration gap, not a reason for Java to emit
old Java-shaped data. The Java conformance target remains the shared helper
contract: `initialize` advertises `provekit-lsp-shared/1`, and
`analyzeDocument` returns `kind = "lsp-document-analysis"`.

## 4. Required Java `lsp-document-analysis` Mapping

The Java helper result should be:

```json
{
  "kind": "lsp-document-analysis",
  "schema_version": "1",
  "kit_id": "java",
  "uri": "file:///project/src/C.java",
  "file": "src/C.java",
  "document_cid": "blake3-512:...",
  "protocol_catalog_cid": "blake3-512:...",
  "entries": [],
  "diagnostics": [],
  "statuses": [],
  "project": null
}
```

Mapping from current Java surfaces:

- `JavaBindLifter.Result.ir[]` entries with `kind = "bind-lift-entry"` become
  shared `entries[]` items with `kind = "bind-lift-entry"`, `entry` equal to the
  existing payload, and `range` owned by the Java kit. Current `fn_line` is not
  enough; the adapter should use javac `SourcePositions` to emit
  `start_line/start_col/end_line/end_col` for the method or carrier.
- `library-sugar-binding-entry` payloads already carry `body_source.span` with
  line/column data; the adapter should promote that span to the enclosing shared
  entry range while preserving the payload unchanged.
- Concept citation comments and contract/comment carriers should become
  `concept-site` and/or `proof-site` entries when the Java kit can localize the
  source carrier. The coordinator must not scan comments itself.
- Java parse failures, lift gaps, malformed concept carriers, and CID mismatch
  cases should become `diagnostics[]` with stable codes such as
  `provekit.lsp.parse_error` and `provekit.lsp.lift_gap`, source `provekit`,
  producer `kit`, `kit_id = "java"`, and a source range.
- Materialize, emit, check, concept, link, and prove availability should become
  `statuses[]`. Java-specific check status must come from Java kit RPC or a
  Java kit status helper; Maven/JUnit semantics do not belong in the coordinator.
- `project` is optional for the Java helper. If present, its CIDs must come from
  linkerd/verifier/content-addressed producers, not coordinator invention.

All ranges use the shared convention: 1-based lines and 0-based columns. The
coordinator performs only the final conversion to native LSP 0-based positions.

## 5. Gaps To Implement After This Audit

### J-LSP-1: Add A Java Shared LSP Helper Or Adapter

Files:
- Add or modify under `implementations/java/provekit-lift-java-source/`.
- Reuse `BindRpcServer.java` and `JavaBindLifter.java`; do not fork a second
  Java source walker unless a later design proves that necessary.

Acceptance:
- `initialize` returns `protocol_version = "provekit-lsp-shared/1"`,
  `kit_id = "java"`, Java source surfaces, supported entry kinds, diagnostic
  codes, and status kinds.
- `analyzeDocument` accepts `{kit_id, uri, file, text, document_version,
  workspace_root, accepted_protocol_catalog_cids, policy_cids}`.
- The result kind is exactly `lsp-document-analysis`.
- Unit tests drive NDJSON over stdio and assert the shared shape.

### J-LSP-2: Emit Java-Owned Source Ranges For Every Normalized Entry

Files:
- `implementations/java/provekit-lift-java-source/src/main/java/com/provekit/lift/java_source/JavaBindLifter.java`
- Shared helper/adapter from J-LSP-1.

Acceptance:
- `bind-lift-entry` shared wrappers contain method or carrier ranges from javac
  source positions.
- `library-sugar-binding-entry` wrappers reuse existing `body_source.span`.
- Concept/comment carriers get localized ranges when the Java kit can map them.
- No Rust coordinator, linkerd, or CLI code scans Java source to fill ranges.

### J-LSP-3: Normalize Java Diagnostics

Files:
- Shared helper/adapter from J-LSP-1.
- Existing Java lifters where range data must be attached to parse/lift errors.

Acceptance:
- Parse failures emit `provekit.lsp.parse_error`.
- Omitted but localizable source sites emit `provekit.lsp.lift_gap`.
- Malformed concept citations and carrier CID mismatches emit stable
  `provekit.lsp.*` codes with Java-owned ranges.
- The old `{kind,message}` or `{severity,message}` diagnostic arrays are treated
  as internal adapter inputs only.

### J-LSP-4: Add Java Kit Status Helper Coverage

Files:
- `implementations/java/provekit-realize-java-core/`
- `implementations/java/provekit-emit-java-junit/`
- Shared helper/adapter from J-LSP-1.

Acceptance:
- `statuses[]` reports Java materialize availability/refusal for localized
  source sites.
- `statuses[]` reports JUnit emit availability and Java check pass/fail/stale
  states through Java kit RPC.
- Prove status is never green for zero claims; vacuous proof success becomes
  `provekit.lsp.vacuous_proof` or an equivalent non-green status.
- Coordinator code does not hardcode Maven, JUnit, or Java package proof lookup.

### J-LSP-5: Wire Coordinator Migration Without Java Leakage

Files:
- `implementations/rust/provekit-lsp/src/plugin.rs`
- `implementations/rust/provekit-lsp/src/main.rs`
- Linkerd adapter code that currently serves `parseFile`.

Acceptance:
- Coordinator can call `analyzeDocument` for Java helpers and preserve shared
  ranges/diagnostic codes.
- Legacy `parseFile` routes call the Java helper or a byte-preserving adapter to
  `lsp-document-analysis`.
- Linkerd and coordinator never parse Java source and never read Java kit shim
  `.proof` files as body authority.

### J-LSP-6: Add A Java Conformance Fixture

Files:
- Add a fixture near the existing LSP or Java conformance fixtures, for example
  `tests/lsp/shared-fixture/java/` or
  `implementations/java/conformance/fixtures/lsp_document_analysis/`.

Acceptance:
- The fixture exercises one Java source document with at least one concept
  citation or contract witness.
- It asserts `initialize -> analyzeDocument -> lsp-document-analysis`.
- It includes one normalized entry, one precise source range, one diagnostic or
  status, and one non-vacuous materialize/prove/check fact.
- The coordinator consumes the same fixture without adding Java parsing logic.

## 6. Tracker Impact

For #1500, the Java LSP state is:

- `lift`: partial/current. Java owns parsers and lifters, but shared LSP entry
  wrapping and full ranges are missing.
- `materialize`: partial. Java realization exists, but editor status reporting
  is not exposed through shared LSP.
- `emit/check`: partial. Java JUnit emit exists, but coordinator-safe status RPC
  is missing.
- `prove`: partial. Proof status must be surfaced as non-vacuous shared LSP
  status/diagnostics; no Java LSP path does that today.
- `LSP`: stale/demo-only. Current Java-facing LSP surfaces are legacy `parse`
  adapters or research notes, not the shared `analyzeDocument` protocol.

For #1486, this audit preserves the parity tracker invariant: Java kit owns
Java language/package/test semantics; coordinator/linkerd consume normalized
facts and project state only.
