# MOSFET Switch Abstraction For Bridgeworks Checked Add

<!-- bridgeworks:claim device_physics.mosfet_switch.valid -->
<!-- bridgeworks:requires experiment.material_parameters.within_tolerance -->
<!-- bridgeworks:projection paper-claim-lifter/v0 -->

## Claim Block

Native claim id: `device_physics.mosfet_switch.valid`

For the Bridgeworks toy 180 nm CMOS envelope, the MOSFET devices may be
abstracted as Boolean-controlled switches for static logic evaluation when the
following parameter set `P` and operating envelope `E` are both satisfied.

Parameter set `P`:

| parameter | nominal | tolerance |
| --- | ---: | ---: |
| nmos_vth_v | 0.43 | +/- 0.03 |
| pmos_abs_vth_v | 0.45 | +/- 0.03 |
| oxide_thickness_nm | 4.10 | +/- 0.20 |
| bandgap_ev | 1.12 | +/- 0.02 |

Envelope `E`:

| parameter | accepted range |
| --- | --- |
| vdd_v | 1.71 to 1.89 |
| temperature_c | -20 to 85 |
| required_noise_margin_v | >= 0.22 |
| fanout | <= 4 |

Under `P` and `E`, an enabled pull-up or pull-down network settles to a valid
logic level before the checked-add-u8 full-adder observation point. The claim is
limited to static Boolean gates used in this exhibit and is not a claim about
timing closure, aging, or arbitrary analog behavior.

Trusted stop: compact-model adequacy for this toy educational envelope.
