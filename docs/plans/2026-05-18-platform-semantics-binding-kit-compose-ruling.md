# Platform-Semantics Binding-Kit Compose API Ruling

Date: 2026-05-18
Status: Active. Implemented in libsugar/src/core/platform_semantics.rs via PR #1201.

## Ruling

The substrate exposes TWO dispatchers for platform-semantics declarations, plus a compose-API that merges their output:

1. `platform_semantics_for_lower_target(lang: &str) -> Option<PlatformSemanticsDeclaration>` - language-kit declarations (per-op platform semantics: arithmetic overflow, division rounding, shift mode, null semantics, bitwise semantics). Arms: python, rust, java, c, typescript.

2. `binding_semantics_for_tag(binding_tag: &str) -> Option<PlatformSemanticsDeclaration>` - binding-kit declarations (per-op library semantics: RowIdMechanism, etc.). Arms: better-sqlite3, pg.

3. `platform_semantics_for_binding(lang: &str, binding_tag: &str) -> Option<PlatformSemanticsDeclaration>` - composes (1) + (2) via `merge_declarations`. Returns None when neither layer has any declaration.

### Conflict resolution: binding-kit wins.

When both kits declare a tag for the same op-CID, the binding-kit's tag overrides the language-kit's tag. Library semantics are more specific than pure-language semantics.

```
merge_declarations(lang, binding) = {
    tags: union by op-CID, binding wins,
    dimension_values: union, dedup by CID,
    op_aliases: union, binding wins,
}
```

## Why this shape

The M+N transport hub depends on it. Per the transport architecture (`project_sugar_transport_architecture` memory): M cross-N translations are infeasible at scale; the substrate needs a hub. With composed declarations:

- Adding a new binding requires ONE binding-kit declaration, not M language-binding pairs.
- Adding a new language requires ONE language-kit declaration, not N language-binding pairs.
- Total declarations = M + N. Without composition, it would be M times N.

Conflict resolution (binding wins) is also load-bearing. SQLite-typescript's `RowIdMechanism = LastInsertRowid` is the binding's claim; the underlying TypeScript language has no opinion on row-id mechanisms. If language ever DID declare a default RowIdMechanism, the binding's specific knowledge would correctly override.

## Discipline

- Production migrate code MUST call `platform_semantics_for_binding(lang, tag)`, not `platform_semantics_for_lower_target(lang)`. The composed view is the one the trichotomy operates on.
- Binding-kit arms in `binding_semantics_for_tag` use the EXACT surface string from `split_library_surface` output (e.g., `better-sqlite3`, `pg`), not human-readable names. Validated empirically by codex/Agent B 2026-05-18.
- Language-kit and binding-kit declaration FILES live in separate modules. Inline composition at the dispatcher layer; no monolithic per-pair files.
- The merge function is the ONLY place that combines the two layers. Callers do not pre-merge or post-merge.

## Future work

- Audit `cmd_transport.rs` and lift-kit paths for `platform_semantics_for_lower_target` calls that should be `platform_semantics_for_binding` calls (tracked in #1207).
- Future binding-kits (mysql, sqlserver, redshift, etc.) follow the same pattern: one module, one arm in `binding_semantics_for_tag`.

## Cross-references

- PR #1155: substrate primitives
- PR #1201: compose API + production wiring
- PR #1204: refuse-leg fix consuming the compose output
- [[2026-05-18-op-coverage-verdict-trichotomy-ruling]] (the comparison primitive that operates on composed declarations)
- [[2026-05-18-dimension-naming-conventions]] (the dimensions binding-kits declare)
- `project_sugar_transport_architecture` (memory): M+N hub framing
