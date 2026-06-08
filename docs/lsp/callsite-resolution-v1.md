# Callsite Resolution v1

This document defines how an LSP forward propagator resolves a source callsite to a callee precondition under the current v1.6.2 content-addressed baseline model.

## Decision

The normative lookup surface is a content-addressed LSP callsite index artifact.

The identity is `baseline_index_cid`, computed from the index artifact bytes. Consumers verify the index against the baseline catalog before using it. The model does not define any filename-derived lookup surface.

## Resolution Pipeline

The LSP plugin performs two separate steps:

1. Resolve the source call expression to a per-language canonical callee identifier.
2. Look up that identifier in the verified baseline index to find candidate contracts.

The index is keyed by canonical callee identifier, not by `file:line:character`. Source location remains diagnostic metadata, but it is not the baseline lookup key.

## Index Artifact Shape

The index artifact is JSON using JCS-compatible objects and arrays. Its CID is:

```
baseline_index_cid = "blake3-512:" || hex(BLAKE3-512(JCS(index_artifact)))
```

Required shape:

```json
{
  "schema_version": 1,
  "kind": "sugar.lsp.callsite_index",
  "language": "rust",
  "protocol_catalog_cid": "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f",
  "baseline_catalog_cid": "blake3-512:...",
  "baseline_contract_set_cid": "blake3-512:...",
  "entries": {
    "std::string::String::len": [
      {
        "contract_name": "rust_std_string_len_returns_usize",
        "member_cid": "blake3-512:...",
        "contract_cid": "blake3-512:...",
        "attestation_cid": "blake3-512:...",
        "pre_cid": "blake3-512:...",
        "post_cid": "blake3-512:...",
        "signer": "ed25519:...",
        "signer_role": "foundation-baseline",
        "declared_at": "2026-05-03T18:00:00Z"
      }
    ]
  }
}
```

`member_cid` identifies the member bytes inside the baseline catalog. `contract_cid` identifies the contract content. `attestation_cid` identifies the signer-specific attestation. These fields are intentionally distinct.

`pre_cid` and `post_cid` identify canonical formulas. If a contract has no precondition or no postcondition, the corresponding field is omitted.

## Producing the Index

An index producer:

1. Verifies the baseline `.proof` artifact CID.
2. Verifies every member CID listed by the baseline catalog.
3. Decodes contract members.
4. Recomputes each `contract_cid` from the `ContractDecl` content.
5. Extracts each member's canonical callee identifier, contract name, optional precondition, optional postcondition, signer, signer role, and declared timestamp.
6. Groups entries by canonical callee identifier.
7. Emits the index artifact and records its `baseline_index_cid`.

The index can be published as a standalone content-addressed artifact, embedded as signed metadata, or materialized as a local cache. All three forms are equivalent only after the consumer recomputes and verifies the same `baseline_index_cid`.

## Verifying the Index

An LSP consumer MUST verify an index before lookup:

1. Recompute `baseline_index_cid` from the index artifact bytes.
2. Check `protocol_catalog_cid` equals the active protocol catalog.
3. Check `baseline_catalog_cid` equals the verified baseline catalog artifact.
4. Recompute the index from the baseline catalog members and compare it to the supplied artifact.
5. Reject entries whose `contract_cid`, formula CIDs, signer, or signer role do not match the verified baseline member.

If any step fails, the supplied index is discarded. The plugin MAY rebuild a local index from the verified baseline catalog instead of failing the whole LSP session.

## Multi-Signer Resolution

When several trusted catalogs provide contracts for the same callee identifier, the resolver applies policy before order:

1. Keep only entries whose signer is trusted for the callee scope.
2. Prefer an explicit workspace signer pin over role defaults.
3. Prefer `language-steward`, then `foundation-baseline`, then `community` when policy permits role fallback.
4. If multiple entries still tie, choose the newest `declared_at`.
5. If timestamps tie, choose lexicographically by `attestation_cid` for deterministic behavior.

The selected entry's `signer` and `signer_role` are copied into the diagnostic payload. The role is never inferred from the filename.

## Per-Language Canonical Identifiers

| Kit | Format | Example |
|---|---|---|
| Rust | Rust full path | `std::vec::Vec::push` |
| Go | Package symbol | `fmt.Println`, `strings.HasPrefix` |
| TypeScript | ECMAScript receiver path | `Array.prototype.push`, `String.prototype.startsWith` |
| Java | Fully qualified member | `java.util.ArrayList.add` |
| C# | Fully qualified member | `System.Collections.Generic.List.Add` |
| Python | Module or builtin path | `builtins.len`, `os.path.join` |
| Ruby | Class member syntax | `String#length`, `Array#push` |
| PHP | Leading-backslash global function | `\\strlen`, `\\preg_match` |
| C | Link symbol | `strlen`, `memcpy` |
| C++ | Qualified symbol | `std::vector::push_back` |
| Swift | Type member | `String.count`, `Array.append` |
| Zig | Module path | `std.mem.copy` |

If a host language parser cannot resolve a dynamic call to one of these forms, the forward propagator uses `top` for that path and suppresses `sugar.lsp.implication_failed`.

## Performance

The hot lookup path MUST complete within 5ms for 100 callsites on a warm LSP session. Verification and index construction may happen during initialization or cache refresh; they are not part of the hot lookup budget.

The in-memory cache key is:

```
(protocol_catalog_cid, baseline_catalog_cid, baseline_index_cid)
```

Changing any member of that tuple invalidates the cache.

## Issue Map

- [#308](https://github.com/TSavo/sugar/issues/308): parent epic.
- [#312](https://github.com/TSavo/sugar/issues/312): original callsite-resolution ticket.
- [#478](https://github.com/TSavo/sugar/issues/478): v1.6.2 rebaseline ticket.
- [#313](https://github.com/TSavo/sugar/issues/313), [#314](https://github.com/TSavo/sugar/issues/314), and [#324](https://github.com/TSavo/sugar/issues/324): representative per-kit forward propagators.
