#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
out="$tmp/out"

section "Mint The Proof DAG"
say "Mint drives lift, satisfies witness demands through lower, and writes .proof artifacts."
explain_then_pause "mint the positive .proof DAG" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is composing lift, lower, and mint. It lifts the native artifacts into ProofIR, sees that the software contract demands a C witness, invokes the lower protocol to obtain that witness evidence, mints the witness as a .proof, and then mints the main .proof that depends on it.

Value ProvekIt adds:
This is the first durable proof step. Without ProvekIt, the exhibit would be a pile of artifacts and a story about how they relate. Mint turns that story into content-addressed evidence: a root proof CID, a contract-set CID, and proof files whose bytes can be inspected and hashed.

Relationship to the chain:
The main .proof does not stand alone. It includes contracts and bridge implications, and the software contract points to the witness proof produced by the C lowerer. The final root CID is the compression moment: one addressable artifact inherits the whole p -> q chain through memento CIDs.

What to look for:
After the prompt, the output should show the main proof CID, contract-set CID, and proof files written beside each other. There should be more than one .proof file because the C witness is minted as protocol evidence, not hidden as a side channel.
TEXT
mint_positive "$out"

section "Mint Result"
jq -r '"  main proof CID: " + .filenameCid' "$out/mint.json"
jq -r '"  contract set:   " + .contractSetCid' "$out/mint.json"
jq -r '"  main proof:     " + .proofFile' "$out/mint.json"

section "Proof Files Written"
find "$out" -maxdepth 1 -name '*.proof' -print | sort | sed 's/^/  /'

analysis_with_receipts <<'TEXT'
The proof CID is the first receipt: it is the address of the main proof artifact ProvekIt just minted. The contract-set CID is the second receipt: it identifies the lifted contract set that the proof is about. The proof-file list is the third receipt: it shows that mint produced durable .proof files, not just terminal output.

The important relationship is that the witness proof is a peer protocol artifact, not an invisible helper. Mint had to satisfy the C witness demand before the main proof could honestly include checked_add_u8.postcondition. The root proof CID is therefore a compressed handle to a graph that includes the lifted contracts, bridge implications, authorities, and witness dependency.
TEXT

section "Point"
say "The main proof and witness proof are both .proof files. The witness proof is not a side channel."

next_script "06-walk-proof-cids.sh"
