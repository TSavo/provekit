# Reporting vulnerabilities

This document describes how to report security issues in Sugar.

## What counts as a vulnerability

A vulnerability is anything that breaks a security claim the protocol or its implementations make. Concretely:

- A way to forge a memento that passes signature verification without holding the signing key.
- A way to produce two different mementos with the same CID.
- A way to make the verifier accept a `.proof` whose `binaryCid` doesn't match the actual binary.
- A way to make the verifier accept a contract whose canonical IR doesn't agree byte-for-byte across kits.
- A way to make the conformance harness pass for a non-conformant kit.
- A way to bypass the signature verification.
- A solver-output forgery that tricks the verifier into caching a wrong implication memento.
- A canonicalization implementation bug that produces different bytes than the spec demands for some valid input.
- A timing or side-channel attack on the kit's signing or verification path.

Bugs that are not vulnerabilities:

- Cosmetic issues (typos, formatting).
- Performance issues (unless they constitute a denial-of-service attack on the verifier).
- Coverage gaps in lift adapters (a missing annotation handler is not a vulnerability; it's a feature request).
- Documentation gaps.

When in doubt, file a security issue rather than a public bug. We can downgrade if it's not a security issue; we can't upgrade after the public report.

## Where to report

Two channels:

### Private security issues (vulnerabilities)

For anything that fits the "vulnerability" definition above:

- **Email**: `security@provekit.dev` (or whatever address the project's `SECURITY.md` lists).
- **GitHub Security Advisories**: open a private advisory on the project's GitHub repository.

Do NOT post vulnerability details in public issue trackers, public PRs, or public chat channels.

### Public issues (everything else)

For bugs that don't fit the vulnerability definition:

- GitHub issues on the appropriate repository.
- Discussion in the project's discussion forum.

## What to include in a report

The minimum useful report:

1. **Affected component**: which kit, which adapter, which CLI version.
2. **Severity assessment**: critical / high / medium / low. The reporter's view; we may adjust.
3. **Reproduction**: minimal steps that reproduce the issue. Include source code, command lines, expected behavior, observed behavior.
4. **Impact analysis**: who is affected, what they experience, what an attacker can achieve.
5. **Suggested fix**: optional but appreciated.

A good report makes triage fast. A bad report (vague, without reproduction, without scope) takes longer to verify.

Example template:

```
## Affected component
provekit-canonicalizer, version 1.1.0, in implementations/rust/provekit-canonicalizer

## Severity
High: produces non-canonical bytes for a specific edge case, breaking
cross-kit conformance.

## Reproduction
Given the IR formula:
  forall x: Int. eq(x, "0")  // String literal "0" with primitive Int sort

The canonicalizer emits:
  blake3-512:abc123...

But the JCS spec requires:
  blake3-512:def456...

Repro:
  cd implementations/rust
  cargo test --test canonicalizer -- --nocapture
  # See "Test 17: type-coerced primitive" failing as of v1.1.0.

## Impact
Any kit producing IR with type-coerced primitives produces canonical bytes
that don't match other kits' canonical bytes for the same formula. Cross-kit
conformance breaks. Tier 1 of the handshake fails for affected formulas.

## Suggested fix
Ensure the canonicalizer rejects type-coerced primitives, or normalizes
them to the spec-canonical form.
```

## How we handle reports

1. **Acknowledge** within 48 hours. We confirm receipt; we don't yet confirm severity.
2. **Triage** within one week. We reproduce, assess severity, and decide the response timeline.
3. **Patch** as soon as feasible. For critical issues, we cut a patch release.
4. **Disclose** after patch. Coordinated disclosure timeline is typically 30-90 days from report, longer if the patch requires multi-kit coordination.

## Coordinated disclosure

For high-severity vulnerabilities affecting the protocol catalog or multiple kits, disclosure is coordinated:

- We notify maintainers of all affected kits.
- We agree on a patch and a release date.
- We publish a security advisory on the release date with affected versions, fixed versions, and migration notes.
- We assign a CVE if the vulnerability is in widely-deployed code.

Affected users are encouraged to upgrade promptly.

## Bug bounty

Sugar does not currently offer a bug bounty. We recognize and credit reporters in security advisories. Reporters who prefer anonymity will be respected.

## What we ask of reporters

- **Don't exploit beyond what's needed to demonstrate the issue.** A proof-of-concept on your own data is sufficient.
- **Don't access data you don't own.** No production systems, no other users' data.
- **Don't disclose publicly until coordinated disclosure date.** Public disclosure of an unpatched vulnerability puts every user at risk.
- **Don't extort.** A vulnerability report that comes with a payment demand is not received as a security report.

In exchange, we commit to:

- Acknowledging your report.
- Providing public credit (if you want it).
- Working with you on disclosure timelines.
- Not pursuing legal action against you for good-faith security research.

## Out-of-scope

Some things look like security issues but aren't Sugar's responsibility:

- **Vulnerabilities in Z3 or other third-party solvers.** Report to the solver project.
- **Vulnerabilities in OpenSSL, BLAKE3 reference implementation, or other cryptographic libraries.** Report to those projects.
- **Vulnerabilities in language runtimes** (Rust toolchain, Node.js, Python interpreter). Report to those projects.
- **Vulnerabilities in source-library annotation systems** (Bean Validation, zod, pydantic). Report to those projects.

If you discover a chain of vulnerabilities involving Sugar and a third party, report to both. Our triage team will coordinate where appropriate.

## What past advisories have looked like

(none as of v1.1.0; this section will populate as advisories ship)

## Read next

- [threat-model.md](threat-model.md): the threat surface Sugar covers.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md): what the protocol's signatures buy.
- [`SECURITY.md`](../../SECURITY.md) (when written): the canonical security policy at repo root.
