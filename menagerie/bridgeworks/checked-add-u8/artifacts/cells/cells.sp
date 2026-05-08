* Bridgeworks checked-add-u8 cell envelope native artifact.
* bridgeworks:claim cells.boolean_gates.valid_in_envelope
* bridgeworks:requires device_physics.mosfet_switch.valid
* bridgeworks:projection spice-envelope-lifter/v0
*
* Toy-lifter-visible envelope E:
*   vdd_min=1.71 vdd_nom=1.80 vdd_max=1.89
*   temp_min_c=-20 temp_max_c=85
*   noise_margin_high_min_v=0.22
*   noise_margin_low_min_v=0.22
*   max_fanout=4

.param VDD_MIN=1.71
.param VDD_NOM=1.80
.param VDD_MAX=1.89
.param TEMP_MIN_C=-20
.param TEMP_MAX_C=85
.param NOISE_MARGIN_HIGH_MIN=0.22
.param NOISE_MARGIN_LOW_MIN=0.22
.param MAX_FANOUT=4

.subckt INV in out vdd vss
M_P out in vdd vdd PMOS W=2u L=180n
M_N out in vss vss NMOS W=1u L=180n
.ends INV

.subckt NAND2 a b out vdd vss
M_P1 out a vdd vdd PMOS W=2u L=180n
M_P2 out b vdd vdd PMOS W=2u L=180n
M_N1 out a n1  vss NMOS W=2u L=180n
M_N2 n1  b vss vss NMOS W=2u L=180n
.ends NAND2

.subckt NOR2 a b out vdd vss
M_P1 out a p1  vdd PMOS W=4u L=180n
M_P2 p1  b vdd vdd PMOS W=4u L=180n
M_N1 out a vss vss NMOS W=1u L=180n
M_N2 out b vss vss NMOS W=1u L=180n
.ends NOR2

.model NMOS NMOS LEVEL=1 VTO=0.43 KP=120u GAMMA=0.40 LAMBDA=0.04
.model PMOS PMOS LEVEL=1 VTO=-0.45 KP=55u GAMMA=0.38 LAMBDA=0.05
