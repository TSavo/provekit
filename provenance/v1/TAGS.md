# Architectural commits to sign-tag

The following commits introduce the load-bearing architectural moves enumerated in attestation.json. Tagging each with a signed annotated git tag adds a verifiable provenance layer beneath the umbrella attestation: the tag carries the architect's claim attached to the exact commit, signed by the architect's git key, and pushed to GitHub for public verification.

Recommended ceremony:

```sh
# Per commit, an annotated, signed tag in the provenance/ namespace:
git tag -s -a provenance/<short-name> <commit-sha> -m "<claim>"
git push origin provenance/<short-name>
```

GitHub renders signed tags with a "verified" badge tied to the architect's GPG/SSH commit-signing key. The same key signs the commits in this PR and the attestation.json signing ceremony (or distinct keys, at the architect's discretion; the umbrella verifier does not require key reuse).

## The commits

| Commit  | Subject                                                                            | Suggested tag                                  |
|---------|------------------------------------------------------------------------------------|------------------------------------------------|
| 055b1a2 | docs(launch): substrate, not blockchain — architectural manifesto                  | `provenance/manifesto-base`                    |
| 2da91d5 | docs(manifesto): add §11 — the address is multi-dimensional                        | `provenance/manifesto-section-11`              |
| fb76135 | docs(manifesto): add §12 — the pin is a tuple                                      | `provenance/manifesto-section-12`              |
| f8d4363 | docs(launch): the pieces on the table — architectural derivation                   | `provenance/derivation-pieces-on-the-table`    |
| 3ed8c09 | docs(launch): path to default-on (strategic companion)                             | `provenance/path-to-default`                   |
| 48e8119 | spec(protocol): contractCid vs attestationCid separation                           | `provenance/spec-contract-cid-vs-attestation`  |
| aeacfd3 | spec(protocol): contract set extension (semver minor as substrate primitive)       | `provenance/spec-contract-set-extension`       |
| b0f552e | spec(protocol): substrate layering — envelope, header, body                        | `provenance/spec-substrate-layers`             |
| 16aaa1c | spec(protocol): version chains, pinning, and package-manager replacement           | `provenance/spec-version-chains-pinning`       |
| c353a66 | spec(bridges): normative addendum on target dimensionality (spec #97)              | `provenance/spec-bridge-target-dimensionality` |
| 3e085d9 | spec(bridges): R6 — single source of truth for cross-kit RPC bridges               | `provenance/spec-bridge-r6`                    |
| e0a4119 | spec: bridge linkage protocol (closes substrate composition arc)                   | `provenance/spec-bridge-linkage-protocol`      |
| d394f9b | spec(linker): linker-daemon-protocol v1.0.0 (LSP+linker step 2 spec)               | `provenance/spec-linker-daemon-protocol`       |
| 6df9b3b | spec(ir-formal-grammar): normative Locus type with locked JCS key order            | `provenance/spec-locus-normative`              |

## Suggested tag message template

```
Sugar architectural provenance — <short claim>

This commit introduces <one-sentence what>. It is part of the
architectural assembly attested in provenance/v1/attestation.json,
umbrella CID
blake3-512:9f2ba5c07f57a732515d465bca838f004255cb4e8cf83edbda443c28a9692b8e3010de7574f31dc1e8899f642718c75511e991d6dd2132d59487df4e0556d0fa.

Architect: Travis Robert Savo (goes by T; handle Kevlar).
```

## Why per-commit tags matter

The umbrella attestation in attestation.json is one rank-N pin per manifesto §12: a tuple of N CIDs claimed at one moment by one signer. The per-commit tags are rank-1 pins: one CID claimed at one moment by one signer, granular at the architectural-move level. Both shapes coexist; the per-commit tags let consumers cite a single architectural move (e.g., "§12 was first introduced at commit fb76135 by signer X at time T") without dragging the whole umbrella into the citation.

The per-commit tags also survive repository renames, fork merges, and history rewrites better than directory-level attestation files: a tag pinned to a commit SHA stays pinned forever, even if the file at that path moves or is renamed.
