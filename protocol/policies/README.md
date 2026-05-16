# Policy mementos

Content-addressed admission policies consumed by the substrate's promotion pipeline. Each file in this directory is a concrete PolicyMemento instance per the CDDL in `protocol/specs/2026-05-13-policy-memento.md`. The substrate does NOT pick policy; it preserves whichever policy the operator signs.

## Files

### `intra-vendor-empirical-admission.json`

A `threshold-policy-memento` admitting a contract from "documentary" to "empirically-witnessed" tier when a single vendor's signed witness set crosses the threshold. Threshold: `consensus_vector.total_sample_count >= 4` with full loss-dimension coverage.

Scope is intra-vendor. The policy does NOT merge contracts across vendors. Vendors agreeing on a contract is automatic by JCS canonicalization to the same CID; vendors disagreeing produces distinct CIDs and the substrate carries both. The operator picks via `library-bindings.json` (see issue #981).

See:
- `protocol/specs/2026-05-13-policy-memento.md` for the PolicyMemento CDDL.
- `protocol/specs/2026-05-14-witness-consensus-promotion.md` for the witness-consensus-promotion v1.0 CLI spec.
- `protocol/specs/2026-05-14-witness-consensus-promotion-v1.1-consensus-vector.md` for the seven-axis consensus vector amendment.

## What is NOT in this directory

- Substrate-merge policies: there is no cross-vendor merge step. CID equality is the merge.
- Vendor-selection policies: that's operator config (`library-bindings.json`), not substrate.
- Migration acknowledgment policies: that's the migrate command's existing RefusalMemento handling (see issue #984).

## Authoring conventions

- Filename: kebab-case description of the admission target.
- JCS-canonical JSON. Alphabetical key order.
- Reference the relevant spec(s) in the file's `decision_payload_schema.$comment`.
- `provenance_cid` is the CID of the author's signed attestation that authored this policy. Use a placeholder of all zeros until signed.
