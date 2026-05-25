# provekit-shim-pg

ProvekIt boundary namespace for the TypeScript `pg` (node-postgres) driver.

The authored source is ordinary TypeScript decorated with `@sugar.bind(...)`.
The TypeScript lifter reads `src/index.ts` with `layer = "library-bindings"`,
emits `library-sugar-binding-entry` records, and the shared Rust mint path writes
the package-root `provekit.proof` envelope.

This is the async sister of `provekit-shim-better-sqlite3` and the TypeScript
analogue of the Rust `provekit-shim-postgres`: same `concept:sql-*` hub, the
node-`pg` engine, and (post #1468) result-cardinality query concepts:
`client.query` returning `result.rows` is `concept:sql-query-all`; returning
`result.rows[0]` is `concept:sql-query-row`. node-`pg` base has no lazy cursor,
so `concept:sql-query-iterate` is refused (mirrors the Rust postgres shim).

## Resolution model

Realize kits install this package from their package manager (npm/pnpm/yarn).
The `provekit.proof` file ships as part of the package and is resolved by the kit
at runtime from `node_modules/provekit-shim-pg/provekit.proof`. There is no
central JSON registry — the kit reads emission templates directly from the shim
proof.

## Re-minting

```bash
provekit lift examples/provekit-shim-typescript-pg --library-bindings
provekit mint --project examples/provekit-shim-typescript-pg --library-bindings --out examples/provekit-shim-typescript-pg
```

After re-minting, publish to your local registry and run `npm install` in consuming
realize kits to pick up the updated proof.
