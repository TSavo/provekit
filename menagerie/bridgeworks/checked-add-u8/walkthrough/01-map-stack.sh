#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "The Whole Stack"
say "Bridgeworks starts below software and climbs upward. The C program is the final boundary, not the whole story."

explain_then_pause "show the native contract stack" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is not invoked in this script. That is deliberate. This step gives the human reader the contract map before any machine output appears, so the later JSON and .proof members can be read as evidence for a chain we already understand.

Value ProvekIt adds:
The value ProvekIt will add in the later steps is that this map stops being prose. ProvekIt will lift each native artifact into a ProofIR claim, record explicit implication edges between neighboring claims, require a witness where behavior must be checked, and mint the result into content-addressed proof artifacts.

Relationship to the chain:
Each line is a contract boundary in its native domain. The experiment supports the physics claim; the physics claim supports the cell abstraction; the cells support gates; gates support RTL; RTL supports ISA semantics; ISA semantics support compiler lowering; compiler lowering supports the C postcondition. The shape is p -> q all the way up. If any p is false, or if any bridge p -> q is false, the final software contract cannot honestly inherit the chain.

What to look for:
After the prompt, read the stack as dependency order, not just display order. The C source is the last boundary. It is downstream from every earlier contract.
TEXT

cat <<'TEXT'
experiment.material_parameters.within_tolerance
  -> device_physics.mosfet_switch.valid
  -> cells.boolean_gates.valid_in_envelope
  -> gates.full_adder.equations
  -> rtl.alu.refines_add8
  -> isa.add8.carry_semantics
  -> compiler.lowering.preserves_checked_add
  -> checked_add_u8.postcondition
TEXT

section "Native Artifacts"
printf '%-18s %s\n' "experiment" "artifacts/experiment/bandgap-measurements.csv"
printf '%-18s %s\n' "physics" "artifacts/device-physics/mosfet-switch-paper.md"
printf '%-18s %s\n' "cells" "artifacts/cells/cells.sp"
printf '%-18s %s\n' "gates" "artifacts/gates/full_adder.blif"
printf '%-18s %s\n' "rtl" "artifacts/rtl/alu.v"
printf '%-18s %s\n' "isa" "artifacts/isa/toy8.isa"
printf '%-18s %s\n' "compiler" "artifacts/compiler/lowering.trace + artifacts/compiler/toy8.asm"
printf '%-18s %s\n' "software" "artifacts/software/checked_add_u8.c"

analysis_with_receipts <<'TEXT'
The receipt is the map itself. It shows eight native contracts in dependency order, not a flat list of files. The experiment contract is first because every later claim relies on the physical/material premise. The software postcondition is last because it is the top of the inheritance chain, not the whole chain.

The artifact table is the second receipt. Each contract belongs to a different native form: CSV, paper, SPICE, BLIF, Verilog, ISA text, compiler trace, and C. The value ProvekIt adds in the later scripts is that these heterogeneous artifacts can all be lifted into one ProofIR contract graph without pretending they share one source language.
TEXT

section "Point"
say "Every artifact is native to its domain. ProofIR is the shared contract language that carries the boundary claims."

next_script "02-show-native-contracts.sh"
