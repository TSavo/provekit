# Error Codes

Sugar diagnostic codes are stable handles for verifier, lift, bridge, and extension failures. Messages may improve over time; codes should remain suitable for editor filters, CI policy, and support searches.

| Code | Meaning | Typical next step |
|---|---|---|
| `SUGAR_E001` | Contract violation. | Inspect the producer postcondition, consumer precondition, and failing source range. |
| `SUGAR_E002` | Missing or unresolved CID. | Check artifact paths, accepted witness roots, and bundle contents. |
| `SUGAR_E003` | Signature or signer policy failure. | Verify the signing key, trust policy, and signed bytes. |
| `SUGAR_E004` | Protocol catalog mismatch. | Run `sugar verify-protocol` and check whether a PEP transition is admitted. |
| `SUGAR_E005` | Canonicalization mismatch. | Compare canonical bytes before hashing; do not compare host-language structures. |
| `SUGAR_E006` | Bridge target mismatch. | Confirm source CID, target CID, and accepted implication witness. |
| `SUGAR_E007` | Extension body rejected. | Validate the GCP, ORP, CBP, FRP, or PEP body with the relevant checker. |
| `SUGAR_W001` | Solver fallback required or timed out. | Add a cached implication witness or simplify the lifted obligation. |
| `SUGAR_I001` | Contract lifted successfully. | Informational; no action needed. |
| `SUGAR_H001` | Suggested bridge, lift, or annotation improvement. | Optional editor hint. |

## Emission Rules

- Use `error` for proven violations and fail-closed verifier conditions.
- Use `warning` when the claim is not rejected but requires solver or policy fallback.
- Use `information` for successful lift and graph-discovery events.
- Use `hint` for editor suggestions that do not affect verification.

The diagnostic source string is always `sugar`.

## Read Next

- [Debugging a failed handshake](../how-to/debugging-a-failed-handshake.md).
- [IDE integration overview](../how-to/ide-integration/overview.md).
- [Writing an LSP plugin](../contributing/writing-an-LSP-plugin.md).
