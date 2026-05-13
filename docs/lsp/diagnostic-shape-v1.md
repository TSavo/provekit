# LSP Diagnostic Shape v1

This document defines the cross-kit diagnostic shape for the forward-propagation precondition check.

The check is:

```
current_post implies callee_pre
```

When the verifier rejects that implication, the LSP plugin emits one `implication-failed` diagnostic at the callsite.

## Scope

This shape is for precondition failures found by the thin forward-propagation loop in [forward-propagation-floor-v1.md](forward-propagation-floor-v1.md). It is not the shape for parse errors, missing CIDs, signature failures, protocol catalog mismatches, or unresolved baseline artifacts.

Those failures use the stable ProvekIt diagnostic codes in [../reference/error-codes.md](../reference/error-codes.md).

## Constants

Forward-propagation precondition failures always use:

| Field | Value |
|---|---|
| `severity` | `1` |
| `source` | `"provekit"` |
| `code` | `"implication-failed"` |

Severity `1` is the LSP Error severity.

## CID Model

The diagnostic MUST preserve the v1.6.2 CID separation:

| Field | Meaning |
|---|---|
| `protocol_catalog_cid` | The active protocol catalog CID. For the current tree this is v1.6.2. |
| `baseline_catalog_cid` | The CID of the verified `.proof` baseline catalog artifact used for lookup. This is the artifact CID, not a friendly filename. |
| `baseline_contract_set_cid` | The signer-independent content set CID for the baseline contracts, when present or derivable. |
| `baseline_index_cid` | The CID of the LSP callsite index artifact described in [callsite-resolution-v1.md](callsite-resolution-v1.md). |
| `callee_contract_cid` | The content-only CID of the callee `ContractDecl`. It is independent of signer state. |
| `callee_attestation_cid` | The signer-specific CID of the memento or member that attests to the callee contract. |
| `callee_pre_cid` | The CID of the canonical precondition formula used for this implication query. |
| `callee_post_cid` | The CID of the canonical postcondition formula, if the callee contract has one. |
| `current_post_cid` | The CID of the accumulated caller post at the callsite. |

Implementations MUST NOT put an attestation CID in a field named `contract_cid`. Bridges, lookup entries, and diagnostics use `contractCid` semantics for contract identity and `attestationCid` semantics only for signer-specific evidence.

## Diagnostic Payload

The LSP diagnostic object is:

```json
{
  "range": {
    "start": { "line": 41, "character": 12 },
    "end": { "line": 41, "character": 24 }
  },
  "severity": 1,
  "source": "provekit",
  "code": "implication-failed",
  "message": "callee precondition not established at this callsite",
  "data": {
    "schema_version": 1,
    "kind": "provekit.lsp.implication_failed",
    "callee": "std::option::Option::unwrap",
    "callee_contract_cid": "blake3-512:...",
    "callee_attestation_cid": "blake3-512:...",
    "callee_pre_cid": "blake3-512:...",
    "callee_post_cid": "blake3-512:...",
    "current_post_cid": "blake3-512:...",
    "missing_conjuncts": [
      "receiver is Some"
    ],
    "signer": "ed25519:...",
    "signer_role": "foundation-baseline",
    "baseline_catalog_cid": "blake3-512:...",
    "baseline_contract_set_cid": "blake3-512:...",
    "baseline_index_cid": "blake3-512:...",
    "protocol_catalog_cid": "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f"
  }
}
```

`range` is the source range of the call expression or callee token. Lines and characters are zero-indexed per LSP.

`callee` uses the per-language canonical identifier format from [callsite-resolution-v1.md](callsite-resolution-v1.md).

`missing_conjuncts` is a display list derived from the failed verifier explanation. It is not a trust root. The trust roots are the CIDs plus the verified signer data.

## Emission Rules

An LSP plugin emits this diagnostic only when all of the following hold:

1. The callee resolves to at least one trusted baseline entry.
2. The selected entry has a precondition formula.
3. The accumulated caller post is not `top`.
4. The verifier rejects `current_post implies callee_pre`.

If the accumulated post is `top`, the plugin suppresses this diagnostic. The floor spec treats `top` as a loss of precision, not as a user-visible contract violation.

If lookup, verification, or trust-policy evaluation fails before the implication query can run, the plugin emits a more specific ProvekIt diagnostic code instead of `implication-failed`.

## Hover Content

On hover over the diagnostic range, the editor SHOULD show:

- The callee's full precondition and postcondition in human-readable form.
- The signer identity and signer role actually used for this diagnostic.
- The baseline catalog CID and baseline index CID used for lookup.
- The current accumulated post at the callsite.
- A diff-style listing of which precondition conjuncts were not established.

The hover MAY shorten CIDs for display, but the full CIDs MUST remain available in diagnostic `data`.

## Quick Fixes

At v1.0.0 floor, an editor MAY offer one quick fix per missing conjunct:

| Field | Value |
|---|---|
| `kind` | `"quickfix"` |
| `title` | `"Add guard: <human-readable conjunct>"` |

The edit content is per-kit because guard syntax is host-language-specific. The quick fix MUST NOT change the diagnostic `data` shape.

## Cross-Kit Consistency

The JSON shape, constants, CID fields, and `missing_conjuncts` field are identical across kits.

The only per-kit variance is:

- The `callee` identifier format.
- The human-readable message text after the required core meaning.
- The quick-fix edit content.

## Issue Map

- [#308](https://github.com/TSavo/provekit/issues/308): parent epic.
- [#311](https://github.com/TSavo/provekit/issues/311): original diagnostic-shape lock ticket.
- [#478](https://github.com/TSavo/provekit/issues/478): v1.6.2 baseline and index rebaseline ticket.
- [#313](https://github.com/TSavo/provekit/issues/313), [#314](https://github.com/TSavo/provekit/issues/314), and [#324](https://github.com/TSavo/provekit/issues/324): representative per-kit forward propagators.
