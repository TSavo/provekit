# ed25519-vendored

Public-domain Ed25519 implementation vendored for the Sugar C kit.

Source: derived from the reference implementation in
  https://github.com/golang/crypto/tree/master/ed25519/internal/edwards25519
  (itself derived from SUPERCOP ref10, public domain / CC0).

Modifications for Sugar:
  - Stripped to the minimum needed: keypair from seed, sign, verify.
  - Bundled SHA-512 (FIPS 180-4, public domain from nacl/tweetnacl).
  - Single compilation unit (ed25519.c) with no external dependencies.
  - POSIX C11, no GNU extensions.

Byte-identical signatures to ed25519-dalek (Rust), OpenSSL EVP_PKEY_ED25519,
and node:crypto's ed25519 for the same (seed, message) pair.
