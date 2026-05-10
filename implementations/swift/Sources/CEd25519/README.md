# CEd25519: vendored Ed25519 thin wrapper for the Swift kit

**Canonical source:** `tools/ed25519-vendored/`. This directory is a
**copy**, NOT a symlink, because SwiftPM does not discover targets
through directory symlinks (it requires the source files to live inside
the package root tree, accessed without symlink traversal). The C kit
references `tools/ed25519-vendored/` directly via its Makefile
(`ED_DIR = ../../../tools/ed25519-vendored`); only this Swift target
needs the in-tree copy.

The two copies MUST stay byte-identical. The header layout differs:
SwiftPM expects public headers under `include/` (so `ed25519.h` lives
at `Sources/CEd25519/include/ed25519.h`), while the canonical layout
has it alongside the `.c` file. Everything else is verbatim.

## Backing library

Both copies are thin wrappers around OpenSSL's `EVP_PKEY_ED25519`. The
build links against `libcrypto`. See the canonical README at
`tools/ed25519-vendored/README.md` for the rationale (byte-identical
signatures with rust's ed25519-dalek and go's crypto/ed25519, RFC-8032
deterministic).

## Refresh procedure

When the canonical `tools/ed25519-vendored/` is updated, mirror the
change here:

```sh
cp tools/ed25519-vendored/ed25519.c \
   implementations/swift/Sources/CEd25519/

cp tools/ed25519-vendored/ed25519.h \
   implementations/swift/Sources/CEd25519/include/
```

## Verification

`diff -r tools/ed25519-vendored/ implementations/swift/Sources/CEd25519/`
should report only:
- `Only in tools/ed25519-vendored: README.md` (canonical README, not needed in copy)
- `Only in tools/ed25519-vendored: ed25519.h` (lives at `include/ed25519.h` in the copy)
- `Only in implementations/swift/Sources/CEd25519: include`
- `Only in implementations/swift/Sources/CEd25519: README.md` (this file)
