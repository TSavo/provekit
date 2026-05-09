#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break Software Contract Identity"
say "Now we enter software. This mutation advertises the wrong contract: overflow_add_u8.postcondition."
explain_then_pause "mutate the software contract identity and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the C source has been replaced with a file that advertises the wrong contract identity. The command will not get to a meaningful behavioral witness because the lifted identity no longer matches the bridge edge.

Value ProvekIt adds:
ProvekIt distinguishes "this artifact claims the wrong contract" from "this artifact claims the right contract but implements it incorrectly." That distinction is crucial. A witness can only discharge a contract after identity has established which contract is being claimed.

Relationship to the chain:
The compiler-to-software edge expects checked_add_u8.postcondition as its consequent. The Bridgeworks software validator only emits that boundary contract when the native artifact advertises the checked-add marker and entrypoint. This mutation advertises overflow_add_u8.postcondition instead, so the software boundary refuses before a bridge edge can land.

What to look for:
After the prompt, the C diff should show both the wrong marker and the wrong function contract. The refusal should explain that checked_add_u8.postcondition was refused because the checked-add contract marker is missing.
TEXT
run_mutation_expect_refusal \
  "software-overflow-add-u8" \
  "mutations/software/overflow-add-u8/overflow_add_u8.c" \
  "artifacts/software/checked_add_u8.c" \
  "checked_add_u8.postcondition"

analysis_with_receipts <<'TEXT'
The diff is the receipt that the software artifact changed its contract identity. The refusal evidence is the receipt that ProvekIt needed checked_add_u8.postcondition but the Bridgeworks software validator refused to emit it.

That proves this is an identity failure, not a value-witness failure. ProvekIt refuses before lower because the software boundary did not claim the contract name the chain requires.
TEXT

section "Point"
say "This is still lift identity. The artifact did not even claim the boundary contract the bridge needs."

next_script "15-break-software-witness.sh"
