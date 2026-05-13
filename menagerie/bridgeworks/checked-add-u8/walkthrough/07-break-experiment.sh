#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Break The Experiment Layer"
say "We change calibrated measurement data without the matching signed calibration note."
explain_then_pause "mutate experiment data and run mint" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is going to mint the same chain after the measured experiment data has been changed. The command path is the same as the positive case: lift, lower as needed, and mint. The difference is that one native artifact at the bottom of the stack no longer satisfies the contract it claims.

Value ProvekIt adds:
The value is that the failure is caught at the contract boundary where it belongs. This is not a software test failing at the top after the real cause has been blurred. ProvekIt refuses the chain because the experimental measurement contract cannot support the physics layer anymore.

Relationship to the chain:
The experiment contract is the root support for the device-physics claim. If experiment.material_parameters.within_tolerance is false, then device_physics.mosfet_switch.valid cannot inherit it honestly, and nothing above it should mint as a complete bridge chain.

What to look for:
After the prompt, the raw diff should show the measurement change. The refusal should mention experiment.material_parameters.within_tolerance, proving the chain failed at the measured-material boundary.
TEXT
run_mutation_expect_refusal \
  "experiment-measurement-changed-without-signature" \
  "mutations/experiment/bandgap_measurement_changed_without_signature.csv" \
  "artifacts/experiment/bandgap-measurements.csv" \
  "experiment.material_parameters.within_tolerance"

analysis_with_receipts <<'TEXT'
The mutation diff is the first receipt: it shows the exact measurement artifact that changed. The refusal evidence is the second receipt: ProvekIt rejected the chain at experiment.material_parameters.within_tolerance.

That proves the initial claim for this break. A bad material measurement is not allowed to flow upward and become a believable software proof. The chain fails at the bottom boundary where the premise was damaged.
TEXT

section "Point"
say "The stack fails at the measured-material boundary before physics, cells, gates, or software can matter."

next_script "08-break-device-physics.sh"
