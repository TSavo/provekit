#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The ISA Layer"
say "We swap unsigned carry semantics for signed overflow semantics."
explain_then_pause "mutate the ISA semantics and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the ISA specification has been changed from unsigned carry semantics to signed overflow semantics. The syntax may still look plausible, but it no longer states the contract the compiler lowering depends on.

Value ProvekIt adds:
The value is semantic continuity. ProvekIt does not let the compiler bridge quietly switch meanings between "carry" and "signed overflow." The ISA contract is explicit, lifted, and checked as its own boundary.

Relationship to the chain:
ISA semantics support compiler lowering. If isa.add8.carry_semantics is false or replaced by a different meaning, then compiler.lowering.preserves_checked_add cannot claim it preserved the machine behavior needed by the C postcondition.

What to look for:
After the prompt, the diff should show the ISA semantic change. The refusal should name isa.add8.carry_semantics.
TEXT
run_mutation_expect_refusal \
  "isa-signed-overflow" \
  "mutations/isa/toy8.isa" \
  "artifacts/isa/toy8.isa" \
  "isa.add8.carry_semantics"

analysis_with_receipts <<'TEXT'
The diff shows the ISA semantic change. The refusal evidence names isa.add8.carry_semantics, which is the CPU contract the compiler lowering depends on.

That proves ProvekIt is preserving semantic meaning across the CPU/compiler boundary. Signed overflow and unsigned carry are not interchangeable just because both are arithmetic-looking claims.
TEXT

section "Point"
say "The CPU contract must mean the same thing the RTL proved."

next_script "13-break-compiler.sh"
