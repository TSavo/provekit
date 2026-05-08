* Bridgeworks red mutation: omitted noise margin.
* bridgeworks:claim cells.boolean_gates.valid_in_envelope
* bridgeworks:mutation expected_refusal=cells_omitted_noise_margin

.param VDD_MIN=1.71
.param VDD_NOM=1.80
.param VDD_MAX=1.89
.param TEMP_MIN_C=-20
.param TEMP_MAX_C=85
.param MAX_FANOUT=4

.subckt INV in out vdd vss
M_P out in vdd vdd PMOS W=2u L=180n
M_N out in vss vss NMOS W=1u L=180n
.ends INV

.model NMOS NMOS LEVEL=1 VTO=0.43 KP=120u GAMMA=0.40 LAMBDA=0.04
.model PMOS PMOS LEVEL=1 VTO=-0.45 KP=55u GAMMA=0.38 LAMBDA=0.05
