#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
project="$(copy_exhibit_to_temp)"
plan="$tmp/witness-plan.json"
lower_stdout="$tmp/lower.stdout"
lower_stderr="$tmp/lower.stderr"
lift_json="$tmp/lift.json"
lift_stderr="$tmp/lift.stderr"
out="$tmp/out"
status=0

mkdir -p "$out"
cp "$EXHIBIT_ROOT/mutations/software/counterfeit-contract/checked_add_u8.c" "$project/artifacts/software/checked_add_u8.c"
write_witness_plan "$plan"

section "Break Software Behavior Without Breaking Lift Identity"
say "This C file advertises checked_add_u8.postcondition, so the lift identity step can still pass."
say "But the implementation disables the overflow branch."
explain_then_pause "inspect the behavior mutation" <<'TEXT'
What ProvekIt is doing here:
ProvekIt has not run yet in this phase. The walkthrough first shows the exact source mutation that will be handed to ProvekIt. This matters because the contract identity stays the same while the implementation behavior changes.

Value ProvekIt adds:
This is the gap that identity-only lifting cannot close. A plain marker check can see checked_add_u8.postcondition, but it cannot know whether the C function actually returns overflow=true for 1 + 255. ProvekIt adds the lower/witness path that can test the behavior demanded by the lifted contract.

Relationship to the chain:
The compiler-to-software bridge can still land on checked_add_u8.postcondition. The failure is no longer "wrong contract name." The failure is that the software artifact claims the right boundary contract but violates it at runtime.

What to look for:
After the prompt, the diff should show that the overflow branch has been disabled while the provekit:contract marker remains checked_add_u8.postcondition.
TEXT
show_mutation_diff \
  "$EXHIBIT_ROOT/artifacts/software/checked_add_u8.c" \
  "$project/artifacts/software/checked_add_u8.c" \
  "artifacts/software/checked_add_u8.c"
grep -n "provekit:contract" "$project/artifacts/software/checked_add_u8.c"
grep -n "if (0 && wide >= 256)" "$project/artifacts/software/checked_add_u8.c"

analysis_with_receipts <<'TEXT'
The diff is the receipt for the behavioral mutation: the overflow branch is disabled. The grep receipts prove the contract marker is still checked_add_u8.postcondition while the implementation now contains if (0 && wide >= 256).

That proves the initial claim for this phase. Identity should still pass, because the contract name did not change. A value witness should fail, because the behavior changed.
TEXT

section "Lift Still Sees The Right Contract Name"
explain_then_pause "lift the mutated source identity" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is lifting the mutated project. The lifter should still find checked_add_u8.postcondition because the marker was not changed. It should also still demand the C witness for that contract.

Value ProvekIt adds:
This phase proves the failure is not a naming failure. ProvekIt separates the structural lift from behavioral discharge. That separation lets the walkthrough show the exact point where identity succeeds but witness generation fails.

Relationship to the chain:
The bridge edge compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition still has a target after lift. The chain has made it to the software boundary, but it has not yet earned the right to mint the software postcondition.

What to look for:
After the prompt, the lift summary should still say lifted: checked_add_u8.postcondition and should still show a demanded witness attached to that same contract.
TEXT
print_provekit lift "$project" --json --quiet
run_provekit_capture "$lift_json" "$lift_stderr" lift "$project" --json --quiet
jq -r '.ir[] | select(.kind == "contract" and .name == "checked_add_u8.postcondition") | "  lifted: " + .name' "$lift_json"
jq -r '.witnesses[] | "  demanded witness: " + .id + " -> " + .attachTo' "$lift_json"

analysis_with_receipts <<'TEXT'
The lifted line is the receipt that identity still succeeds: ProvekIt still sees checked_add_u8.postcondition. The demanded-witness line is the receipt that lift refuses to treat identity as proof of behavior.

Together, those receipts prove the distinction this script is built around. The artifact claims the right contract, but ProvekIt still requires lower to produce behavioral evidence before mint can accept it.
TEXT

section "Lower Witness Fails Red"
say "Now Rust CLI invokes the C lowerer. The C lowerer generates and runs the C value witness in a temp directory."
explain_then_pause "lower the witness obligation into C evidence" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is invoking lower, not lift. The Rust CLI reads the witness requirement, dispatches to the C lowerer, and asks it to realize evidence for checked_add_u8.postcondition. The C lowerer generates a C witness program at runtime, compiles it, runs it over the exhaustive u8 domain, and returns a lower-result JSON object.

Value ProvekIt adds:
This is the behavioral evidence path. The generated C emitter is not checked in, and the witness result is not an informal log. ProvekIt captures the generated witness artifact, the counterexample, and the realizer result in JSON that can be minted into a witness .proof when it succeeds. Here it does not succeed, so the chain goes red.

Relationship to the chain:
The software contract is the final q in the bridge chain. It cannot be minted merely because the marker and bridge edge exist. It needs a witness proving that the implementation satisfies the postcondition. The counterexample breaks that final discharge.

What to look for:
After the prompt, the full lower-result JSON should be line-numbered. The highlighted source line is the generated C emitter. The highlighted error and message lines should show counterexample a=1 b=255, with expected overflow=true and observed overflow=false.
TEXT
print_provekit lower --project "$project" --surface c --mode witness --plan "$plan" --out "$out" --json --quiet
set +e
run_provekit_capture "$lower_stdout" "$lower_stderr" lower --project "$project" --surface c --mode witness --plan "$plan" --out "$out" --json --quiet
status=$?
set -e

if [ "$status" -eq 0 ]; then
  say "Unexpectedly minted witness proof:"
  cat "$lower_stdout"
  exit 1
fi

section "Full Lower Result JSON With Line Numbers"
show_json_file "$lower_stdout"

section "Relevant Bit: Raw Lowered C Emitter Line"
highlight_raw_line "$lower_stdout" '"source": "#include <stdbool.h>\n#include <stdint.h>\n#include <stdio.h>\n\n#include \"artifacts/software/checked_add_u8.c\"' "The lower result contains the generated C emitter as evidence."
highlight_raw_line "$lower_stdout" '"message": "counterexample: a=1 b=255\nexpected: overflow=true value=0\nobserved: overflow=false value=0",' "The generated C witness found the concrete failing input."

section "Relevant Bit: Red Counterexample"
highlight_raw_line "$lower_stdout" '"error": "counterexample: a=1 b=255\nexpected: overflow=true value=0\nobserved: overflow=false value=0",' "ProvekIt returns red because no witness .proof can be minted."

analysis_with_receipts <<'TEXT'
The generated-source line is the receipt that lower really emitted a C witness program. The message and error lines are the receipts that the generated witness ran and found a concrete counterexample: a=1, b=255.

That proves the initial claim for this phase. ProvekIt is not guessing that the implementation is wrong. It lowered the ProofIR witness demand into C-domain evidence, ran it, and refused to mint a witness proof because observed overflow=false contradicts expected overflow=true.
TEXT

section "No Checked-In Generated Witness C"
say "The generated C harness is runtime evidence. This search should print nothing:"
find "$EXHIBIT_ROOT" -path '*/target/*' -prune -o -name 'checked_add_witness.c' -print

section "Main Mint Fails For The Same Reason"
explain_then_pause "rerun mint through the full chain" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is now running the full mint path on the same broken software artifact. This repeats the composed flow: lift the project, see the witness demand, invoke lower, and refuse to mint the main proof when the witness cannot be produced.

Value ProvekIt adds:
This connects the explicit lower failure back to the user-facing mint command. The lower result is not a separate diagnostic universe. It is the witness protocol that mint depends on. If lower cannot emit a witness .proof, mint cannot honestly produce the main proof DAG.

Relationship to the chain:
The bridge chain reaches the software boundary, but the final contract member cannot acquire the witness input CID it needs. That means the root proof CID for the whole stack cannot be minted for this mutated artifact.

What to look for:
After the prompt, the mutation diff appears again, then mint refuses with ORP witness failed: checked_add_u8.postcondition and the same counterexample.
TEXT
run_mutation_expect_refusal \
  "software-counterfeit-contract" \
  "mutations/software/counterfeit-contract/checked_add_u8.c" \
  "artifacts/software/checked_add_u8.c" \
  "checked_add_u8.postcondition"

analysis_with_receipts <<'TEXT'
The second mutation diff is the receipt that mint saw the same broken C behavior. The refusal evidence is the receipt that the composed mint path failed for the same reason as the explicit lower command: ORP witness failed for checked_add_u8.postcondition, with the same counterexample.

That proves lower is not an isolated diagnostic. It is the witness protocol mint depends on. If lower cannot produce the witness .proof, the main proof DAG cannot get the witness input CID and the Bridgeworks root proof cannot be minted.
TEXT

section "Point"
say "Same lifted identity, false behavior, no witness .proof, no bridge chain."

next_script "16-run-whole-exhibit.sh"
