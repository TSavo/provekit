# Signing your own catalog

ProvekIt's protocol is content-addressable and signature-agnostic. Anyone with standing to make claims about a piece of code can sign a catalog of those claims. The substrate verifies signatures against their own embedded signer (per #248); trust is callsite policy, not protocol primitive.

This doc walks through the signing ceremony for a third party who wants to sign their own catalog of contracts about a language, library, or codebase. The foundation baselines are the worked example; this is the path for everyone else.

## Why this exists

The foundation key signs ProvekIt's own protocol. It does not have standing to sign authoritative contracts about TypeScript, Rust, Python, or PHP — those are the language stewards' to sign. Foundation baselines (per the [baseline catalog rubric](baseline-catalog-rubric.md)) are explicitly advisory: a starting point until stewards step up.

The protocol's value is creating room for actual stewards to sign. This doc is that room.

## What "standing" means

You have standing to sign a catalog about a thing when you wrote the thing, maintain the thing, or otherwise have authoritative knowledge about its semantics. Examples:

- The rust-lang team has standing to sign claims about `std::*`.
- A library maintainer has standing to sign claims about their library's public API.
- A consultant who has audited a closed-source binary has standing to sign claims about its observed behavior, **as advisory** (their signer_role is `community`, not `language-steward`).
- The foundation has standing to sign claims about ProvekIt's own protocol. It does NOT have standing to sign authoritative contracts about other languages — only foundation-baseline advisory.

The protocol does not enforce standing. It records who signed. Consumers decide whose signatures they trust by pinning specific signer keys.

## The substrate is ready

After #248, `verify_proof(bytes, expected_cid) -> { ok, signer }` answers one question: does this envelope's signature verify against its own embedded signer, and does the body hash to expected_cid? Trust policy ("is this signer authorized for this catalog") is callsite code.

This means signing your own catalog requires no protocol changes. You generate a key, mint a catalog, sign it, publish it. Verifiers using your catalog pin your key in their callsite policy.

## The ceremony

### 1. Generate a signing keypair

Ed25519 only. The seed is 32 bytes; derived public key is 32 bytes; signature is 64 bytes. Any tool that follows RFC 8032 produces compatible bytes.

```sh
# OpenSSL
openssl genpkey -algorithm Ed25519 -out signer.key
openssl pkey -in signer.key -pubout -out signer.pub

# Or via the kit's own helpers (rust example)
cargo run --bin foundation-keygen -- --out signer.key
```

The public key in self-identifying form is `ed25519:<base64-stdpad-of-32-bytes>`. That's what goes in the envelope's `signer` field. The seed never leaves your control.

### 2. Author the catalog

Mirror the per-kit Side A pattern (see `implementations/<kit>/mint-<kit>-self-contracts/`). Author a slab of `ContractDecl` entries describing the contracts you want to sign. Each ContractDecl has a name, optional pre/post/inv formulas, and an out-binding.

The formula DSL is shared across kits. See [adapter-coverage-rubric.md](adapter-coverage-rubric.md) for what predicates are expressible today; predicate gaps are tracked at #256.

The output is a list of canonical contract bytes (each ContractDecl JCS-encoded). The catalog's `members` map keys each by its BLAKE3-512 CID.

### 3. Mint the proof envelope

Use any kit's `pksc_proof_build` (or its kit-equivalent) to assemble:

- `members`: the contract CIDs + bytes
- `metadata`: the [advisory metadata block](baseline-catalog-rubric.md#3-advisory-metadata-shape) — `signer_role`, `baseline.version`, `baseline.language`, etc. (Use whichever role applies to you: `language-steward`, `community`, etc.)
- `signer_cid`: your public key in `ed25519:...` form
- `declared_at`: ISO-8601 timestamp
- `signer_seed`: your 32-byte private seed (never persisted)

The output is a CBOR-encoded `.proof` envelope whose filename CID is BLAKE3-512 of the bytes.

### 4. Verify locally before publishing

```sh
provekit verify --catalog mycatalog-v1.proof --expected-cid blake3-512:...
```

Confirms:
- BLAKE3-512(bytes) matches the filename CID
- Signature verifies against the embedded signer
- Each member's CID matches BLAKE3-512(member-bytes)

If `ok: true, signer: <your-pubkey>`, the envelope is internally consistent. Trust is the consumer's call.

### 5. Publish

A signed catalog is just bytes. Publish wherever bytes live:

- Git repository (commit alongside source)
- IPFS / content-addressable storage
- Static HTTP host (S3, GitHub Pages, your own server)
- Package registry attachment (npm, crates.io, PyPI as a sidecar artifact)

The catalog's filename CID is its identity. Consumers pin by CID, fetch from anywhere they can reach the bytes.

### 6. Register in the federated index (optional but recommended)

The federated index (see "Federated index format" below) is a static catalog the foundation maintains listing known signers per language / library / project. To get listed, open a PR against `protocol/federation/known-signers.toml` adding your pubkey + scope. The foundation does not vouch for signers in the index — listing means "this signer claims standing for this scope," not "the foundation endorses the signer's claims."

Consumers can pin signers from the index, from elsewhere, or from no index at all. The index is a discoverability convenience, not a gate.

## Federated index format

Stored at `protocol/federation/known-signers.toml`, signed as a contract memento with the foundation key. Every entry:

```toml
[[signer]]
pubkey = "ed25519:<base64>"
role = "language-steward"  # or "community" or "library-maintainer"
scopes = ["language:rust", "library:tokio", "project:foo/bar"]
display_name = "rust-lang team"
contact_url = "https://github.com/rust-lang"
```

Scope vocabulary:

- `language:<name>` — claims standing for a language's stdlib (rust, go, php, etc.)
- `library:<package>` — claims standing for a specific library's public API
- `project:<owner/repo>` — claims standing for a specific codebase's contracts
- `kind:<arbitrary>` — extension point for future scope types

The index is itself a contract memento, signed, content-addressable. Each commit to it produces a new index CID. Consumers pin which index CID they trust.

## Trust policy at the consumer

The verifier returns `{ ok, signer }`. The consumer applies trust policy:

```typescript
// Example consumer trust policy
const TRUSTED_SIGNERS = {
  "language:rust": "ed25519:<rust-lang-team-pubkey>",
  "language:php": "ed25519:<foundation-pubkey>",  // PHP has no steward yet
  // ...
};

function shouldTrust(catalogScope: string, result: VerifyResult): boolean {
  if (!result.ok) return false;
  const trustedKey = TRUSTED_SIGNERS[catalogScope];
  return trustedKey === result.signer;
}
```

Variations: pin multiple signers per scope and require any-of (lower bar) or all-of (higher bar). Pin the foundation as a fallback when no steward exists. Refuse to trust catalogs older than N versions. The protocol gives you the verified signature; the policy is yours.

## Show absence

The protocol's pressure mechanism is making missing signatures visible. When a consumer queries the federated index for "language:rust" and there is no `language-steward` entry, the CLI surfaces it explicitly:

```
$ provekit baselines list --language=rust

  rust-baseline-v1                  authoritative: foundation-baseline (advisory)
                                    no language-steward signature for rust 1.81

To use the advisory baseline, pin:
    ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=
```

If the steward later signs and lists in the index, the same query becomes:

```
$ provekit baselines list --language=rust

  rust-baseline-v1                  authoritative: language-steward
                                    signed by: ed25519:<rust-lang-team>
                                    foundation-baseline also available (advisory)
```

The advisory baseline doesn't disappear when the steward signs; consumers may pin either. Visibility of the gap drives the steward toward signing.

## Worked example

`docs/contributing/signing-your-own-catalog-example/` ships a complete reproducible ceremony:

- A fixture Ed25519 keypair (`fixture-signer.key` + `fixture-signer.pub`) for a fictional "Acme Corp" who claims to maintain a fictional "acmelib"
- A mini catalog of 3 ContractDecls about acmelib (`acmelib-v1.proof`)
- A signed federated index entry (`acmelib-known-signer.toml`)
- A consumer verification script (`verify-acmelib.sh`) that pins Acme's key and verifies the catalog

To reproduce:

```sh
cd docs/contributing/signing-your-own-catalog-example
./mint.sh                    # mints acmelib-v1.proof from the source
./verify-acmelib.sh          # verifies it pins to Acme's fixture key
```

Both scripts succeed. The example demonstrates the full signing → publishing → verifying loop without any foundation involvement past the index entry, and the index entry itself can be skipped if the consumer pins Acme's key directly.

The fixture key is published in the example for reproducibility; do not use it for real signing.

## What this doc is NOT

- It is not a list of approved signers. The federated index is not curated by the foundation; listing means "self-declared standing," not "foundation endorsement."
- It is not a gate. Anyone can sign catalogs without listing in the index. Consumers pin keys directly.
- It is not a trust framework. The protocol records signatures; consumers apply trust policy. The protocol's job ends at "this signature is internally consistent."

## See also

- #253 launch v1.0.0 epic
- #248 verify_proof is consistency-only; trust pin is callsite
- #254 / `baseline-catalog-rubric.md` — what counts as a basic catalog
- #256 DSL extension survey
- `protocol/specs/2026-04-30-lift-plugin-protocol.md` — RPC shape catalog authoring uses
