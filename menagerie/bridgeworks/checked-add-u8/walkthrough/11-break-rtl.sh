#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The RTL Layer"
say "We change carry extraction in the Verilog ALU."
explain_then_pause "mutate the RTL and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the Verilog RTL has been changed. The mutation damages the carry extraction in the ALU, so the RTL no longer refines unsigned 8-bit addition in the way the ISA layer expects.

Value ProvekIt adds:
ProvekIt turns RTL refinement into a first-class contract boundary. That matters because RTL is neither the software source nor the CPU paper; it is its own native domain. The proof chain should fail at the RTL contract instead of pretending the downstream ISA and compiler claims can still inherit from it.

Relationship to the chain:
RTL supports ISA semantics. If rtl.alu.refines_add8 is false, the ISA add8 carry semantics no longer have a valid circuit-level basis. The compiler and software postcondition are downstream of that break.

What to look for:
After the prompt, the diff should show the carry-bit mutation. The refusal should name rtl.alu.refines_add8.
TEXT
run_mutation_expect_refusal \
  "rtl-wrong-carry-bit" \
  "mutations/rtl/alu_wrong_carry.v" \
  "artifacts/rtl/alu.v" \
  "rtl.alu.refines_add8"

analysis_with_receipts <<'TEXT'
The diff shows the Verilog carry mutation. The refusal evidence names rtl.alu.refines_add8, which is the contract connecting the gate-level implementation to ISA-visible add8 behavior.

Those receipts prove the RTL layer is an active contract boundary. The proof cannot keep climbing into ISA and compiler claims after the circuit refinement premise fails.
TEXT

section "Point"
say "The RTL contract says the circuit refines unsigned 8-bit addition with carry."

next_script "12-break-isa.sh"
