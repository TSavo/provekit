#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The Gate Equations"
say "We replace an XOR truth table with OR behavior in the full-adder gate artifact."
explain_then_pause "mutate the gate equations and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the gate-level artifact has been changed. The broken artifact replaces XOR behavior with OR behavior in the full-adder logic, so the gate equation contract no longer describes the circuit the upper layers need.

Value ProvekIt adds:
The value here is precise failure localization across representation boundaries. A wrong gate equation could eventually appear as a wrong software result, but ProvekIt refuses at the gate contract before that mistake is laundered through RTL, ISA, and compiler layers.

Relationship to the chain:
Gate equations are the bridge from valid cells to RTL behavior. If gates.full_adder.equations is false, then rtl.alu.refines_add8 cannot honestly inherit the gate-level support it needs.

What to look for:
After the prompt, the diff should show the full-adder logic mutation. The refusal should name gates.full_adder.equations.
TEXT
run_mutation_expect_refusal \
  "gates-xor-replaced-by-or" \
  "mutations/gates/full_adder_xor_replaced_by_or.blif" \
  "artifacts/gates/full_adder.blif" \
  "gates.full_adder.equations"

analysis_with_receipts <<'TEXT'
The diff is the receipt that the full-adder equations changed. The refusal evidence is the receipt that ProvekIt rejected gates.full_adder.equations.

That proves the chain is using the gate contract as a real support for RTL, not as decoration. If XOR becomes OR, the proof stops at the gate layer instead of letting the wrong logic travel upward.
TEXT

section "Point"
say "Gate equations are a contract boundary between cells and RTL."

next_script "11-break-rtl.sh"
