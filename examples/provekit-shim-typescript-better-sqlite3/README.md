# @provekit-shim/typescript-better-sqlite3

Vendored ProvekIt boundary namespace for the TypeScript `better-sqlite3` driver.

The authored source is ordinary TypeScript decorated with `@sugar.bind(...)`.
The TypeScript lifter reads `src/index.ts` with `layer = "library-bindings"`,
emits `library-sugar-binding-entry` records, and the shared Rust mint path writes
the package-root `provekit.proof` envelope.

```bash
provekit lift --project examples/provekit-shim-typescript-better-sqlite3 --library-bindings
provekit mint --project examples/provekit-shim-typescript-better-sqlite3 --library-bindings --out examples/provekit-shim-typescript-better-sqlite3
```
