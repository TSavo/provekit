#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
lift_json="$tmp/lift.json"
lift_stderr="$tmp/lift.stderr"

section "Lift The Whole Exhibit"
say "ProvekIt reads .provekit/config.toml, delegates to the configured lifter, and emits one ProofIR document."
explain_then_pause "run full ProofIR lift" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is doing the full lift. It reads the same project configuration, delegates to the Bridgeworks lifter, and receives a ProofIR-shaped document that contains contract claims, authorities, bridge implications, and witness requirements.

Value ProvekIt adds:
The paper is not JSON. The SPICE file is not C. The ISA spec is not TOML. ProofIR is the shared first-order contract language that carries the claims across those native forms without pretending the native artifacts are the same kind of thing. ProvekIt gives the chain one language for contracts while letting each artifact remain native to its own domain.

Relationship to the chain:
Lift makes the chain visible, but it does not discharge every obligation. The C postcondition appears as a contract claim, and the compiler-to-software bridge appears as an implication, but the software behavior still needs a witness. That separation is important: identity and structure can be lifted before runtime behavior is accepted.

What to look for:
After the prompt, the full JSON should identify itself as an ir-document. The highlighted lines should show checked_add_u8.postcondition as a ProofIR contract and should show a witness demand attached to that same contract through surface c.
TEXT
print_provekit lift "menagerie/bridgeworks/checked-add-u8" --json --quiet
run_provekit_capture "$lift_json" "$lift_stderr" lift "menagerie/bridgeworks/checked-add-u8" --json --quiet

section "Full Lifted ProofIR JSON With Line Numbers"
show_json_file "$lift_json"

section "Relevant Bit: Raw Contract Lines"
highlight_raw_line "$lift_json" '"kind": "ir-document",' "This is the full lifted ProofIR document emitted by ProvekIt."
highlight_raw_line "$lift_json" '"name": "checked_add_u8.postcondition",' "The software contract is now a ProofIR contract claim."

section "Relevant Bit: Witness Demands"
say "Lift still does not prove software behavior. It can demand a lower-witness step."
highlight_raw_line "$lift_json" '"attachTo": "checked_add_u8.postcondition",' "The lift output demands a witness for that contract."
highlight_raw_line "$lift_json" '"surface": "c"' "The demanded witness is delegated to the C lowerer."

analysis_with_receipts <<'TEXT'
The line-numbered JSON is the receipt for the full lift. The ir-document line proves ProvekIt is no longer only identifying native markers; it has emitted the shared ProofIR document. The checked_add_u8.postcondition lines prove the software boundary has become a ProofIR contract claim.

The witness lines prove the other half of the claim: lift did not silently accept the C behavior. It demanded a witness attached to the software contract and routed that demand to surface c. That is the relationship ProvekIt adds here: it separates "I can see the contract" from "I have behavioral evidence for the contract."
TEXT

section "Point"
say "Lift carries identity and boundary claims into ProofIR. Witnessing is a separate lower step."

next_script "04-show-bridge-edges.sh"
