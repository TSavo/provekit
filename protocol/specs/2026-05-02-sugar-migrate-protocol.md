# Sugar: `sugar migrate` Verb (catalog version bumps)

**Date:** 2026-05-02
**Status:** v1.4.0+ non-normative tooling spec. This document describes a CLI verb behavior, not on-wire protocol. It is filed in `protocol/specs/` for discoverability; it is intentionally NOT listed in the protocol catalog and contributes nothing to any catalog CID.

## What this document specifies

The behavior of `sugar migrate <new-version>`, a CLI verb that automates the mechanical steps required to cut a new minor catalog version (e.g. v1.3.0 to v1.4.0). The verb is a workflow runner, not a protocol participant. It produces no on-wire bytes; it edits a fixed list of files in lockstep, then exits.

The verb exists because cutting a new catalog version touches roughly ten source files in a precise order, and skipping or misordering any one of them produces a broken release: a catalog whose CID does not match the embedded asset, or a signature that does not match the catalog, or a docstring that lies. The history of v1.1.0, v1.2.0, and v1.3.0 cuts is the empirical evidence that the dance is mechanical, error-prone, and worth automating.

## §1. Goal

Reduce the v1.X.0 cut from a ten-step manual procedure to a single command. Make the operation atomic: either every file is updated to the new version OR nothing is changed. Failure at any step rolls back to the pre-migration tree. A successful run produces a working tree that, after `git add -A && git commit`, IS the version-cut commit.

The verb is non-normative because no on-wire byte depends on its existence. A maintainer can perform the same edits by hand and produce a byte-identical commit. The verb is a convenience that codifies the file list and the order; the protocol does not know it exists.

## §2. Inputs

```
sugar migrate <new-version> [--from <prev-version>] [--declared-at <iso8601>] [--force-dirty] [--dry-run]
```

| Argument | Required | Description |
|---|---|---|
| `<new-version>` | yes | Target version label, form `vX.Y.Z` (e.g. `v1.5.0`). Must parse as a semantic-version major.minor.patch with a leading `v`. |
| `--from <prev-version>` | no | Previous version label (e.g. `v1.4.0`). When omitted, the verb reads the current `EXPECTED_CATALOG_CID` and the current catalog `version` property, derives the previous version from the latter, and refuses to proceed if they disagree. |
| `--declared-at <iso8601>` | no | ISO-8601 UTC timestamp for the catalog's `declaredAt` field. Defaults to the current UTC time at invocation, formatted as `YYYY-MM-DDTHH:MM:SSZ`. |
| `--force-dirty` | no | Allow the verb to operate on a working tree with uncommitted changes. Default behavior is to refuse and print the dirty-file list. |
| `--dry-run` | no | Print the planned mutation set (files + line numbers) but do not write. Exit code is 0 if the plan is well-formed, non-zero if any precondition fails. |

The verb is single-shot. It does not accept `--patch` or `--major` mode; semantic-version mechanics are the operator's responsibility. A `v1.3.5` to `v1.4.0` bump is the same operation as `v1.3.0` to `v1.4.0`; the verb does not interpret the version string except to format constants and filenames.

## §3. The ten mutation points

The verb performs exactly the following ten operations, in order, on the working tree. The numbering matches the canonical migration checklist; subsequent sections refer to these by number.

1. **`protocol/specs/2026-04-30-protocol-catalog.json`** (the catalog itself).
   The catalog filename is fixed at the catalog's original creation date and does NOT rotate at version bumps; v1.1.0 through v1.3.0 all use the same `2026-04-30-protocol-catalog.json` path, and the migrate verb preserves this. The verb updates the file in place: set the `version` property to the new version label, set `declaredAt` to the supplied or computed timestamp, and append any new spec property keys whose specs were drafted during the version window. Existing property values remain placeholders (the actual CIDs are filled in by step 11; see §4) until `recompute-spec-cids --write` runs. New property additions for this version come from the `migrate.toml` opt-in list (see §6) or a `--add-spec <key>=<basename>` flag passed to the verb.

2. **`tools/recompute-spec-cids/src/main.rs`** SPEC_MAP entries.
   For each spec key added in step 1, append a `(<key>, <date>-<basename>.md)` tuple to `SPEC_MAP`. Update the `print_header` line that names the current target version (currently `# Protocol catalog freeze (v1.3.0)`) to the new version label. The verb edits this file by AST-aware text replacement, not regex; if the file's structure has drifted such that the canonical anchors (the `SPEC_MAP: &[(&str, &str)] = &[` line and the `# Protocol catalog freeze` print line) cannot be found, the verb refuses to proceed.

3. **`tools/foundation-keygen/Cargo.toml`** + new signing binary.
   Append a `[[bin]]` table for `sign_catalog_v<X>_<Y>_<Z>` whose `path` points at `src/bin/sign_catalog_v<X>_<Y>_<Z>.rs`. The binary itself is created in the same step, generated from the prior version's binary as a template: copy `src/bin/sign_catalog_v<X>_<Y-1>_<Z>.rs` (or v<X-1>_<max-Y>_<Z> at major-version boundaries), substitute the version-numbered identifiers, and rewrite the `build_signed_attestation_for(...)` call to take the new version constant.

4. **`tools/foundation-keygen/src/lib.rs`** version constants.
   Add a public constant `V<X>_<Y>_<Z>_DECLARED_AT: &str = "<iso8601>"` matching the timestamp from step 1, and ensure `build_signed_attestation_for` and `signature_path_for` both accept the new version label. The two functions are version-parameterized as of v1.2.0; the verb does not need to edit them, only confirm their dispatch table contains the new label.

5. **`.sugar/catalog-signatures/v<X>.<Y>.<Z>.json`** signed attestation.
   Run the new `sign_catalog_v<X>_<Y>_<Z>` binary built in step 3. The binary writes to this canonical path. The verb does not author the file by hand; it invokes the signing binary and confirms the file's signature verifies under the foundation v0 root key.

6. **`implementations/rust/sugar-cli/assets/catalog-signature-v<X>.<Y>.<Z>.json`** embedded asset.
   Copy the file produced in step 5 byte-for-byte to the CLI's embedded-assets directory. The two files MUST be byte-identical; the verb refuses if the post-copy hash differs.

7. **`implementations/rust/sugar-cli/assets/protocol-catalog.json`** refreshed catalog.
   Copy the post-recompute catalog file (the on-disk artifact written by step 11; see §4) to the CLI's embedded-assets directory. The two files MUST be byte-identical.

8. **`implementations/rust/sugar-cli/src/protocol.rs`** version constants.
   Update `EXPECTED_CATALOG_CID` to the new catalog CID computed by `recompute-spec-cids --write` in step 11, and update `EMBEDDED_CATALOG_SIGNATURE` to reference `catalog-signature-v<X>.<Y>.<Z>.json`. The verb edits these by anchored text replacement on the existing constant declarations; if the declarations have moved or been renamed, the verb refuses.

9. **`implementations/rust/sugar-cli/src/main.rs`** version docstring.
   Update the version-label docstring (currently a single-line comment naming the supported catalog version) to the new label. The anchor is the existing line; the verb does not introduce the comment if it is missing.

10. **`docs/launch/bluepaper.md`** §0 catalog CID + Appendix A entry + version log.
    Three edits in one file:
    - §0 quotes the catalog CID. Replace the prior CID with the new one from step 11.
    - Appendix A is the catalog-version table. Append a row for the new version with date, CID, and a one-line summary supplied via `--changelog` or read from a `migrate.toml` snippet.
    - The version log at the end of the bluepaper is a flat list of `vX.Y.Z (date): summary`; append a line.

These ten points are the authoritative file list. The v1.3.0 cut commit (`2ec87ad`) modified twelve files; the two extras are spec-content edits (`protocol/specs/2026-04-30-proof-file-format.md` softened a MUST to MAY) which are version-window content authored by humans, not mechanical migrate operations. The verb does not write spec content; it cuts versions over whatever spec content the operator has already committed.

## §4. Hashing and signing flow

The ten mutation points above are interleaved with three computational steps that produce the values steps 8 and 10 need. The full sequence the verb executes is:

```
[steps 1, 2]                                  open the catalog and SPEC_MAP for the new version
[step 11: recompute-spec-cids --write]        compute every spec CID + the new catalog CID
[step 3]                                      generate the new sign_catalog_v<X>_<Y>_<Z> binary + Cargo.toml entry
[step 4]                                      register V<X>_<Y>_<Z>_DECLARED_AT
[step 12: cargo build]                        build the signing binary
[step 5: cargo run sign_catalog]              produce the canonical attestation under the foundation key
[steps 6, 7]                                  copy attestation + catalog into the CLI's embedded assets
[steps 8, 9]                                  wire the new CID + asset filename into protocol.rs and main.rs
[step 10]                                     propagate the new CID into the bluepaper
[step 13: sugar verify-protocol --signed]  final smoke test
```

Steps 11 (`recompute-spec-cids --write`), 12 (`cargo build --release --manifest-path tools/foundation-keygen/Cargo.toml`), and 13 (`sugar verify-protocol --signed`) are tooling invocations, not file mutations. They participate in the migration but produce no edits the verb itself authors; their outputs (catalog CID, signing-binary executable, verification verdict) feed the ten mutation points and the rollback decision.

The verb invokes `recompute-spec-cids --write` (the `--write` flag is mandatory after audit-#180; the safe default would refuse). The catalog CID printed by that invocation is captured into a variable and consumed by steps 8 and 10. If the invocation fails, the migration aborts.

## §5. Atomicity

The verb stages every file write into a temporary shadow directory under `.sugar/migrate-staging/<new-version>/` and only flushes the staged tree onto the live working tree as the final operation, after step 13 verifies. Concretely:

1. Create `.sugar/migrate-staging/<new-version>/` (empty; if it exists from a prior failed run, the verb refuses unless `--force-dirty` is passed).
2. For each of the ten mutation points, compute the new file content and write it to the staging directory at the same relative path.
3. Run the tooling commands (steps 11, 12, 13) against the **staged tree** by setting working directories appropriately. `recompute-spec-cids --write` is invoked with the staging path; `cargo build` and `sugar verify-protocol --signed` are run against the staging tree merged in memory with the live tree (using a temporary overlay or a copied workspace, depending on platform support). The verifier MUST see the staged catalog, not the live one.
4. If every tooling command succeeds, rename the ten staged files onto the live working tree atomically (per-file `rename(2)`; the verb does not require a single transaction across all ten because the live tree was clean at start per §7's pre-flight, so the rename sequence is monotonic).
5. If any tooling command fails, delete the staging directory and exit non-zero. The live working tree is untouched.

The verb prints the staging directory path on failure so the operator can inspect the partial work. On success, the staging directory is deleted.

## §6. Configuration

A repository may carry a `tools/migrate/migrate.toml` file (location not normative; this document fixes it for the reference implementation) that lists:

- The new spec keys to add in step 1 (since this is per-version-window editorial content, not derivable from the working tree alone).
- Optional changelog snippet for the bluepaper Appendix A row and version-log line.

When `migrate.toml` is absent, the verb reads `--add-spec` and `--changelog` flags from the command line. When both are absent and the version-window has no new specs to add, the migration proceeds with the catalog's existing property set unchanged (a "rebake-only" minor version).

## §7. Pre-flight checks

Before any staging directory is created, the verb runs the following checks. Any failure aborts the migration with a clear message; none of them mutate the working tree.

1. The working tree is clean (`git status --porcelain` is empty). Refuse unless `--force-dirty`.
2. The supplied `<new-version>` parses as `vX.Y.Z` with non-negative integers.
3. The supplied or derived `--from` value matches the current on-disk catalog `version` property.
4. The current catalog passes `recompute-spec-cids` (read-only default mode). If the current catalog is broken, the migration cannot trust its starting point.
5. The foundation signing key is reachable (the keygen tool's normal lookup path resolves a usable key).
6. No staging directory exists from a prior failed migration unless `--force-dirty`.

## §8. Open questions

Marked clearly because they are unresolved at spec time and may need decisions before implementation.

- **Should the verb run `sugar verify-protocol --signed` as a final smoke test?** Position: yes, as step 13 above. The cost is one CLI invocation; the benefit is catching wiring errors that the per-step assertions do not. This document declares step 13 as authoritative and removes the question from the implementation, but flags it here for visibility because earlier drafts treated it as optional.

- **Where does the changelog text come from?** Three candidates: (a) `--changelog "..."` flag, (b) `migrate.toml` snippet, (c) auto-derived from `git log <prev-tag>..HEAD`. Option (c) is tempting but couples the verb to git, which the rest of Sugar avoids; the catalog has no git dependency. Recommendation: support (a) and (b); refuse if neither is supplied and the migration is non-trivial (i.e., a new property was added).

- **What about major-version bumps (v1.X.Y to v2.0.0)?** The verb as specified handles minor bumps. Major bumps may require breaking-change attestation, multi-version signature carryover, and a different naming convention for the catalog file. Out of scope for v1; punt to a future `sugar migrate-major` verb if the need arises.

- **Should the verb commit the migration?** Position: no. The verb produces a clean working tree with the migration applied; the operator commits with their own message and review. Auto-commit would couple to git and would risk landing a bad migration as a single commit that is harder to amend than a staged set of changes.

- **Cross-language asset mirroring.** The CLI's embedded assets live in the Rust crate. If, in the future, other language CLIs ship embedded catalogs, the verb's step 7 generalizes to N targets. For now, only Rust is in scope; document the assumption in the implementation but do not generalize prematurely.

## §9. What this verb is NOT

- **Not a runtime or normative protocol.** No on-wire byte depends on the verb. A maintainer can perform every step by hand and produce a byte-identical commit. The verb is automation; the protocol is the file contents.

- **Not a multi-version catalog merge.** The verb cuts one new version from the current state. It does not reconcile divergent forks of the catalog or merge property sets across branches. A repository with two divergent in-flight catalog drafts must resolve the divergence before invoking the verb.

- **Not a fork handler.** The verb operates on a single foundation key path; it does not support multi-signer attestations, key rotation in the middle of a migration, or alternative trust roots. Such cases require a deliberate manual cut.

- **Not a spec-content authoring tool.** The verb does not write spec markdown, edit normative protocol semantics, or change CDDL grammars. Spec content is human-authored before the migration; the verb mints CIDs over the bytes the operator has already committed.

- **Not safe on a dirty working tree by default.** The verb refuses to proceed if `git status --porcelain` is non-empty unless `--force-dirty` is passed. This is to protect uncommitted edits from being interleaved with the staged migration writes during the atomic-rename phase.

- **Not a continuous-integration entry point.** The migration produces a working tree intended for human review and a deliberate commit. CI MAY run `recompute-spec-cids` (default safe mode) and `sugar verify-protocol --signed` to confirm a freshly-pulled tree is internally consistent, but it MUST NOT invoke `sugar migrate` automatically.

## §10. Conformance criteria

This is a non-normative spec, so "conformance" is shorthand for "behaves the way this document describes." A reference implementation conforms if:

1. It refuses to proceed on a dirty working tree without `--force-dirty`.
2. It writes nothing to the live tree until step 13 succeeds in the staging tree.
3. The catalog CID it propagates to steps 8 and 10 is the byte-identical value `recompute-spec-cids --write` printed in step 11.
4. The signature it propagates to steps 6 and 7 is the byte-identical artifact step 5 produced.
5. After a successful run, `sugar verify-protocol --signed` against the live tree exits 0.
6. After a failed run, the live tree is byte-identical to its pre-invocation state (no partial writes).

A future implementation that diverges from this document is welcome to do so; the protocol does not depend on the verb's behavior. This document is a contract between the verb's authors and its operators, not between peers on the wire.

## §11. Relationship to the protocol catalog

This spec lives in `protocol/specs/` for discoverability (any operator looking for "how do I cut a new version" finds it next to the other version-management docs), but it is intentionally NOT a property of the protocol catalog. Its CID is therefore not minted by `recompute-spec-cids`; its filename does not appear in `SPEC_MAP`; the foundation key has nothing to attest about it. Editing this document changes no on-wire byte, breaks no peer, and does not require a catalog re-bake.

The recursion of the protocol catalog (per `2026-04-30-protocol-catalog-format.md` §3) is unaffected: the catalog continues to bootstrap from `protocol-catalog-format.md`, which describes how the catalog is hashed; the migrate verb is a tool that updates the catalog file, not a participant in the catalog's content-addressing.
