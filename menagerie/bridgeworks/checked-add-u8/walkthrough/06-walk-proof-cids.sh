#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
out="$tmp/out"
dump_json="$tmp/main-dump.json"

section "Walk The CID DAG"
say "The main .proof carries authorities, contracts, implications, and an input CID to the witness proof."
explain_then_pause "mint and inspect the CID DAG" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting a fresh positive proof and then dumping it back out as structured JSON. The point is to stop treating the .proof as an opaque success token and inspect the graph it actually contains.

Value ProvekIt adds:
The main .proof is not just a receipt saying the run passed. It is a content-addressed object graph. Inside it are separate mementos for contracts, bridge implications, and authorities. Each member is a durable claim with its own kind, content, and CID. That is what lets the root CID compress the chain without flattening away the relationships.

Relationship to the chain:
The software contract does not stand alone inside that graph. It has input CIDs, one of which points to the witness proof minted by lower. The implication mementos are the bridgework: experiment supports physics, physics supports cells, and so on up to compiler lowering supporting software. The main proof root CID carries those dependencies by reference.

What to look for:
After the prompt, the first dump should show the member kinds in the main proof and then the input CIDs on checked_add_u8.postcondition. One of those CIDs is the witness proof that will be opened in the next phase.
TEXT
mint_positive "$out"

main_proof="$(jq -r '.proofFile' "$out/mint.json")"
main_cid="$(jq -r '.filenameCid' "$out/mint.json")"
print_provekit dump "$main_proof" --json --quiet
run_provekit_capture "$dump_json" "$tmp/dump.stderr" dump "$main_proof" --json --quiet

section "Main Proof Member Kinds"
jq -r '.members | to_entries | map(.value.header.kind) | sort | group_by(.) | map("  " + .[0] + ": " + (length|tostring))[]' "$dump_json"

section "Software Contract Input CIDs"
say "One of these inputs is the witness proof CID minted by lower."
jq -r '.members | to_entries[] | select(.value.header.kind == "contract" and .value.header.name == "checked_add_u8.postcondition") | .value.header.inputCids[] | "  " + .' "$dump_json"

analysis_with_receipts <<'TEXT'
The member-kind counts are the first receipt. They prove the main .proof contains different kinds of mementos instead of one flattened blob. Contract members carry boundary claims. Implication members carry p -> q bridge edges. Authority members record who is allowed to speak for those claims.

The software contract input CIDs are the second receipt. They prove checked_add_u8.postcondition depends on other content-addressed artifacts. One of those inputs is the witness proof minted by lower, so the software contract is not accepted merely because the lifter saw a marker. It is connected to behavioral evidence by CID.
TEXT

section "Witness Proof Written Beside Main Proof"
explain_then_pause "open the witness proof by CID" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is dumping the witness .proof that was written beside the main proof. This is not a generated C file checked into the repository. It is a protocol artifact minted by lower and referenced by CID from the main proof.

Value ProvekIt adds:
The witness proof gives runtime evidence a durable envelope. It can carry the generated witness artifact CID, the number of cases checked, the witness member, and the signer identity for the lowerer. That makes the value check inspectable without mixing generated code into the source tree.

Relationship to the chain:
The main software contract depends on this witness proof. That means the final checked_add_u8.postcondition claim is not accepted merely because the lifter saw the right marker. It is accepted because the lowerer emitted witness evidence and that witness became part of the proof DAG.

What to look for:
After the prompt, the output should show a witness member, cases checked, and the generated C witness artifact CID. Those are the behavioral evidence behind the software contract.
TEXT
for proof in "$out"/*.proof; do
  cid="$(basename "$proof" .proof)"
  if [ "$cid" != "$main_cid" ]; then
    printf '  witness proof: %s\n' "$proof"
    print_provekit dump "$proof" --json --quiet
    run_provekit_capture "$tmp/witness-dump.json" "$tmp/witness-dump.stderr" dump "$proof" --json --quiet
    jq -r '.members | to_entries[] | select(.value.header.kind == "witness") | "  witness member: " + .key' "$tmp/witness-dump.json"
    jq -r '.members | to_entries[] | select(.value.header.kind == "witness") | "  cases checked: " + (.value.metadata.evidence.casesChecked|tostring)' "$tmp/witness-dump.json"
    jq -r '.members | to_entries[] | select(.value.header.kind == "witness") | "  generated C witness artifact CID: " + .value.metadata.evidence.witnessArtifact.cid' "$tmp/witness-dump.json"
  fi
done

analysis_with_receipts <<'TEXT'
The witness member line is the receipt that this second .proof is a witness proof. The cases-checked line proves the lowerer did real domain work rather than only emitting metadata. The generated C witness artifact CID proves the runtime-generated emitter is represented by content address, even though its source file is not checked into the repo.

Together, those receipts prove the initial claim for this script: the main proof compresses the contract and implication DAG, while the witness proof carries the behavioral evidence that the software contract needs.
TEXT

section "Point"
say "The generated C witness is represented by a CID in witness evidence; the durable protocol object is the .proof."

next_script "07-break-experiment.sh"
