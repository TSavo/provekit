# provekit-shim-python-aiosqlite

Substrate-honest concept bindings for the `aiosqlite` async SQLite driver.

## What this is

The ASYNC mirror of `provekit-shim-python-sqlite3`. aiosqlite is a thin async
wrapper over the stdlib `sqlite3` module: the same surface with `await`,
`async with`, and async cursor iteration. Every claim lives in
`provekit_shim_python_aiosqlite/__init__.py`. No sidecar files.

Concept names match the sqlite3 / rusqlite shims to enable cluster formation
across kits. The one divergence from a mechanical mirror: `sync-vs-async` is NOT
a loss dimension here (aiosqlite IS the async kit), so it is dropped from every
binding's `loss` list; the remaining genuinely-lost dimensions are kept.

The connection/cursor query bindings cite the result-cardinality concepts
(`concept:sql-query-row` / `-all` / `-iterate`, #1468), not a flat
`concept:sql-query`.

## How to lift and mint

```sh
# From the provekit repo root, with the Rust toolchain installed:
pip install -e implementations/python/provekit-lift-python-source

cd examples/provekit-shim-python-aiosqlite
cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- mint --out .

cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- proof inspect blake3-512:*.proof
```

`mint --out .` writes the CID-named `.proof`; the materialize `sugar_proof`
glob and `manifest_audit` read it as the source-of-truth for this kit's
binding-entry concepts. The old central `python-canonical-bodies-aiosqlite.json`
was deleted in #1468; the `.proof` is now the only body-source.
