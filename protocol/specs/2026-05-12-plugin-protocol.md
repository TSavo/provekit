# Universal Plugin Protocol (`provekit-plugin/1`)

**Status:** v1.0.0 normative draft. Listed in the protocol catalog under property key `plugin-protocol` (catalog entry to be appended in a follow-up CI mint; this spec MUST NOT edit `2026-04-30-protocol-catalog.json` directly). CID is computed from the bytes of this file (raw-bytes BLAKE3-512).
**Date:** 2026-05-12
**Author:** T Savo
**Related:**
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `2026-04-30-ir-formal-grammar.md` (IrFormula shape used by `sugar` plugins)
- `2026-04-30-protocol-versioning.md` (version token grammar)
- `2026-04-30-agent-plugin-protocol.md` (kind-specific predecessor; coexists, see §0.4)
- `2026-04-30-lift-plugin-protocol.md` (kind-specific predecessor; coexists, see §0.4)
- `2026-04-30-extension-protocols.md` and `2026-05-06-extension-protocols.md` (extension surfaces this protocol unifies)
- `2026-05-03-substrate-layers-envelope-header-body.md` (envelope/header/metadata layering reused)
- `2026-05-03-contract-cid-vs-attestation-cid.md` (CID semantics for the declared-behavior vs delivery split, §6)
- `2026-05-09-pattern-predicate-protocol.md` (precedent for content-addressed editorial extensions registered at runtime)
- `2026-05-12-plugin-protocol.md` consumers in this PR: `2026-05-12-sugar-dict-memento.md`, `2026-05-12-loss-function-memento.md`
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 (`loss-record` schema consumed by `loss-function` plugins)
- `2026-05-15-concept-hub-abstraction-layer.md` (loss-record dimensions also normative here)

## §0. Purpose

ProvekIt is a substrate, not a program. Every semantic decision the substrate makes (how a canonical clause is rendered into a target language, how loss is scored, which discharge backend handles which obligation, which lifter handles which file extension, which realizer fires for which clause shape) MUST be externalized as a content-addressed memento. The runtime binary is the engine; the plugins ARE the algebra (per `project_provekit_substrate_trinity` and the `algebra-is-the-portable-thing` thesis at `docs/papers/16-after-portability-the-universal-address-space.md`).

This spec defines the universal seam through which any such memento is loaded, addressed, version-negotiated, and registered. Three principles are non-negotiable:

1. **No hard coding.** A built-in default is still a memento at a fixed declared CID. The runtime MUST be able to enumerate, replace, and discharge every built-in along the same surface a third-party plugin uses.
2. **Trichotomy across plugin boundaries.** Plugin loading itself MUST emit `exact`, `loudly-bounded-lossy`, or `refuse` per `project_provekit_first_principle.md`. No silent fallback. A refusal to load a plugin is a precise extension request, not a hidden error.
3. **Federation by CID.** A plugin's identity is its declared-behavior CID. Delivery (JSON file, JSON-RPC server, statically-linked symbol) is a transport detail; CID stability is the federation guarantee (§6.2).

### §0.1 Relation to the substrate trinity

A plugin manipulates {terms, contracts, implications}. Sugar dicts render `contracts`; loss functions score divergences over `implications`; lifters mint `terms`; realizers discharge `contracts -> implications`. The plugin protocol is the registration mechanism for any of these algebraic operations, not just the four named in this PR.

### §0.2 What is NOT in scope for v1.0.0

- Discovery beyond CLI flags (no central registry; federation by reference is left to the consumer mementos themselves).
- Hot-reload of plugins mid-run (a `PluginRegistryMemento` is sealed at run start, §9).
- Sandboxing of plugin RPC processes (the host environment's process model is out of scope; plugins run with the runtime's privileges).
- Cross-runtime plugin portability (a Rust runtime and a TypeScript runtime MAY each implement this protocol; byte-identical CIDs across runtimes are required by §6.2, but the runtime binaries themselves are not portable).

### §0.3 Trichotomy mapping

| Outcome                    | Meaning                                                                                       |
|----------------------------|-----------------------------------------------------------------------------------------------|
| `exact`                    | Every requested plugin loaded; every declared CID validated; registry sealed without warning. |
| `loudly-bounded-lossy`     | One or more NON-CRITICAL plugins failed to load; each failure recorded as a `PluginLoadFailureMemento` (§8); registry sealed with the explicit failure-set in `PluginRegistryMemento.failures`. The run proceeds; downstream consumers MAY refuse to compose through the failure-set. |
| `refuse`                   | A plugin flagged `critical: true` failed to load, OR a duplicate-CID collision occurred at the same `(kind, cid)` slot (§9.2), OR protocol-version negotiation failed (§5). The runtime MUST exit non-zero with the failure list. |

### §0.4 Relation to the existing kind-specific plugin specs

The two existing plugin specs (`2026-04-30-agent-plugin-protocol.md`, `2026-04-30-lift-plugin-protocol.md`) remain authoritative for their kinds (`agent`, `lift`) at their declared protocol-version tokens (`provekit-agent/1`, `provekit-lift/1`). This spec defines a SUPERSET surface: any new plugin kind (`sugar`, `loss-function`, `discharge-backend`, `realizer`, `effect-signature`, `concept-extension`, and the open set §2.1 enumerates) MUST follow this protocol. Re-expression of the two predecessor specs as `provekit-plugin/1` memento kinds is an architect-call follow-up and is NOT taken by this PR. The two predecessors and this spec coexist; consumers wanting an `agent` or `lift` plugin continue to use their dedicated protocols.

## §1. The plugin memento

A plugin memento is a content-addressed record of a plugin's DECLARED BEHAVIOR. Delivery is separate (§3, §4). Two plugins with byte-identical content payloads MUST produce byte-identical CIDs even if one is a JSON file and the other is a JSON-RPC server (§6.2).

### §1.1 Wire shape (CDDL)

```cddl
; Shared scalar types:
;   hash, cid, signature, pubkey, iso8601, json-value
;
; Locked JCS key order: alphabetical within each object.
; The CDDL display order below is illustrative; JCS enforces alphabetical
; key order on the wire (per 2026-04-30-canonicalization-grammar.md).

; Open enum of plugin kinds. Validators MUST accept unknown kinds at
; shape level (§5.3 of the consumer specs decides whether to refuse).
; The canonical labels (v1.0.0) are listed in §2.1.
plugin-kind = tstr

; Protocol-version token; MUST conform to the grammar of
; 2026-04-30-protocol-versioning.md.
protocol-version = tstr

; Semver string; MUST conform to https://semver.org/spec/v2.0.0.html.
semver = tstr

; Locked JCS key order: cid, content, critical, kind, protocol_versions,
; provenance_cid, schemaVersion, version
plugin-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,            ; over JCS(header ++ metadata)
    signer:     pubkey
  },
  header: {
    cid:                 cid,         ; DERIVED -- see §6
    content:             json-value,  ; kind-specific payload; CDDL'd by the consumer spec
    critical:            bool,        ; if true, load failure refuses the whole run (§8)
    kind:                plugin-kind,
    protocol_versions:   [+ protocol-version],
    provenance_cid:      cid,         ; CID of the ProvenanceMemento for this plugin
    schemaVersion:       "1",
    version:             semver
  },
  metadata: {
    ? note: tstr,
    ? source_url: tstr,
    ? maintainer: tstr
  }
}
```

### §1.2 Field semantics

| Layer    | Field                | Required | Meaning |
|----------|----------------------|----------|---------|
| envelope | `declaredAt`         | yes      | ISO-8601 UTC minting timestamp. |
| envelope | `signature`          | yes (swarm) | Ed25519 over `JCS(header ++ metadata)`. |
| envelope | `signer`             | yes      | `ed25519:<base64>` minter public key. |
| header   | `cid`                | yes      | Content CID; DERIVED per §6.1. |
| header   | `content`            | yes      | The kind-specific payload. Its CDDL is defined by the consumer spec for `kind` (e.g., `2026-05-12-sugar-dict-memento.md` §2 for `kind = "sugar"`). Validators of this protocol MUST NOT validate the inner shape; consumer specs MUST. |
| header   | `critical`           | yes      | If `true`, a failure to load this plugin (§8) MUST refuse the run. If `false`, the failure is recorded as a `PluginLoadFailureMemento` and the run proceeds. Default if elided is `false`; producers MUST emit the field explicitly to keep CIDs byte-stable. |
| header   | `kind`               | yes      | One of the canonical labels (§2.1) or an open-extension label. Unknown labels MUST NOT be rejected at shape level. |
| header   | `protocol_versions`  | yes      | The protocol-version tokens this plugin SPEAKS. The runtime MUST refuse to load a plugin whose `protocol_versions` does not contain a token the runtime accepts (§5). |
| header   | `provenance_cid`     | yes      | CID of a `ProvenanceMemento` (per `2026-05-06-provenance-memento.md`) recording the build chain that produced this plugin's declared content. |
| header   | `schemaVersion`      | yes      | MUST be `"1"` for v1.0.0 of this protocol. |
| header   | `version`            | yes      | Semver of the plugin's own version line. Producers SHOULD bump on every content-payload change; consumers MUST compare CIDs, not semver, for identity. |
| metadata | `note`               | no       | Human-readable. OMITTED when absent. |
| metadata | `source_url`         | no       | Where the plugin's source lives. OMITTED when absent. |
| metadata | `maintainer`         | no       | Free-form maintainer string. OMITTED when absent. |

## §2. Plugin kinds

### §2.1 Canonical labels (v1.0.0)

Open enum. The following labels are reserved by v1.0.0 of this protocol; their consumer specs are minted separately. A `kind` not in this list is an extension label; validators MUST accept it at shape level (§5.3 of consumer specs decides further).

| `kind`                | Consumer spec                                                                 | What it carries                                                                                  |
|-----------------------|-------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `sugar`               | `2026-05-12-sugar-dict-memento.md`                                            | Canonical-clause-to-surface-syntax rendering rules; the first consumer of this protocol.         |
| `loss-function`       | `2026-05-12-loss-function-memento.md`                                         | Scoring algorithms over `loss-record` candidates; the second consumer of this protocol.          |
| `discharge-backend`   | DEFERRED to follow-up; precedent `2026-04-30-multi-solver-protocol.md`        | Z3 / cvc5 / Vampire / Maude / CeTA / others; one plugin per backend.                             |
| `lifter`              | DEFERRED; precedent `2026-04-30-lift-plugin-protocol.md`                      | Source-to-IR mint procedure per language or per surface (annotations, docstrings, tests).        |
| `realizer`            | DEFERRED; precedent `2026-05-06-obligation-realizer-protocol.md`              | Contract-to-discharge-obligation lowering rule.                                                  |
| `effect-signature`    | DEFERRED                                                                      | New effect-signature mints; consumed by the algebraic-effects layer.                             |
| `concept-extension`   | DEFERRED                                                                      | New `concept:*` hub op mints (consumed by `2026-05-15-concept-hub-abstraction-layer.md`).        |

### §2.2 Kind discipline

A `kind` label is part of the plugin memento CID (§6.1). The same content bytes registered under two different kinds produce two different CIDs. This is correct: a `sugar` plugin and a `loss-function` plugin are different things even if their `content` bytes coincide accidentally. The kind is semantic.

## §3. File interface

A plugin MAY be delivered as a JSON file containing the JCS-canonical bytes of the plugin memento's `header`. The runtime loads it as follows:

1. Read the file. Reject (refuse) on read error.
2. Parse JSON. Reject (refuse) on parse error.
3. Validate against the plugin-memento CDDL (§1.1). Validate the `content` payload against the consumer spec's CDDL for the declared `kind`.
4. Compute the CID per §6.1. If a `cid` is asserted in the parsed bytes and does not match, reject (refuse): cached-with-truth-source means the cache MUST equal truth.
5. Negotiate protocol-version per §5. Reject (refuse) on mismatch.
6. Register in the runtime registry under `(kind, cid)` per §9.

### §3.1 CLI flag form

The canonical CLI form is:

```
--plugin <kind>:<source>
```

where `<source>` is a filesystem path (absolute or relative to CWD). The runtime MUST distinguish file sources from RPC sources by inspecting `<source>`: a string beginning with `http://`, `https://`, or `tcp://` (or matching the JSON-RPC endpoint grammar of §4) is treated as RPC; otherwise it is treated as a file path.

Per-kind aliases SHOULD be provided by the runtime for ergonomic reasons:

```
--sugar <source>           ≡  --plugin sugar:<source>
--loss-function <source>   ≡  --plugin loss-function:<source>
--lifter <source>          ≡  --plugin lifter:<source>
```

Aliases MUST desugar to the canonical form before registry insertion; the canonical form is what appears in the `PluginRegistryMemento` (§9).

### §3.2 Multi-load and order

Repeated flags load multiple plugins of the same kind. CLI flag ORDER is preserved through to the registry's `load_order` array (§9.1) and is consulted by kind-specific tie-breaking (e.g., `2026-05-12-sugar-dict-memento.md` §4.4: later sugar dicts win ties). Order MUST be deterministic across runs given identical argv.

## §4. JSON-RPC interface

A plugin MAY be delivered as a JSON-RPC 2.0 server speaking over stdio or HTTP. The runtime connects, calls `describe` to obtain the plugin memento, validates and registers as in §3.

### §4.1 Endpoint grammar

```
<endpoint> = "stdio:" <argv-list>
           | "http://" <host> ":" <port> <path>
           | "https://" <host> ":" <port> <path>
           | "tcp://" <host> ":" <port>
```

The `stdio:<argv-list>` form spawns a subprocess (matching the LSP/MCP shape of the predecessor specs `2026-04-30-agent-plugin-protocol.md` and `2026-04-30-lift-plugin-protocol.md`).

### §4.2 Methods

#### §4.2.1 `provekit.plugin.describe`

The first call after connect. Returns the plugin's declared content memento.

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "provekit.plugin.describe",
  "params": {
    "runtime_protocol_versions": ["provekit-plugin/1"]
  }
}
```

Response (success):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "header": { /* the plugin memento's header object, JCS-canonical */ },
    "envelope": { /* the plugin memento's envelope object */ },
    "metadata": { /* the plugin memento's metadata object */ }
  }
}
```

The runtime MUST compute the CID over the returned `header` per §6.1 and compare to the asserted `header.cid`. Mismatch is a refuse.

#### §4.2.2 `provekit.plugin.invoke`

Kind-specific. The consumer spec for each kind defines the `params` and `result` shapes for `invoke`. The protocol-level guarantee is only the JSON-RPC 2.0 envelope.

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "provekit.plugin.invoke",
  "params": { /* kind-specific */ }
}
```

#### §4.2.3 `provekit.plugin.shutdown`

Graceful close. After this, an `stdio:` plugin SHOULD exit zero on stdin EOF; an HTTP plugin MAY ignore the call.

### §4.3 Error model on the wire

JSON-RPC errors per RFC 7065. The runtime treats any error response from `describe` as a failure to load (§8). The runtime treats any non-JSON output on stdout from an `stdio:` plugin as a refuse.

## §5. Version negotiation

### §5.1 Protocol versions the runtime accepts

The runtime declares a SET of protocol-version tokens it accepts. v1.0.0 of this spec defines exactly one token: `provekit-plugin/1`. Future minor versions of this protocol that are wire-compatible MAY add tokens to the set; future major versions mint a new spec at a new file path.

### §5.2 Negotiation procedure

A plugin's `header.protocol_versions` MUST contain at least one token the runtime accepts. The runtime selects the FIRST token (in plugin-declared order) that it accepts and uses that for the remainder of the session.

If no token matches, the runtime refuses to load the plugin. This MUST emit a `PluginLoadFailureMemento` with `reason_kind = "protocol-version-mismatch"` (§8.1). The plugin is NOT registered.

### §5.3 No silent fallback

The runtime MUST NOT attempt an "older protocol" or "best-effort" parse on version mismatch. Per Supra omnia, rectum, a version mismatch is a refuse, not a degraded load.

## §6. Content-addressing rules

### §6.1 CID construction

The `cid` field is `"blake3-512:" ++ hex(BLAKE3-512(<cid_input>))` where `<cid_input>` is the JCS-canonical bytes of the header with `cid` elided. Concretely:

```
cid_input = JCS({
  "content":           <content>,
  "critical":          <critical>,
  "kind":              <kind>,
  "protocol_versions": <protocol_versions sorted ascending>,
  "provenance_cid":    <provenance_cid>,
  "schemaVersion":     "1",
  "version":           <version>
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

JCS canonicalization per `2026-04-30-canonicalization-grammar.md`. The `protocol_versions` array is sorted ascending lexicographically before JCS; the runtime MUST sort, and producers MUST emit sorted arrays.

### §6.2 Delivery does not affect CID

The CID is determined by the content payload (`content`, `kind`, `critical`, `protocol_versions`, `provenance_cid`, `schemaVersion`, `version`). Two plugins with byte-identical headers MUST produce the same CID regardless of whether they are delivered as JSON files or as JSON-RPC servers. This is the federation guarantee.

**INVARIANT (CID-delivery-independence):** For any plugin P, `CID(P-as-file) == CID(P-as-rpc)` if and only if their JCS-canonical header bytes coincide.

### §6.3 Built-in plugins

A runtime MAY compile in default plugins. Such built-ins MUST be content-addressed by the same procedure (§6.1) and MUST appear in the `PluginRegistryMemento` at the same CID a user would compute from the equivalent JSON file. Built-ins are NOT a privileged class; they MUST be enumerable, replaceable, and dischargable along the same surface user-supplied plugins use. A consumer's "the default loss function" reference is to a known CID (see `2026-05-12-loss-function-memento.md` §6 for the canonical default loss function's declared content).

## §7. CLI flag conventions

| Flag form                              | Effect                                                                                                  |
|----------------------------------------|---------------------------------------------------------------------------------------------------------|
| `--plugin <kind>:<source>`             | Canonical form. Loads one plugin of declared kind from the source.                                      |
| `--<kind> <source>`                    | Per-kind alias. Desugars to the canonical form. See §3.1.                                               |
| `--no-default-plugins`                 | Suppresses ALL built-in plugin registration. The user MUST supply every plugin they wish loaded.        |
| `--no-default-plugin <kind>`           | Suppresses built-ins for one kind only.                                                                 |
| `--strict-plugins`                     | Promotes EVERY plugin load failure to a refuse (overrides individual `critical = false` declarations).  |
| `--plugin-registry-out <path>`         | After the registry seals (§9), writes the `PluginRegistryMemento` to `<path>`.                          |

Flag order is preserved through to `PluginRegistryMemento.load_order` (§9.1). Built-ins (when not suppressed) are appended AT THE END of the load order array; user flags precede built-ins. Rationale: a user-loaded plugin should beat a built-in in tie-breaks for the same kind, matching the §3.2 "later wins" rule applied to the {user flags} ++ {built-ins} concatenation.

## §8. Error model

### §8.1 `PluginLoadFailureMemento`

When a plugin fails to load (file not found, parse error, validation error, RPC timeout, version mismatch, signature invalid, CID mismatch), the runtime MUST mint a `PluginLoadFailureMemento`:

```cddl
; Locked JCS key order: cid, declared_source, failure_at, kind, plugin_kind,
; reason_detail, reason_kind, schemaVersion
plugin-load-failure-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,
    signer:     pubkey
  },
  header: {
    cid:              cid,            ; DERIVED -- see §8.3
    declared_source:  tstr,           ; the CLI flag value verbatim, e.g. "sugar:./my-sugar.json"
    failure_at:       iso8601,        ; when the load was attempted
    kind:             "plugin-load-failure",
    plugin_kind:      plugin-kind,    ; the declared kind from the CLI flag
    reason_detail:    tstr,           ; human-readable diagnostic
    reason_kind:      failure-reason-kind,
    schemaVersion:    "1"
  },
  metadata: { ? note: tstr }
}

failure-reason-kind = "file-not-found"
                    / "parse-error"
                    / "validation-error"
                    / "rpc-timeout"
                    / "rpc-error"
                    / "protocol-version-mismatch"
                    / "signature-invalid"
                    / "cid-mismatch"
                    / "duplicate-cid-collision"
                    / "critical-load-aborted"
                    / tstr
```

### §8.2 Trichotomy of plugin loading

| Run outcome                | Condition                                                                                     | Behavior                                                                                          |
|----------------------------|-----------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------|
| `exact`                    | Every requested plugin loaded; no `PluginLoadFailureMemento` minted.                          | Registry sealed (§9); run proceeds.                                                               |
| `loudly-bounded-lossy`     | One or more NON-CRITICAL plugins failed; ALL failures recorded as `PluginLoadFailureMemento`s. | Registry sealed with `failures` populated; run proceeds; downstream consumers MAY refuse.        |
| `refuse`                   | ANY critical plugin failed, OR `--strict-plugins` is set and any plugin failed, OR §9.2 collision. | Runtime MUST exit non-zero. The failures are emitted to stderr AND written to the registry-out path if specified. |

### §8.3 Failure-memento CID

```
cid_input = JCS({
  "declared_source": <declared_source>,
  "failure_at":      <failure_at>,
  "kind":            "plugin-load-failure",
  "plugin_kind":     <plugin_kind>,
  "reason_detail":   <reason_detail>,
  "reason_kind":     <reason_kind>,
  "schemaVersion":   "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

## §9. Registry semantics

### §9.1 `PluginRegistryMemento`

At run start, after all `--plugin` flags are processed and all built-ins are loaded (modulo §7 suppressions), the runtime SEALS the registry into a `PluginRegistryMemento`:

```cddl
; Locked JCS key order: built_in_count, cid, failures, kind, load_order,
; loaded, runtime_protocol_versions, schemaVersion, sealed_at
plugin-registry-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,
    signer:     pubkey
  },
  header: {
    built_in_count:            uint,                         ; how many entries in load_order are built-ins
    cid:                       cid,                          ; DERIVED -- see §9.3
    failures:                  [* cid],                      ; PluginLoadFailureMemento CIDs, in load-attempt order
    kind:                      "plugin-registry",
    load_order:                [* { kind: plugin-kind, cid: cid, source: tstr } ],
    loaded:                    [* { kind: plugin-kind, cid: cid } ],   ; sorted by cid ascending
    runtime_protocol_versions: [+ protocol-version],
    schemaVersion:             "1",
    sealed_at:                 iso8601
  },
  metadata: { ? note: tstr }
}
```

`load_order` preserves CLI order plus built-ins-at-end (§7); `loaded` is the same set sorted by CID (for content-addressing). Both fields are part of the CID; reordering CLI flags changes `load_order` bytes and rolls the registry CID, which rolls every downstream consumer that cited this registry. That is correct: a different load order is a different run.

### §9.2 Duplicate-CID collision rule

Two plugins of the same `(kind, cid)` slot is a no-op (the second registration is silently dropped; the first wins). Two plugins of the same `kind` with DIFFERENT CIDs are BOTH registered; tie-breaking among them is kind-specific (consumer specs define it).

Two DIFFERENT plugin mementos asserting the same `(kind, cid)` are impossible by construction (the CID is derived from the content). If observed, this is a hash collision or a tampered memento; the runtime MUST refuse with `reason_kind = "duplicate-cid-collision"` (§8.1) on the unlikely event of CID equality with non-identical bytes.

### §9.3 Registry CID

```
cid_input = JCS({
  "built_in_count":            <built_in_count>,
  "failures":                  <failures>,
  "kind":                      "plugin-registry",
  "load_order":                <load_order>,
  "loaded":                    <loaded sorted by cid ascending>,
  "runtime_protocol_versions": <runtime_protocol_versions sorted ascending>,
  "schemaVersion":             "1",
  "sealed_at":                 <sealed_at>
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

### §9.4 Provenance propagation

Any pipeline-output memento produced by a run MUST cite the `PluginRegistryMemento.cid` in its provenance. Concretely, every output memento's `provenance_cid` chain MUST resolve to a `ProvenanceMemento` whose `inputs` array contains the registry CID. This is the audit trail: a verifier can re-derive any output by replaying the exact plugin set the run used.

## §10. Federation

### §10.1 No central registry required

Plugins are content-addressed. A consumer references a plugin by CID (or by the kind plus a selection predicate the consumer spec defines). No directory service is required; a `PluginRegistryMemento` published alongside a `.proof` carries enough information for any verifier to fetch and re-load every plugin the run used (modulo plugin availability at the verifier's location, which is an out-of-protocol concern handled by the same content-addressed-storage mechanisms the rest of the substrate uses).

### §10.2 Plugin dependencies

A plugin's `content` payload MAY reference OTHER plugin CIDs (e.g., a sugar dict that depends on a particular lifter producing specific term-CIDs). The reference is content-addressed; the dependency graph is auditable. v1.0.0 of this protocol does NOT define an automatic dependency-resolution mechanism; consumer specs MAY require the runtime to refuse if a referenced dependency is not present in the registry.

### §10.3 Future discovery service

A federated discovery service (e.g., a content-addressed plugin index) is anticipated but is OUT OF SCOPE for v1.0.0. The CLI flag surface (§7) is the v1.0.0 interface. A future spec MAY define a discovery protocol on top of this base.

## §11. Cross-references

- The `cid` construction follows `2026-04-30-canonicalization-grammar.md`.
- The `provenance_cid` field MUST resolve to a `ProvenanceMemento` per `2026-05-06-provenance-memento.md`.
- The `protocol_versions` token grammar MUST conform to `2026-04-30-protocol-versioning.md`.
- The existing kind-specific plugin specs (`2026-04-30-agent-plugin-protocol.md`, `2026-04-30-lift-plugin-protocol.md`) remain authoritative for their kinds; this spec does NOT supersede them (§0.4).
- The first two consumer specs are minted in this PR:
  - `2026-05-12-sugar-dict-memento.md` (kind = `"sugar"`).
  - `2026-05-12-loss-function-memento.md` (kind = `"loss-function"`).
- The `loss-record` shape consumed by `loss-function` plugins is defined in `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 and elaborated in `2026-05-15-concept-hub-abstraction-layer.md` §2.4.
- The substrate trinity precedent: `project_provekit_substrate_trinity` (memo); the algebra-as-portable-thing thesis: `docs/papers/16-after-portability-the-universal-address-space.md`.

## §12. Out of scope for v1.0.0

- Re-expressing `provekit-agent/1` and `provekit-lift/1` as `provekit-plugin/1` kind mementos.
- Hot-reload of the registry mid-run.
- Cross-runtime portability beyond byte-identical CIDs.
- Sandboxing of RPC plugin processes.
- A federated discovery service.
- Consumer specs for `discharge-backend`, `lifter`, `realizer`, `effect-signature`, and `concept-extension` (deferred follow-ups).
- An edit to `2026-04-30-protocol-catalog.json` registering this spec's catalog entry (CI mint follow-up).
