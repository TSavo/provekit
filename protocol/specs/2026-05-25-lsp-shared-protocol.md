# LSP Shared Protocol

**Version:** v0.1.0
**Date:** 2026-05-25
**Status:** cataloged by `protocol/catalogs/provekit-lsp-shared-1.catalog.json`
with `protocol_catalog_cid =
blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c`.
**Author:** T Savo
**Companion specs:** [Linker Daemon Protocol](2026-05-04-linker-daemon-protocol.md), [Bridge Linkage Protocol](2026-05-03-bridge-linkage-protocol.md), [Lift Plugin Protocol](2026-04-30-lift-plugin-protocol.md), [Plugin Extension Protocol](2026-05-12-plugin-protocol.md), [Bind-IR Lift-Result Shape](2026-05-13-bind-ir-lift-result.md), [Body Template Memento](2026-05-13-body-template-memento.md), [Concept Hub Abstraction Layer](2026-05-15-concept-hub-abstraction-layer.md)

Terms: MUST, SHALL, SHOULD, MAY per RFC 2119.

## §0. Purpose

This spec defines the shared protocol boundary for ProvekIt editor integrations.
The LSP surface is the editor-facing consumer of the current substrate: ProofIR
claims, `concept:*` bindings, call edges, materialize/refusal state, emit/check
state, and proof receipts. It is not a transpiler, not a source-to-source
converter, and not a standalone verifier.

The protocol has one job: turn a live editor document into kit-owned normalized
facts with source ranges, then turn substrate and kit status back into editor
diagnostics, hovers, code lenses, and future inline hints.

The architecture is:

```text
Editor
  -> LSP coordinator
  -> kit LSP helper / kit lift helper
  -> normalized LSP document facts
  -> provekit-linkerd / provekit verifier / kit status RPC
  -> editor diagnostics, hovers, lenses, hints
```

The LSP coordinator is language-agnostic. Every language-specific parse,
source-position, framework, test, package, materialize, emit, and check decision
belongs to the owning kit.

## §1. Non-goals

The following are explicitly out of scope:

1. Java-to-Rust, Rust-to-Java, or any other source-to-source transpilation.
2. Editor-driven conversion between host languages.
3. Language parsing in the Rust CLI, the shared LSP coordinator, or linkerd.
4. CLI/LSP coordinator reads of kit shim `.proof` files as body authority.
5. Hardcoded Maven, pytest, `go test`, Cargo, or other host check semantics in
   the coordinator.
6. Treating old forward propagation as the whole LSP architecture.
7. Returning `totalClaims: 0` or an equivalent vacuous green state as proof
   success.

Forward propagation remains in scope as one diagnostic producer. It consumes
normalized pre/post/call-edge facts and emits implication diagnostics. It does
not define the shared protocol by itself.

## §2. Roles

**Editor client.** VS Code, Neovim, Emacs, or another LSP-aware client. It speaks
standard Language Server Protocol to the coordinator.

**LSP coordinator.** The process that owns LSP wiring: document sync, hover,
code lens, diagnostics publication, code actions, routing, cache invalidation,
and conversion between shared ProvekIt facts and LSP messages. It MUST NOT parse
host-language source.

**Kit LSP helper.** A per-kit helper that accepts live document snapshots and
returns normalized facts plus source ranges. It MAY be a thin wrapper around the
kit's PEP 1.7.0 `kind = "lift"` helper. It owns source parsing, source ranges,
framework semantics, and host-language diagnostics.

**Kit status helper.** A per-kit helper that reports materialize, emit, check,
and package/proof-body availability for source sites owned by that kit. It MAY
be the same process as the kit LSP helper.

**Linker daemon.** `provekit-linkerd`, one process per project, owns the hot
project union of contracts, call edges, derived bridges, and link-bundle state.
It exposes project diagnostics and pins at editor speed.

**Verifier/prover.** The proof engine that checks nonzero claims and emits proof
receipts. It is not embedded in every kit LSP helper.

## §3. Transport

Kit LSP helpers SHALL speak JSON-RPC 2.0 over NDJSON on stdio, matching the
existing ProvekIt plugin family.

The protocol version token for this shared surface is:

```text
provekit-lsp-shared/1
```

The coordinator MAY accept legacy helper shapes during a migration window, but
new in-tree helpers SHOULD emit `provekit-lsp-shared/1` from their `initialize`
result and SHOULD expose the method set in §4.

## §4. Methods

### §4.1 `initialize`

Request:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "client": {"name": "provekit-lsp", "version": "0.0.0"},
    "protocol_version": "provekit-lsp-shared/1",
    "workspace_root": "/project",
    "config_path": ".provekit/config.toml"
  }
}
```

Result:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "name": "provekit-lsp-python",
    "version": "0.1.0",
    "protocol_version": "provekit-lsp-shared/1",
    "kit_id": "python",
    "protocol_catalog_cid": "blake3-512:...",
    "capabilities": {
      "source_surfaces": ["python-source"],
      "entry_kinds": ["bind-lift-entry", "library-sugar-binding-entry", "call-edge"],
      "diagnostic_codes": ["provekit.lsp.parse_error"],
      "status_kinds": ["materialize", "emit", "check", "prove"]
    }
  }
}
```

### §4.2 `analyzeDocument`

`analyzeDocument` is the required live-editor method. It replaces the older
split between LSP-only `parse` and lift-only `lift` for editor use. A kit MAY
implement it by delegating to its existing lift helper, but the shared LSP result
shape is fixed here.

Request:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "analyzeDocument",
  "params": {
    "kit_id": "python",
    "uri": "file:///project/src/demo.py",
    "file": "/project/src/demo.py",
    "text": "def f(x):\n    return x\n",
    "document_version": 42,
    "workspace_root": "/project",
    "accepted_protocol_catalog_cids": ["blake3-512:..."],
    "policy_cids": []
  }
}
```

Result:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "kind": "lsp-document-analysis",
    "schema_version": "1",
    "kit_id": "python",
    "uri": "file:///project/src/demo.py",
    "file": "src/demo.py",
    "document_cid": "blake3-512:...",
    "protocol_catalog_cid": "blake3-512:...",
    "entries": [],
    "diagnostics": [],
    "statuses": [],
    "project": null
  }
}
```

### §4.3 `shutdown`

The helper SHALL flush local caches and exit cleanly after `shutdown`.

## §5. Analysis Result Shape

```cddl
cid = tstr
uri = tstr
json-value = any

lsp-document-analysis = {
  kind: "lsp-document-analysis",
  schema_version: "1",
  kit_id: kit-id,
  uri: uri,
  file: tstr,
  document_cid: cid,
  protocol_catalog_cid: cid,
  entries: [* lsp-entry],
  diagnostics: [* lsp-diagnostic],
  statuses: [* lsp-status],
  project: lsp-project-state / null
}

kit-id = "rust" / "go" / "cpp" / "csharp" / "python" / "ruby" /
         "swift" / "ts" / "zig" / "java" / "c" / "php" / tstr

lsp-entry = bind-entry-ref / library-sugar-binding-ref / call-edge-ref /
            concept-site-ref / proof-site-ref / lsp-extension-entry

bind-entry-ref = {
  kind: "bind-lift-entry",
  entry: json-value,
  range: source-range
}

library-sugar-binding-ref = {
  kind: "library-sugar-binding-entry",
  entry: json-value,
  range: source-range
}

call-edge-ref = {
  kind: "call-edge",
  entry: json-value,
  range: source-range
}

concept-site-ref = {
  kind: "concept-site",
  concept_name: tstr,
  ? concept_cid: cid,
  ? citation_payload_cid: cid,
  range: source-range
}

proof-site-ref = {
  kind: "proof-site",
  site_id: tstr,
  ? contract_cid: cid,
  ? attestation_cid: cid,
  range: source-range
}

lsp-extension-entry = {
  kind: tstr,
  * tstr => json-value
}

source-range = {
  start_line: uint,
  start_col: uint,
  end_line: uint,
  end_col: uint
}
```

`entry` carries the existing normative payload for that entry kind. For example,
a `bind-lift-entry` is the object from `2026-05-13-bind-ir-lift-result.md`, and
a `library-sugar-binding-entry` is the library-sugar binding object from the
same spec. This shared protocol does not fork those payloads. It adds editor
range and routing context around them.

`document_cid` is `blake3-512` over the exact UTF-8 document bytes submitted in
the `analyzeDocument` request. A helper that receives only a content hash instead
of text MUST validate cached text agreement before emitting the result.

All ranges use 1-based lines and 0-based columns. The LSP coordinator converts
to the editor client's native 0-based LSP positions. Producers MUST ensure
`start_line:start_col` is not after `end_line:end_col`.

## §6. Diagnostics

Diagnostics use source string `provekit` after conversion to LSP. Kit helpers
and daemons SHALL emit stable codes and MUST include a source range whenever the
diagnostic is tied to source. Whole-file diagnostics MAY use the first byte of
the document, but producers SHOULD prefer precise ranges.

```cddl
lsp-diagnostic = {
  code: diagnostic-code,
  message: tstr,
  severity: "error" / "warning" / "information" / "hint",
  range: source-range,
  producer: diagnostic-producer,
  ? kit_id: kit-id,
  ? protocol_catalog_cid: cid,
  ? related_cids: [* cid],
  ? data: json-value
}

diagnostic-producer = "kit" / "linkerd" / "verifier" / "materialize" /
                      "emit" / "check" / "lsp-coordinator" / tstr

diagnostic-code = tstr
```

Initial stable diagnostic codes:

| Code | Producer | Meaning |
|---|---|---|
| `provekit.lsp.parse_error` | kit | The kit could not parse the live document snapshot. |
| `provekit.lsp.lift_gap` | kit | The kit parsed the source but could not lift a source site into normalized facts. |
| `provekit.lsp.catalog_mismatch` | kit/coordinator | The helper used a protocol catalog CID that does not match project policy. |
| `provekit.lsp.materialize_unavailable` | materialize | The site has no resolvable materialize route for the requested target/library tuple. |
| `provekit.lsp.materialize_refused` | materialize | The route exists but refuses under policy or loss budget. |
| `provekit.lsp.emit_unavailable` | emit | The kit cannot emit the requested test/check artifact for this site. |
| `provekit.lsp.check_failed` | check | A kit-owned host check failed. |
| `provekit.lsp.unresolved_symbol` | linkerd | A call edge target could not be resolved in the project union. |
| `provekit.lsp.unprovable_obligation` | linkerd/verifier | A call-site or bridge obligation could not be discharged. |
| `provekit.lsp.implication_failed` | forward propagation | Current post facts do not establish the callee precondition. |
| `provekit.lsp.vacuous_proof` | verifier | A proof path reported success without nonzero claims. |

Forward propagation diagnostics MUST use `provekit.lsp.implication_failed` and
SHOULD include `callee`, `callee_contract_cid`, `callee_pre_cid`,
`current_post_cid`, and `missing_conjuncts` in `data`.

## §7. Statuses

Statuses are non-error editor facts that drive hovers, code lenses, and future
inline hints. They are separate from diagnostics so the editor can show useful
state without implying failure.

```cddl
lsp-status = {
  kind: status-kind,
  range: source-range,
  state: "available" / "unavailable" / "refused" / "unknown" /
         "passed" / "failed" / "stale",
  producer: diagnostic-producer,
  ? message: tstr,
  ? related_cids: [* cid],
  ? data: json-value
}

status-kind = "materialize" / "emit" / "check" / "prove" / "concept" /
              "link" / tstr
```

Examples:

1. A concept citation has a matching `(target_language, target_library_tag,
   concept_name)` body route: `kind = "materialize"`, `state = "available"`.
2. The route exists but loss policy refuses it: `kind = "materialize"`,
   `state = "refused"`.
3. The kit can emit a pytest/JUnit/Go test artifact: `kind = "emit"`,
   `state = "available"`.
4. The verifier has a nonzero proof receipt for the site: `kind = "prove"`,
   `state = "passed"`.

## §8. Project State

The coordinator MAY merge kit analysis with daemon project state. When present,
the `project` field uses the same rank pins as linkerd.

```cddl
lsp-project-state = {
  ? contractSetCid: cid,
  ? callEdgeSetCid: cid,
  ? bridgeSetCid: cid,
  ? linkBundleCid: cid,
  ? diagnosticsCid: cid
}
```

The coordinator MUST NOT invent these CIDs. They come from linkerd, verifier
receipts, or other content-addressed producers.

## §9. Coordinator Rules

1. The coordinator SHALL route a document snapshot to the configured kit helper
   based on project configuration and file identity.
2. The coordinator MUST NOT parse host-language source.
3. The coordinator MUST NOT read kit shim `.proof` files or body-template files
   as runtime authority.
4. The coordinator MAY cache responses by `(kit_id, document_cid,
   protocol_catalog_cid, policy_cids)`.
5. The coordinator SHALL convert shared diagnostics to LSP diagnostics without
   changing stable diagnostic codes.
6. The coordinator MAY call linkerd `getDiagnostics` and `projectStatus`, and
   MAY call legacy `parseFile` adapters during migration. A `parseFile` adapter
   MUST invoke the owning kit helper or a lossless adapter to that helper; it
   MUST NOT make linkerd the owner of host-language source parsing.
7. The coordinator SHALL treat `provekit.lsp.vacuous_proof` as at least a
   warning.

## §10. Kit Helper Rules

1. A kit helper SHALL own source parsing and source-position mapping for its
   language.
2. A kit helper SHOULD wrap existing PEP 1.7.0 `kind = "lift"` behavior rather
   than fork a second source walker.
3. A kit helper SHALL emit normalized entries with source ranges for every site
   it can map.
4. A kit helper SHOULD return source-local diagnostics for parse errors, lift
   gaps, unsupported source surfaces, malformed concept citations, and carrier
   CID mismatches.
5. A kit helper MUST NOT report proof success for a vacuous zero-claim route.
6. A kit helper MAY omit entries it cannot lift, but SHOULD emit a
   `provekit.lsp.lift_gap` diagnostic for each omitted source site it can
   localize.

## §11. Linkerd and Verifier Integration

The shared LSP protocol does not replace the Linker Daemon Protocol. It defines
the editor-facing envelope that kit helpers and the coordinator use before and
after daemon calls.

The preferred live route is:

1. Kit helper analyzes the live document and emits normalized entries, ranges,
   diagnostics, and statuses.
2. Coordinator forwards normalized document facts, contract streams, and
   call-edge streams to `provekit-linkerd`.
3. Linkerd derives bridge/link diagnostics and project pins.
4. Coordinator merges kit diagnostics/statuses with linkerd/verifier
   diagnostics/statuses.
5. Editor receives LSP diagnostics, hovers, code lenses, and future inline hints.

If linkerd receives a live source snapshot through the older `parseFile`
surface, that surface is a legacy adapter only. Linkerd MUST invoke the owning
kit helper through `analyzeDocument` or a lossless adapter to the same
`lsp-document-analysis` shape; it MUST NOT grow host-language parsers. The older
`parse` and `lift` routes remain migration inputs, not the shared target.

## §12. Migration

This spec rebaselines the current LSP issue set:

1. #308 remains the forward-propagation epic, but forward propagation is a child
   diagnostic producer under this shared LSP protocol.
2. #664 remains the editor overlay issue, but its hover, inline hint, code lens,
   and diagnostic surfaces consume `entries`, `diagnostics`, `statuses`, and
   `project` from this spec.
3. #1500 through #1503 depend on this shared protocol before per-kit
   implementation tickets are sufficient.
4. Existing kit helpers using `parse`, legacy `provekit-lift/1` `lift`, or
   ad hoc `declarations`/`callEdges` results should gain an adapter to
   `analyzeDocument`.
5. The TypeScript verifier-shell LSP documentation should be marked historical
   or updated to route through this shared protocol.

## §13. Conformance

A kit helper conforms to this spec if:

1. `initialize` returns `protocol_version = "provekit-lsp-shared/1"`.
2. `analyzeDocument` is deterministic for byte-identical `(kit_id, file, text,
   accepted_protocol_catalog_cids, policy_cids)` inputs.
3. Returned source ranges point into the submitted document snapshot.
4. Returned `bind-lift-entry` and `library-sugar-binding-entry` payloads conform
   to their existing specs.
5. Malformed source, malformed concept carriers, and carrier CID mismatches
   produce diagnostics rather than silent omission.

An LSP coordinator conforms if:

1. It never parses host-language source.
2. It routes source snapshots to kit helpers and preserves their source ranges.
3. It converts shared diagnostics to LSP diagnostics with stable `provekit`
   source and stable codes.
4. It can merge kit and linkerd diagnostics for the same file.
5. It treats zero-claim proof success as a warning or error.

Linkerd conforms to this shared LSP surface if it can consume either
`lsp-document-analysis` directly or a byte-preserving adapter from it, and can
return diagnostics that the coordinator maps to the stable codes in §6.

## §14. First Slice

The first implementation slice SHOULD be:

1. Add a conformance fixture for one kit that exercises
   `initialize -> analyzeDocument -> lsp-document-analysis`.
2. The fixture contains one source site with a concept citation or contract
   witness, one source range, and one non-vacuous proof/materialize status.
3. The coordinator consumes the fixture result and publishes a stable LSP
   diagnostic or code lens.

Java or Python are the preferred first kits because their current parity issues
explicitly require rebaselining and their source lifters already carry useful
range and diagnostic data.
