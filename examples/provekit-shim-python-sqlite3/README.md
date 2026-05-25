# provekit-shim-python-sqlite3

Substrate-honest concept bindings for the Python stdlib `sqlite3` module.

## What this is

A vendored boundary namespace under the ProvekIt proofchain (paper 24). Every
claim lives in `provekit_shim_python_sqlite3/__init__.py`. No sidecar files.

58 envelope members: 48 `library-sugar-binding-entry` + 10 `refusal-memento`.

## How to lift and mint

```sh
# From the provekit repo root, with the Rust toolchain installed:
pip install -e implementations/python/provekit-lift-python-source

cd examples/provekit-shim-python-sqlite3
cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- mint --out .

cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- proof inspect blake3-512:*.proof
```

`mint --out .` writes the CID-named `.proof`; the materialize `sugar_proof`
glob and `manifest_audit` read it as the source-of-truth for this kit's
binding-entry concepts. (The old central `python-canonical-bodies-sqlite3.json`
was deleted in #1463; the `.proof` is now the only body-source.)

## Concept coverage

See `provekit_shim_python_sqlite3/__init__.py` for all 48 bindings and 10 refusals.
Concept names match the rusqlite shim to enable cluster formation across kits.
