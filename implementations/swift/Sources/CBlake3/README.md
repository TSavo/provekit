# CBlake3 — vendored BLAKE3 portable C source for the Swift kit

**Canonical source:** `tools/blake3-vendored/`. This directory is a
**copy**, NOT a symlink, because SwiftPM does not discover targets
through directory symlinks (it requires the source files to live inside
the package root tree, accessed without symlink traversal). All other
ProvekIt kits reference `tools/blake3-vendored/` directly via Make /
CMake / build scripts; only this Swift target needs the in-tree copy.

The two copies MUST stay byte-identical. The header layout differs
slightly: SwiftPM expects public headers under `include/` (so
`blake3.h` lives at `Sources/CBlake3/include/blake3.h`), while the
canonical layout has it alongside the `.c` files. Everything else is
verbatim.

## Refresh procedure

When the canonical `tools/blake3-vendored/` is refreshed from upstream
(per `tools/blake3-vendored/README.md`), mirror the change here:

```sh
cp tools/blake3-vendored/blake3.c \
   tools/blake3-vendored/blake3_dispatch.c \
   tools/blake3-vendored/blake3_impl.h \
   tools/blake3-vendored/blake3_portable.c \
   tools/blake3-vendored/LICENSE_A2 \
   implementations/swift/Sources/CBlake3/

cp tools/blake3-vendored/blake3.h \
   implementations/swift/Sources/CBlake3/include/
```

## Verification

`diff -r tools/blake3-vendored/ implementations/swift/Sources/CBlake3/`
should report only:
- `Only in tools/blake3-vendored: README.md` (canonical README, not needed in copy)
- `Only in tools/blake3-vendored: blake3.h` (lives at `include/blake3.h` in the copy)
- `Only in implementations/swift/Sources/CBlake3: include`
- `Only in implementations/swift/Sources/CBlake3: README.md` (this file)
