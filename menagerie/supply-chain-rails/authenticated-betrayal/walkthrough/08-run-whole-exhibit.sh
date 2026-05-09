#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
runner_json="$tmp/runner.json"
runner_stderr="$tmp/runner.stderr"

section "Run Whole Exhibit"
explain_then_pause "run the rail matrix" <<'TEXT'
The final step runs the exhibit runner. This is not a separate proof engine. The runner shells through provekit for package inspection, lift, lower, mint, version compatibility, binary verification, and policy admission. Its job is orchestration and checking that the receipts compose into the promised green-red story.

The important thing to see is the shape of the final matrix. Conventional fixture receipts are green. ProvekIt then rejects on witness, contract-set, binary, and CI input-closure rails, each with its own receipt field.

What to look for: the final report says ok true for the exhibit, conventional receipts green, and red rails named with specific reasons.
TEXT

print_runner --json
run_runner_capture "$runner_json" "$runner_stderr" --json

section "Whole Exhibit JSON"
show_json_file "$runner_json"
highlight_raw_line "$runner_json" '"tool": "slsa-verifier"' "SLSA verifier evidence stays green."
highlight_raw_line "$runner_json" '"tool": "in-toto-verify"' "in-toto verifier evidence stays green."
highlight_raw_line "$runner_json" '"filenameCid":' "The admitted baseline .proof is content-addressed."
highlight_raw_line "$runner_json" '"proofFile":' "The baseline proof exists as a concrete .proof artifact."
highlight_raw_line "$runner_json" '"reasonCode": "env-secret-read"' "The preserved-contract witness rail catches the poisoned behavior."
highlight_raw_line "$runner_json" '"missingContracts": [' "The weakened-contract path trips the version rail."
highlight_raw_line "$runner_json" '"reason": "binaryCid mismatch"' "The substituted-bytes path trips the binary rail."
highlight_raw_line "$runner_json" '"reason": "inputClosureCid mismatch"' "The stale-CI path trips the closure rail."

analysis_with_receipts <<'TEXT'
This is the exhibit claim with receipts. The package has green conventional fixture evidence, but ProvekIt refuses admission when the release betrays a preserved contract, weakens the contract set, substitutes bytes, or reuses stale CI evidence.

The proof is not a poisoned tarball. The proof is the receipt chain showing exactly which claim stayed green and exactly which rail went red.
TEXT
