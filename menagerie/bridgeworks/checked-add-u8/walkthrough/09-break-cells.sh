#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The Cell Model Layer"
say "We omit the noise margin that makes the boolean gate abstraction valid."
explain_then_pause "mutate the cell model and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the SPICE cell artifact has been changed. The cell layer is where physical transistor behavior becomes a usable boolean abstraction, so the contract here is about whether that abstraction is valid in its envelope.

Value ProvekIt adds:
ProvekIt gives this analog-to-digital boundary a place in the same proof graph as the software postcondition. The cell model does not have to become source code. It stays SPICE, but its contract can still be lifted, checked, and connected to the rest of the implication DAG.

Relationship to the chain:
Cells support gates. If cells.boolean_gates.valid_in_envelope is false, then the BLIF full-adder equations no longer have a justified physical interpretation. The chain must fail before RTL or software can claim inherited correctness.

What to look for:
After the prompt, the diff should show the missing or damaged cell-model condition. The refusal should name cells.boolean_gates.valid_in_envelope.
TEXT
run_mutation_expect_refusal \
  "cells-omitted-noise-margin" \
  "mutations/cells/cells_omitted_noise_margin.sp" \
  "artifacts/cells/cells.sp" \
  "cells.boolean_gates.valid_in_envelope"

analysis_with_receipts <<'TEXT'
The diff shows the cell-model damage. The refusal evidence names cells.boolean_gates.valid_in_envelope, which is the exact contract that lets transistor behavior be used as boolean gate behavior.

Those receipts prove ProvekIt is preserving the boundary between physics/cells and logic. A later gate or RTL claim cannot erase the missing cell-level premise.
TEXT

section "Point"
say "The SPICE model is the bridge from physical transistor behavior to boolean gates."

next_script "10-break-gates.sh"
