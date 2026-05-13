#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
stdout_file="$tmp/bridgeworks.json"
stderr_file="$tmp/bridgeworks.stderr"

section "Run The Whole Exhibit"
say "The full runner mints the positive stack, then applies every mutation and requires each one to fail closed."
explain_then_pause "run the whole Bridgeworks exhibit" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is being driven by the Bridgeworks runner across the whole exhibit. The runner mints the positive chain, then applies each mutation to a temp copy and requires ProvekIt to refuse the broken chain at the expected contract boundary.

Value ProvekIt adds:
This is the regression harness for the story. It proves the walkthrough is not a set of hand-curated screenshots. Every contract boundary in the stack has a broken specimen, and the ProvekIt flow must fail closed for each one.

Relationship to the chain:
The positive case demonstrates that experiment, physics, cells, gates, RTL, ISA, compiler, software identity, and software witness can be composed into one proof DAG. The mutation cases demonstrate that each dependency is real: breaking any layer prevents the root proof from being minted honestly.

What to look for:
After the prompt, the JSON report should say ok=true. Each mutation row should show refused=true and should name the expected boundary contract that caught the break.
TEXT
print_bridgeworks "menagerie/bridgeworks" --all --json
run_bridgeworks_capture "$stdout_file" "$stderr_file" "menagerie/bridgeworks" --all --json

section "Report"
jq -r '"  ok: " + (.ok|tostring)' "$stdout_file"
jq -r '.reports[0].mutations[] | "  " + .id + " -> refused=" + (.refused|tostring) + " expected=" + .expectedRefusal' "$stdout_file"

analysis_with_receipts <<'TEXT'
The ok=true line is the receipt that the full exhibit runner accepted the overall report. Each mutation row is a receipt that one broken artifact was refused and that the expected boundary contract was the one named by the report.

That proves the final claim of the walkthrough. Bridgeworks is not just one positive proof. It is a menagerie member with controlled failure modes at every layer: native artifact, lifted ProofIR contract, explicit bridge edge, witness proof, and compressed root .proof all have to agree.
TEXT

section "Point"
say "Bridgeworks is the whole stack: native artifacts, ProofIR contracts, explicit bridge edges, witness proofs, and one compressed .proof root."
