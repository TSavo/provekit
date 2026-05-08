# Bridgeworks Bandgap Measurement Calibration Note

<!-- bridgeworks:claim experiment.material_parameters.within_tolerance -->
<!-- bridgeworks:calibration_signature BW-CAL-2026-05-08-SHA256-4b57c1d8 -->
<!-- bridgeworks:projection calibration-note-lifter/v0 -->

Instrument set:

- SMU: BW-SMU-02, calibration date 2026-05-01.
- Thermal chamber: BW-TC-01, calibration date 2026-05-01.
- Ellipsometer: BW-EL-03, calibration date 2026-05-02.

Acceptance policy:

- `bandgap_ev` must remain in 1.10 to 1.14 eV.
- `nmos_vth_v` must remain in 0.40 to 0.46 V.
- `pmos_abs_vth_v` must remain in 0.42 to 0.48 V.
- `oxide_thickness_nm` must remain in 3.90 to 4.30 nm.

The CSV named `bandgap-measurements.csv` is accepted only when it carries the
same calibration signature as this note. Changing measurements without changing
the signature is a refusal case for the toy measurement lifter.
