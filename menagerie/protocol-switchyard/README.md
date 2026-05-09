# Protocol Switchyard

Protocol Switchyard is the runnable Menagerie destination for protocol
evolution as content-addressed routing.

Its core claim is that protocol versions are roots, migrations are
witnessed edges, and compatibility is a checked route rather than a
promise in release notes. The runner cashes the claim out as concrete
blake3-512 CIDs.

## What runs end to end

The MVP exhibit is a synthetic HTTP profile migration. The runner does
the following on every invocation:

1. reads two prose obligation specs for the v1 profile and two for the
   v2 profile under `profiles/http/`;
2. computes a blake3-512 CID for each spec by hashing its raw bytes;
3. builds two HTTP profile catalog roots (v1 and v2), each naming the
   spec CIDs as property bindings;
4. shells through the existing `provekit` CLI to mint a witnessed
   migration edge between the two roots via `provekit protocol evolve`;
5. prints the four load-bearing CIDs: `fromCatalogCid`, `toCatalogCid`,
   `bodyCid` (the ProtocolEvolutionBodyClaim), and `witnessCid` (the
   TruthDischargeWitness over that body).

The first three CIDs are JCS-canonicalized blake3-512 over JSON
artifacts. The fourth is a JCS-canonicalized blake3-512 over the witness
body that names the migration body CID.

The runner does not re-implement signing or hashing primitives. It
shells through the `provekit` CLI for the migration edge, and it depends
on `provekit-canonicalizer` for the spec-byte hashes so they match the
CLI's catalog property recompute step.

## Two boundary obligations per profile

Each profile names two obligations matching paper 10 section 4:

- `request-smuggling-refusal`: the parser refuses messages with
  ambiguous framing between `Content-Length` and `Transfer-Encoding`;
- `content-length-transfer-encoding`: the parser determines body length
  from a single, ordered source.

The v1 obligation accepts some boundary cases. The v2 obligation closes
those cases unconditionally and adds three additional refusal reason
codes. The change in spec bytes flows through the catalog property CID
into the profile-root CID, which is exactly the substrate move paper 10
asks for.

## Aspirational scope (honest)

This MVP demonstrates the witnessed-edge shape. It does not yet:

- execute grammar conformance against the prose specs;
- mint an implementation conformance witness for any HTTP server;
- emit a paper-10-section-6 migration body with bridge checker
  witnesses, compatibility invariant CIDs, or refusal CIDs as separate
  artifacts.

Refusal receipts are the right shape for these. The MVP exists and
names what it cannot yet prove.

## Run it

```sh
cargo run --manifest-path menagerie/protocol-switchyard/Cargo.toml -- --all
```

The runner exits 0 and prints the four CIDs above plus the paths to the
catalog, policy, verifier, body, and witness JSON it produced under
`$TMPDIR`. Pass `--json` for a structured report.

## See also

- `docs/papers/10-after-protocol-specs-how-protocols-actually-evolve.md`,
  the paper this exhibit cashes out;
- `protocol/evolution/v1.6.3/`, the dogfood evolution edge for the
  ProvekIt protocol catalog itself;
- `implementations/rust/provekit-cli/src/cmd_protocol.rs`, the
  `provekit protocol evolve` and `provekit protocol check-evolution`
  commands the runner shells through.
