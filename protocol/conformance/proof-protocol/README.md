# Proof Protocol Corpus

This directory is the first bootstrap corpus for `.proof` itself.

`proof-protocol.cid.txt` is a local locator for the current corpus artifact,
not protocol evidence and not a stable-pointer protocol. The artifact filename
is CID-shaped, not `proof-protocol.proof`, because the `.proof` trust root is
still the filename CID. Any load-bearing claim that a consumer should follow a
named protocol artifact must be carried as a signed, content-addressed
memento/attestation under policy.

Regenerate:

```sh
cargo run -p provekit-cli --manifest-path implementations/rust/Cargo.toml -- \
  proof mint-protocol --out-dir protocol/conformance/proof-protocol --json
```

Current corpus:

- `valid-basic-proof`: expected `true`
- `invalid-filename-cid`: expected `false`

The protocol `.proof` carries the fixture manifest in signed metadata under
`provekit.proofProtocol.fixtures.v0`. Core verification treats metadata as
signed bytes, not normative execution logic. A current proof-verification gate
interprets this manifest under policy.
