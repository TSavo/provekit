#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The Compiler Layer"
say "We let machine code ignore carry, so the compiler no longer preserves checked addition."
explain_then_pause "mutate the compiler assembly and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the compiler's emitted assembly has been changed. The source file can still claim the right software contract, and the compiler trace can still describe the right intent, but the machine-level artifact no longer branches on carry.

Value ProvekIt adds:
This is one of the clearest Bridgeworks moments. Software correctness depends on more than source text. ProvekIt makes the compiler contract explicit, then refuses the proof when that contract stops supporting the software postcondition.

Relationship to the chain:
The compiler contract is the final non-software bridge before C. If compiler.lowering.preserves_checked_add fails, the edge into checked_add_u8.postcondition has no valid antecedent, even if the C marker still advertises the right name.

What to look for:
After the prompt, the diff should show the raw assembly mutation where `BR_CARRY overflow` disappears. The refusal should name compiler.lowering.preserves_checked_add.
TEXT
run_mutation_expect_refusal \
  "compiler-ignores-carry" \
  "mutations/compiler/toy8.asm" \
  "artifacts/compiler/toy8.asm" \
  "compiler.lowering.preserves_checked_add"

analysis_with_receipts <<'TEXT'
The diff shows the compiler assembly mutation: ADD8 remains, but the carry branch that separates ok from overflow is gone. The refusal evidence names compiler.lowering.preserves_checked_add, the last non-software contract before the C postcondition.

Those receipts prove the software proof depends on compiler behavior. The source can still contain the right marker, but ProvekIt refuses the chain if the compiler bridge no longer supports that marker.
TEXT

section "Point"
say "The compiler contract is the bridge from machine carry semantics to the software postcondition."

next_script "14-break-software-identity.sh"
