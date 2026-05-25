# provekit-shim-python-requests

Substrate-honest concept bindings for the Python `requests` HTTP library.

## What this is

A vendored boundary namespace under the ProvekIt proofchain (paper 24). Every
claim lives in `provekit_shim_python_requests/__init__.py`. No sidecar files.

The primary bindings match the catalog shapes:

- `concept:http-request`: `(HttpMethod, Url, HeaderMap, Optional<ByteStreamOrBytes>) -> HttpResponse`
- `concept:http-response`: `(HttpStatus, HeaderMap, ByteStreamOrBytes) -> HttpResponse`

The shim also keeps the legacy requests-kit status helpers as loudly-bounded
lossy bindings. Carrier surfaces that cannot preserve the catalog shape are
signed as refusals.

## How to lift and mint

```sh
# From the provekit repo root, with the Rust toolchain installed:
pip install -e implementations/python/provekit-lift-python-source

cd examples/provekit-shim-python-requests
cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- mint --out .

cargo run --manifest-path ../../implementations/rust/Cargo.toml \
    -p provekit-cli --bin provekit -- proof inspect blake3-512:*.proof
```

`mint --out .` writes the CID-named `.proof`; the requests realize kit resolves
that `.proof` from the installed `provekit-shim-python-requests` package and
extracts `library-sugar-binding-entry` members for `target_library_tag ==
"requests"`.

## Concept coverage

See `provekit_shim_python_requests/__init__.py` for the materialized,
loudly-bounded-lossy, and refused surfaces.
