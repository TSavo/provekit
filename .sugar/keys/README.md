# Sugar Foundation Keys

This directory holds the public half of the Sugar Foundation Root
Key. The private half is gitignored.

## Files

| File | Status | Purpose |
|---|---|---|
| `foundation-v0.pub` | committed | Ed25519 public key, in `ed25519:<base64>` form. Pinned trust anchor for v1.1.0 catalog signatures. |
| `foundation-v0.priv` | gitignored | Ed25519 seed (32 bytes, hex-encoded). Regenerable from a public seed (see below). |

## v0: deterministic test seed

v0 of the Foundation key is generated from the publicly-known seed
`[0x42; 32]`. This is intentional and documented:

- The seed is checked into the repository (`tools/foundation-keygen/src/lib.rs`,
  constant `FOUNDATION_V0_SEED`).
- Anyone running `cargo run --release --bin foundation-keygen` produces
  the identical keypair byte-for-byte.
- A signature under this key is structurally valid, but the trust
  anchor is "the bytes match the public seed in this repo." Anyone can
  forge a v0-signed catalog attestation. v0 is for proving the
  signature path works end-to-end, not for production trust.

The `foundation-v0.priv` file is gitignored not because the v0 seed is
secret (it is not), but to mirror the procedure v1 will require: an
HSM-generated seed must never be committed, and the .pub/.priv split
mechanic should be in place from day one.

## v0 -> v1 migration (planned)

v1 of the Foundation key will be:

- Generated offline on an HSM-backed device (YubiHSM / Ledger / equivalent).
- Distributed only as the public half (`foundation-v1.pub`).
- Used to sign a key-rotation attestation referencing both the v0 and
  v1 public keys. Verifiers that trusted v0 can bridge to v1 by
  validating the rotation attestation.

The rotation attestation file format and validation procedure are a
future spec. Until then, treat v0 signatures as proof-of-mechanism,
not proof-of-authority.

## Procedure

```
cargo run --release --bin foundation-keygen
cargo run --release --bin sign-catalog
```

Both binaries live under `tools/foundation-keygen/`. The first writes
this directory's `.priv` and `.pub`. The second writes
`.sugar/catalog-signatures/v1.1.0.json`.

## Verification

```
sugar verify-protocol --signed
```

The CLI embeds the public key from `foundation-v0.pub` and the signed
attestation from `.sugar/catalog-signatures/v1.1.0.json` at compile
time. `--signed` validates: (a) the catalog CID matches the embedded
expected CID, and (b) the signed attestation's signature verifies
against the embedded public key, with the message reconstructed from
the attestation's six non-signature fields, JCS-canonicalized.

Override paths via `--pubkey-file <path>` and `--signature-file <path>`.
