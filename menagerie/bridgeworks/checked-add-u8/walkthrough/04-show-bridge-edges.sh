#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
lift_json="$tmp/lift.json"
lift_stderr="$tmp/lift.stderr"

section "Bridge Edges"
say "A bridge edge is the explicit p -> q contract between neighboring domains."
say "This step does not prove the edge. It shows the lifter made the edge claim explicit in ProofIR."
explain_then_pause "lift and inspect bridge edges" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is lifting the same exhibit again, but this script focuses the reader on the implication members in the lifted output. Those implications are the bridge edges: explicit p -> q claims between neighboring domains.

Value ProvekIt adds:
A bridge edge is not just adjacency in a diagram. ProvekIt turns it into a first-class claim with an antecedent, consequent, authority, and witness text. That means later proof minting can include the edge as a content-addressed memento instead of relying on a human to remember why two layers were connected.

Relationship to the chain:
The final edge is compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition. That says the software postcondition is only justified if compiler lowering really preserves the machine-level checked-add semantics. The C contract is not floating alone. It depends on compiler lowering, which depends on ISA semantics, which depends on RTL, gates, cells, physics, and experiment.

What to look for:
After the prompt, the full ProofIR JSON appears first. The highlighted raw lines should show the compiler contract as antecedent, the C postcondition as consequent, and the proofWitness string that makes the implication readable.
TEXT
print_provekit lift "menagerie/bridgeworks/checked-add-u8" --json --quiet
run_provekit_capture "$lift_json" "$lift_stderr" lift "menagerie/bridgeworks/checked-add-u8" --json --quiet

section "Full Lifted ProofIR JSON With Line Numbers"
show_json_file "$lift_json"

section "Relevant Bit: Raw Bridge Edge Lines"
highlight_raw_line "$lift_json" '"antecedent": "compiler.lowering.preserves_checked_add",' "This is the compiler-side contract in the p -> q edge."
highlight_raw_line "$lift_json" '"consequent": "checked_add_u8.postcondition",' "This is the software-side contract that must follow from it."
highlight_raw_line "$lift_json" '"proofWitness": "compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition",' "The raw ProofIR edge makes the cross-domain implication explicit."

analysis_with_receipts <<'TEXT'
The receipts are the three raw implication lines. The antecedent line shows the compiler contract. The consequent line shows the software contract. The proofWitness line shows the exact p -> q relationship lifted into ProofIR.

That proves the initial claim for this step: Bridgeworks is not merely placing software after compiler in a diagram. ProvekIt has made the compiler-to-software dependency explicit as data. Later, mint can turn this edge into a content-addressed implication memento instead of relying on this explanation.
TEXT

section "Point"
say "Now the claim is visible. The next scripts mint it into a .proof DAG, then mutate each layer to show the chain fails closed."

next_script "05-mint-proof-dag.sh"
