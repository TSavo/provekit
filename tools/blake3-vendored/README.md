# BLAKE3 — vendored portable C sources

Upstream: https://github.com/BLAKE3-team/BLAKE3
Version: 1.8.5
License: Apache-2.0 (see `LICENSE_A2`)

This directory vendors the **portable** subset of the BLAKE3 reference C
implementation so the C++ peer self-contracts orchestrator builds on any
Linux / macOS host without requiring `libblake3-dev` / `brew install blake3`
to be installed.

Files vendored:
- `blake3.c`           — front-end API (`blake3_hasher_*`)
- `blake3.h`           — public header
- `blake3_dispatch.c`  — feature dispatcher; falls back to portable when
                         AVX2/AVX512/SSE2/SSE41/NEON are disabled
- `blake3_impl.h`      — internal definitions
- `blake3_portable.c`  — pure-C compression function (no SIMD)

The build script `tools/build-cpp-self-contracts.sh` compiles these with
`-DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41
-DBLAKE3_USE_NEON=0` so the dispatcher selects the portable path
unconditionally. Performance is irrelevant for the tens of contracts the
orchestrator hashes.

If a future change needs SIMD performance, drop in the matching
`blake3_*_unix.S` files for the target arch and lift the `-D` flags
selectively. Until then: portable everywhere, zero system deps.

To refresh from upstream:
```sh
curl -sL https://github.com/BLAKE3-team/BLAKE3/archive/refs/tags/<VERSION>.tar.gz \
    | tar xz -C /tmp
cp /tmp/BLAKE3-<VERSION>/c/{blake3.c,blake3.h,blake3_dispatch.c,blake3_impl.h,blake3_portable.c} \
   tools/blake3-vendored/
cp /tmp/BLAKE3-<VERSION>/LICENSE_A2 tools/blake3-vendored/
```
