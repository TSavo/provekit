#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
lift_json="$tmp/lift.json"
lift_stderr="$tmp/lift.stderr"
plan="$tmp/runtime-no-env-plan.json"
lower_json="$tmp/lower.json"
lower_stderr="$tmp/lower.stderr"
status=0

section "Preserved Contract Fails Witness"
explain_then_pause "lift the poisoned release and lower the preserved contract" <<'TEXT'
This is the central "oh shit" moment. The 1.4.2 package keeps the baseline contract set. That means the maintainer is explicitly still claiming runtime.no-env-secret-read. ProvekIt does not block the claim at the text layer. It lifts the claim and then demands a witness.

Lowering is where the claim becomes evidence. The JavaScript lowerer receives the ProofIR obligation for runtime.no-env-secret-read and inspects the host artifact. The package contains a rare-path process.env.SAFE_JSON_TOKEN read, so the lowerer cannot mint a witness .proof.

What to look for: lift still shows the baseline contractSetCid, says the contract source is provekit-lift-ts, and carries a witness attached to runtime.no-env-secret-read. Lower then returns status rejected with reasonCode env-secret-read and the concrete counterexample text.
TEXT

print_provekit lift "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet
run_provekit_capture "$lift_json" "$lift_stderr" lift "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet
jq '.witnesses[] | select(.attachTo=="runtime.no-env-secret-read")' "$lift_json" > "$plan"

section "Lift JSON"
show_json_file "$lift_json"
highlight_raw_line "$lift_json" '"contractSetCid": "blake3-512:274b2fc4' "The poisoned release preserves the baseline contract set."
highlight_raw_line "$lift_json" '"lifter": "provekit-lift-ts"' "The preserved contract set came from the TypeScript lifter."
highlight_raw_line "$lift_json" '"attachTo": "runtime.no-env-secret-read"' "Lift demands a witness for the preserved runtime contract."

print_provekit lower --project "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --surface javascript --mode witness --plan "$plan" --json --quiet
set +e
run_provekit_capture "$lower_json" "$lower_stderr" lower --project "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --surface javascript --mode witness --plan "$plan" --json --quiet
status=$?
set -e
if [ "$status" -eq 0 ]; then
  say "Unexpectedly accepted poisoned witness."
  show_json_file "$lower_json"
  exit 1
fi

section "Lower Refusal JSON"
show_json_file "$lower_json"
highlight_raw_line "$lower_json" '"status": "rejected"' "The lowerer refused to mint a witness .proof."
highlight_raw_line "$lower_json" '"reasonCode": "env-secret-read"' "The violated rail is the preserved no-env-secret-read contract."
highlight_raw_line "$lower_json" 'process.env.SAFE_JSON_TOKEN' "The counterexample names the forbidden secret read."
highlight_raw_line "$lower_json" '"source": "scan(index.js) => reject' "The lower result carries the emitted witness evidence description."

analysis_with_receipts <<'TEXT'
The lift lines prove the maintainer made the old claim. The lower lines prove ProvekIt forced the claim into evidence and rejected it. The package was not rejected because the signer was fake or the provenance was missing. It was rejected because the preserved contract could not produce a witness.

That is the supply-chain primitive in action: claim first, evidence required, red receipt when evidence cannot be produced.
TEXT

next_script "05-weaken-contracts-fail-version.sh"
