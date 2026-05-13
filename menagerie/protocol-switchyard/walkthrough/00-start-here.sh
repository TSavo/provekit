#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Protocol Switchyard Walkthrough"
say "This is a CLI-first tour of the Protocol Switchyard exhibit."
say "Paper 10 says: protocol versions are roots, migrations are witnessed edges, and compatibility is a checked route, not a release-note promise."

explain_then_pause "check tools and prepare local binaries" <<'TEXT'
What ProvekIt is doing here:
This first script does not mint anything yet. It establishes the execution surface for the rest of the exhibit. Every later step runs through the same primitives a user would run: provekit hash, provekit protocol evolve, provekit protocol check-evolution, and the protocol-switchyard runner. The script verifies the local toolchain and prepares the local binaries.

Value ProvekIt adds:
The walkthrough does not hide behind cargo invocations or checked-in JSON fixtures. After binaries are prepared, every numbered tour script writes its own catalogs, policy, and verifier into a temp dir, then invokes ProvekIt by name. Break scripts mutate copies under temp dirs. The exhibit on disk is read-only.

What to look for:
After the prompt the script reports the toolchain and prepares any stale binaries. Later scripts should not show binary preparation again unless the source under implementations/rust or this exhibit changed.
TEXT

section "Tool Check"
need_cmd cargo
need_jq
need_cmd python3
say "cargo, jq, and python3 are available."
say "Repo root: $REPO_ROOT"
say "Exhibit:   $EXHIBIT_ROOT"

section "Binary Prep"
say "The walkthrough builds local binaries once, then invokes the protocol tools by name."
ensure_walkthrough_bins
say "provekit and provekit-protocol-switchyard are on the walkthrough PATH."

section "Tour Map"
say "01-show-v1-profile.sh         display the v1 prose specs"
say "02-mint-v1-catalog.sh         hash v1 specs and emit fromCatalogCid"
say "03-show-v2-profile.sh         display v2 specs and what changed"
say "04-mint-v2-catalog.sh         hash v2 specs and emit toCatalogCid"
say "05-mint-migration-witness.sh  invoke provekit protocol evolve, show all four CIDs"
say "06-verify-witness.sh          invoke provekit protocol check-evolution"
say "10-break-spec-bytes.sh        tamper with v2 spec, show changed-spec CID rail fires"
say "11-break-policy-mode.sh       declare migration-required against extension-only policy"
say "12-break-input-closure.sh     drop a required CID from inputCids"
say "13-break-evidence-cid.sh      tamper with catalog-diff bytes after the body was minted"
say "20-run-whole-exhibit.sh       invoke the runner end-to-end"

section "Rule"
say "The exhibit on disk is read-only."
say "Every tour or break script writes JSON into a temp directory and never mutates the spec files in profiles/http."

analysis_with_receipts <<'TEXT'
The receipts here are environmental. The tour proves paper 10 by minting a witnessed edge between two catalog roots that name spec-document CIDs, then checks that edge with the same verifier. The break scripts prove the rails by perturbing one input at a time and showing which check refuses the chain.
TEXT

next_script "01-show-v1-profile.sh"
