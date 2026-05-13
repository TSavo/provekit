# MOSFET Switch Abstraction Red Mutation

<!-- bridgeworks:claim device_physics.mosfet_switch.valid -->
<!-- bridgeworks:mutation expected_refusal=device_physics_parameters_outside_envelope -->

This mutated paper keeps the claim marker but moves the parameter set outside
the accepted Bridgeworks envelope.

| parameter | nominal | tolerance |
| --- | ---: | ---: |
| nmos_vth_v | 0.55 | +/- 0.03 |
| pmos_abs_vth_v | 0.61 | +/- 0.03 |
| oxide_thickness_nm | 5.20 | +/- 0.20 |
| bandgap_ev | 1.21 | +/- 0.02 |

The values above are intentionally outside the measurement-backed parameter set
`P`; the toy paper lifter must refuse this artifact.
