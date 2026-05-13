#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The Device Physics Layer"
say "We move the MOSFET paper outside the accepted physical envelope."
explain_then_pause "mutate the physics artifact and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is minting the chain after the device-physics artifact has been moved outside the accepted physical envelope. The artifact is a paper-like native document, not code, but the lifter still treats its claim as a contract boundary.

Value ProvekIt adds:
This is where Bridgeworks stops being a software-only story. ProvekIt lets a physics-domain claim participate in the same proof chain as C software without forcing the paper into a software manifest format. The lifter knows how to read the native marker and validate the domain-specific contract.

Relationship to the chain:
The physics claim is the bridge between measured material behavior and cell-model validity. If device_physics.mosfet_switch.valid is false, the cell abstraction has lost its physical support, so gates, RTL, ISA, compiler, and software cannot inherit a complete chain.

What to look for:
After the prompt, the diff should show the physics artifact leaving the accepted envelope. The refusal should name device_physics.mosfet_switch.valid rather than a downstream software symptom.
TEXT
run_mutation_expect_refusal \
  "device-physics-outside-envelope" \
  "mutations/device-physics/mosfet-switch-paper_outside_envelope.md" \
  "artifacts/device-physics/mosfet-switch-paper.md" \
  "device_physics.mosfet_switch.valid"

analysis_with_receipts <<'TEXT'
The mutation diff is the receipt for what changed in the native physics artifact. The refusal evidence is the receipt for where ProvekIt stopped the proof: device_physics.mosfet_switch.valid.

That proves the point of this break. A physics paper can be a contract-bearing artifact in the same chain as software, and ProvekIt can refuse the proof at that paper boundary before any downstream layer gets to inherit it.
TEXT

section "Point"
say "The paper is not a JSON file, but it still carries a boundary contract."

next_script "09-break-cells.sh"
