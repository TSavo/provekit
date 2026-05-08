# Bridgeworks

Bridgeworks is the planned Menagerie destination for claim transport across
true domain crossings.

Its core claim is that a software contract can be supported by lower-domain
contracts without collapsing every artifact into one representation. The paper
is still a paper. The CPU is still a CPU-shaped artifact. ProvekIt preserves the
portable part: `artifact I -> projection k -> claim t -> signed edge graph`.

The first planned exhibit is checked 8-bit addition:

```text
checked_add_u8 postcondition
  <- compiler lowering preservation
  <- ADD8 ISA carry semantics
  <- ALU RTL refinement
  <- full-adder gate equations
  <- Boolean cell envelope
  <- MOSFET switch abstraction
  <- measured material parameters
```

Each layer is intended to be built by an independent white-room agent from a
contract sheet. The agent receives the native artifact format, adjacent boundary
claims, and required refusal cases, then builds the smallest native artifact
that fulfills the real contract for that domain. The destination succeeds only
when the projected claims compose into one compressed receipt.

Design spec:

- [Bridgeworks White-Room Contract Stack Design](../../docs/superpowers/specs/2026-05-08-bridgeworks-white-room-contract-stack-design.md)
