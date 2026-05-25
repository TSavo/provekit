# provekit-shim-better-sqlite3

ProvekIt boundary namespace for the TypeScript `better-sqlite3` driver.

The authored source is ordinary TypeScript decorated with `@sugar.bind(...)`.
The TypeScript lifter reads `src/index.ts` with `layer = "library-bindings"`,
emits `library-sugar-binding-entry` records, and the shared Rust mint path writes
the package-root `provekit.proof` envelope.

## Resolution model

Realize kits install this package from their package manager (npm/pnpm/yarn).
The `provekit.proof` file ships as part of the package and is resolved by the kit
at runtime from `node_modules/provekit-shim-better-sqlite3/provekit.proof`.
There is no central JSON registry — the kit reads emission templates directly
from the shim proof.

## Re-minting

```bash
provekit lift --project examples/provekit-shim-typescript-better-sqlite3 --library-bindings
provekit mint --project examples/provekit-shim-typescript-better-sqlite3 --library-bindings --out examples/provekit-shim-typescript-better-sqlite3
```

After re-minting, publish to your local registry and run `npm install` in consuming
realize kits to pick up the updated proof.
