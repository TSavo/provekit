# Foundation baseline catalogs

Per-language advisory catalogs of hidden predicates about each language's
standard library. These are signed by the ProvekIt foundation v0
ed25519 key as a starting point for users who want to verify proofs
about code in the named language.

**They are NOT authoritative.** The authoritative signer for any
language's contracts is the language steward (e.g., the rust-lang team
for Rust, the Go core team for Go, Python core developers for Python).
If a steward has signed their own catalog, prefer it. If they have not,
fork the foundation baseline and sign your own — see
[`docs/contributing/signing-your-own-catalog.md`](../contributing/signing-your-own-catalog.md).

## How baselines are structured

Each baseline is a `.proof` envelope containing:

- A signed memento per ContractDecl (one entry per `<builtin>__<predicate>`).
- A `kind=disclaimer` memento carrying the verbatim disclaimer text.
- Envelope metadata at the catalog level identifying signer role, language,
  language version, kit version, and the disclaimer CID.

The compliance criteria a baseline must meet to ship at v1.0.0 are
specified in [`docs/contributing/baseline-catalog-rubric.md`](../contributing/baseline-catalog-rubric.md):

- Builtin count >= 50 (>= 100 for PHP)
- Each builtin has >= 2 ContractDecls (type signature + determinism floor)
- Envelope metadata carries the rubric §3 fields
- Disclaimer matches the rubric §4 base text verbatim, with a per-language
  addendum naming the steward.

## Catalog index

| Language | Catalog | Status | Authored against |
|----------|---------|--------|------------------|
| rust     | [`.provekit/baselines/rust-std-baseline-v1.proof`](../../.provekit/baselines/rust-std-baseline-v1.proof) | v1 (foundation-baseline, advisory) | rustc 1.81.0 |
| go       | _pending #258_ | not yet minted | — |
| cpp      | _pending #259_ | not yet minted | — |
| ts       | _pending #260_ | not yet minted | — |
| csharp   | _pending #261_ | not yet minted | — |
| swift    | _pending #262_ | not yet minted | — |
| java     | _pending #263_ | not yet minted | — |
| python   | _pending #264_ | not yet minted | — |
| ruby     | _pending #265_ | not yet minted | — |
| zig      | _pending #266_ | not yet minted | — |
| c        | _pending #267_ | not yet minted | — |
| php      | _pending #268_ | not yet minted | — |

Per-language addendum: see the language-specific markdown next to this
file (e.g. [`rust.md`](rust.md)).

## Federation

The federated index of known signers lives at
`protocol/federation/known-signers.toml` (when published). Stewards who
sign their own catalogs add an entry there pointing at their pubkey;
consumers pin signers from the index, from elsewhere, or directly by
hardcoded pubkey. The foundation does not curate the index — listing
means "self-declared standing," not "foundation endorsement."

## See also

- [`docs/contributing/baseline-catalog-rubric.md`](../contributing/baseline-catalog-rubric.md) (#254)
- [`docs/contributing/signing-your-own-catalog.md`](../contributing/signing-your-own-catalog.md) (#255)
- [`docs/contributing/dsl-extension-survey.md`](../contributing/dsl-extension-survey.md) (#256)
- Issue #257 (rust pilot, this document's first entry)
- Issues #258–#268 (per-language follow-ups)
