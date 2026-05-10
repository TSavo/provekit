# provekit-mint-amp

`provekit-mint-amp` mints AMP and LSP algebraic mementos into a content-addressed catalog. It accepts small JSON specs for algorithm, binding, sort, equation, effect signature, language signature, and language morphism claims, resolves nested spec references to CIDs, canonicalizes with the workspace JCS and BLAKE3-512 helpers, signs the canonical payload bytes with Ed25519, writes the signed envelope, and updates `index.json`.

## CLI Examples

Mint a `SortMemento` with the unsigned development signer into an explicit test catalog:

```sh
provekit mint sort \
  --spec implementations/rust/provekit-mint-amp/tests/fixtures/sort_c_int.spec.json \
  --unsigned \
  --catalog /tmp/provekit-minter-test-catalog
```

Mint an `EquationMemento` that references the sort spec. The minter resolves the sort spec path recursively and stores the sort CID in `formal_sorts`.

```sh
provekit mint equation \
  --spec implementations/rust/provekit-mint-amp/tests/fixtures/equation_c_branch_identity.spec.json \
  --unsigned \
  --catalog /tmp/provekit-minter-test-catalog
```

Mint a `LanguageSignatureMemento` that references sorts, operations, equations, and effect signatures. Any referenced spec paths are minted first.

```sh
provekit mint language-signature \
  --spec implementations/rust/provekit-mint-amp/tests/fixtures/language_signature_c_c11.spec.json \
  --unsigned \
  --catalog /tmp/provekit-minter-test-catalog
```

Production catalog minting should use `--signer PATH`, where `PATH` contains an Ed25519 private key in PEM, raw hex, or base64 form. The unsigned signer is restricted to catalog roots whose path explicitly marks them as test or dev.

## Specs

The wire shapes follow the draft AMP and LSP specs:

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`

CCP composition is not needed for these minting paths. If a future minter needs composition, it should use `libprovekit::compose` rather than reimplementing the CCP algebra.
