# ProvekIt compared to SBOM formats (CycloneDX, SPDX)

SBOMs are inventory; ProvekIt is behavior. They are complementary in a complete supply-chain posture.

## What an SBOM is

A Software Bill of Materials lists the components in an artifact:

```json
{
  "components": [
    {"name": "lodash", "version": "4.17.21", "purl": "pkg:npm/lodash@4.17.21"},
    {"name": "axios", "version": "1.6.0", "purl": "pkg:npm/axios@1.6.0"},
    ...
  ]
}
```

The two dominant formats:

- **CycloneDX** (OWASP): JSON or XML or Protobuf. Strong on metadata, vulnerability tracking, license info.
- **SPDX** (Linux Foundation): tag-value or JSON or YAML. ISO/IEC 5962:2021 standard. Strong on license compliance.

Both formats answer "what's in this artifact?" That's the question. Inventory.

## What an SBOM does NOT answer

- **What does the artifact do?** SBOMs don't capture behavior.
- **Are the listed components correct?** SBOMs don't verify components.
- **Do the components interact safely?** SBOMs don't check composition.

The SBOM tells you `lodash@4.17.21` is included. It does not tell you whether your usage of `lodash.parseInt` is safe, whether `lodash.parseInt`'s implementation matches your expectations, or whether there's a buffer overflow in a transitive dependency.

These questions are out of scope for inventory.

## Where SBOMs and ProvekIt overlap

Almost nowhere. The questions are orthogonal:

- SBOM: "what's there?"
- ProvekIt: "what does what's there do, and does it satisfy the contracts I depend on?"

## How they fit together

A complete supply-chain artifact:

1. **CycloneDX SBOM**: lists every component, every version, every license, every known vulnerability.
2. **ProvekIt `.proof`**: signs behavioral contracts on the components, pins the binary CID, encodes bridges.

A consumer's verification pipeline:

1. Parse the SBOM. Confirm no known-vulnerable versions are present.
2. Parse the `.proof`. Confirm the signature, verify the handshake, check `binaryCid` against the running artifact.
3. (Optional) Cross-reference: every component listed in the SBOM should have a corresponding `.proof` (if available) or be flagged as unverified.

The SBOM's role is "is this list of components acceptable?" The `.proof`'s role is "does this artifact behave as claimed?"

## Specific integrations

CycloneDX has an extension for cryptographic attestations (`vulnerabilityDisclosures` and `attestations` fields). A `.proof` could be referenced by CID from a CycloneDX SBOM:

```json
{
  "attestations": [
    {
      "type": "behavioral-contract",
      "format": "provekit-proof",
      "uri": "https://registry.example.com/blake3-512:9d57c5e4....proof"
    }
  ]
}
```

The consumer reads the SBOM, fetches the `.proof` by CID, verifies. The combination provides inventory + behavioral verification in a coordinated pipeline.

## SBOM limits ProvekIt addresses

If you have an SBOM but no behavioral verification, you know what's there but not whether it works correctly. Consider:

- A dependency's SBOM entry says `lodash@4.17.21`. Known-good version, no advisories.
- An attacker has compromised the npm registry and serves a different binary at that version.
- Your build pulls the malicious binary.

The SBOM doesn't catch this. The version string is correct; the binary differs. You'd need either:

- A registry that signs releases (Sigstore + transparency log).
- A `.proof` whose `binaryCid` matches the legitimate binary; the malicious binary's hash mismatches; verification fails.

ProvekIt's `binaryCid` provides exactly this defense. SBOM enumerates; `binaryCid` validates.

## ProvekIt limits SBOM addresses

Conversely, if you have a `.proof` but no SBOM:

- You can verify behavior of explicitly-contracted functions.
- You don't have a comprehensive inventory.
- You can't easily check "is anyone shipping a known-vulnerable transitive dependency?"

SBOMs are essential for vulnerability tracking. They list everything; they're queryable; they're standard. ProvekIt doesn't replace this.

## Where each is required by what

- **Compliance regimes** (NIST, NTIA, FDA, FedRAMP) increasingly require SBOMs for procurement.
- **No major regime requires ProvekIt yet.** It's too new.
- **Some defense / aerospace standards** require formal verification at specific assurance levels (DO-178C, Common Criteria EAL5+); none currently accepts ProvekIt-style hash-bounded verification as equivalent to ITP-checked proofs.

For compliance: ship an SBOM. ProvekIt is value-add, not a compliance requirement.

For security: ship both. SBOM for inventory, `.proof` for behavior, both for defense in depth.

## When you don't need both

- **You ship a public artifact and care about consumers' visibility into dependencies**: ship an SBOM. (ProvekIt optional but additive.)
- **You ship a closed-source artifact and care about clients' verification of behavior**: ship a `.proof`. SBOM optional for closed-source distribution; SBOM is more often required for open or shared artifacts.
- **You ship both**: ship both. They cost different things to produce; both pay off.

## The decision

SBOM is mostly compliance + inventory. ProvekIt is verification + integrity. They complement.

If your supply-chain checklist already includes "ship SBOM," add "ship `.proof`" for the behavioral layer. If it doesn't include either, decide which question is more pressing for your consumers and start there.

Most enterprises today ship SBOMs, increasingly require them from suppliers, but have no behavioral verification. Adding `.proof` to that pipeline is the high-leverage move.

## Read next

- [slsa-sigstore-in-toto-scitt.md](slsa-sigstore-in-toto-scitt.md) — the broader supply-chain framework comparison.
- [`../../security/supply-chain.md`](../../security/supply-chain.md) — supply-chain attack scenarios.
- [`../../security/what-binaryCid-catches.md`](../../security/what-binaryCid-catches.md) — what `binaryCid` provides.
