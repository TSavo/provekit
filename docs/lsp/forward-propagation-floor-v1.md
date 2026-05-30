# Forward-Propagation v1.0.0 Floor Spec

This document defines the scope for the LSP forward-propagator implementation across all kits.

## IN scope at v1.0.0 floor

- **Variable assignment posts**: `let x = expr` accumulates a post `typeof(x)` or the callee's post for the assigned callee
- **Sequential flow**: posts accumulate left-to-right in a basic block
- **If/else branch merge**: uses G3 disjunction (per #256) to merge the two branch posts
- **Function call posts**: when the callee has a known post in a verified baseline index, that post is added to the accumulated post
- **Callsite pre-check**: for callsites with a known callee pre, emit a verifier query `current_post implies callee_pre`; failed implication produces a diagnostic per [diagnostic-shape-v1.md](diagnostic-shape-v1.md)
- **`top` fallback**: any construct not in this list maps the propagator's post to `top` for the current scope

## OUT of scope (defer to v1.1+)

- **Loops**: post is `top` at loop entry
- **Closures**: post is `top` at capture site
- **Dynamic dispatch**: post is `top` when callee is not statically resolved
- **Complex aliasing**: no alias analysis; each variable post is tracked independently
- **Inter-procedural analysis**: beyond the single-function body
- **Exception propagation**: post at `?`/`throw`/`raise`/`panic` is `top`

## `top` semantics

When the propagator cannot track a post precisely, it uses `top` (the weakest possible post: no predicates). A callsite pre-check against `top` always fails implication. The propagator MUST NOT emit a false-positive diagnostic for `top`-fallback paths; it must suppress the diagnostic when the accumulated post is `top`.

## Cross-kit applicability

Every per-kit issue (#313-#324) must implement exactly this scope. Deviating from the IN/OUT lists requires a new floor spec version, not a per-kit exception.

## Diagnostic Shape

Diagnostics follow [diagnostic-shape-v1.md](diagnostic-shape-v1.md).

The diagnostic payload MUST carry the v1.6.2 identifiers that let a user or tool replay the decision:

- `protocol_catalog_cid`
- `baseline_catalog_cid`
- `baseline_index_cid`
- `callee_contract_cid`
- `callee_pre_cid`
- `current_post_cid`
- `signer` and `signer_role`

The plugin MUST suppress `provekit.lsp.implication_failed` when the accumulated post is `top`.

## Callsite Resolution

Callsite resolution follows [callsite-resolution-v1.md](callsite-resolution-v1.md).

The current model has two steps:

1. Resolve a source call expression to the kit's canonical callee identifier.
2. Look up that identifier in a verified, content-addressed baseline index.

The index is keyed by callee identifier, not by `file:line:character`. Source location is diagnostic metadata only.

The normative index identity is `baseline_index_cid`. The v1.6.2 LSP baseline index model does not define any filename-derived lookup surface.

## Issue Cross-Links

- [#308](https://github.com/TSavo/provekit/issues/308): parent forward-propagation epic.
- [#311](https://github.com/TSavo/provekit/issues/311): diagnostic shape.
- [#312](https://github.com/TSavo/provekit/issues/312): callsite resolution.
- [#313](https://github.com/TSavo/provekit/issues/313), [#314](https://github.com/TSavo/provekit/issues/314), and [#324](https://github.com/TSavo/provekit/issues/324): representative per-kit implementation issues that should reference these docs.
- [#478](https://github.com/TSavo/provekit/issues/478): v1.6.2 rebaseline for this doc family.
