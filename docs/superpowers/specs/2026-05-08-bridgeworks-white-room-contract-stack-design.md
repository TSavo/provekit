# Bridgeworks White-Room Contract Stack Design

## Purpose

Bridgeworks should be the Menagerie destination that makes the Sugar
primitive feel larger than software verification. Bug Zoo shows real software
bugs as missing edges. Bridgeworks should show a claim crossing epistemic
domains without becoming unverified translation:

```text
artifact I -> projection k -> claim t -> signed edge graph
```

The exhibit should produce the "oh, this is the same primitive all the way
down" moment. A software postcondition is witnessed by a compiler-lowering
contract, enforced by an ISA and CPU contract, reduced to gate and cell
contracts, justified by a device-physics claim, and anchored in an experiment.

## Core Example

Use checked 8-bit addition as the first Bridgeworks exhibit.

The user-facing claim is:

```text
checked_add_u8(a, b) returns ok(sum = a + b) iff a + b < 256,
otherwise it returns overflow.
```

That claim is small enough to make every layer concrete, but it is not a toy
contract. Every real machine that exposes checked integer addition relies on
the same chain of obligations: carry semantics, lowering preservation, ALU
refinement, gate behavior, cell behavior, and device assumptions.

## The Contract Chain

Every layer keeps its native artifact. The paper is not JSON. The CPU is not
TOML. Sugar receives native artifacts through lifters and records only the
projected contract claims and bridge edges.

| Layer | Native artifact | Projection `k(I)` | Claim `t` |
| --- | --- | --- | --- |
| Software contract | `checked_add_u8.c` | Existing C lifter path | `checked_add_u8(a,b)` returns `ok(sum)` iff `a+b < 256`; otherwise `overflow`. |
| Compiler lowering | `lowering.trace` plus toy assembly | Lowering-trace lifter | Source checked-add lowers to `ADD8` plus branch-on-carry without changing the contract. |
| ISA | `toy8.isa` | ISA lifter | `ADD8(a,b)` yields `sum=(a+b) mod 256` and `carry=(a+b >= 256)`. |
| CPU/RTL | `alu.v` | RTL lifter | The ALU output wires implement the ISA `sum` and `carry` relation for all 8-bit inputs. |
| Gates | `full_adder.blif` or `full_adder.aig` | Netlist lifter | The ripple-carry network implements the full-adder equations. |
| Cells | `cells.sp` | Cell-envelope lifter | The Boolean gates are valid under voltage, temperature, and noise envelope `E`. |
| Device physics | `mosfet-switch-paper.md` or `.tex` | Paper-claim lifter | The MOSFET switch abstraction is valid under parameter set `P` and envelope `E`. |
| Experiment | `bandgap-measurements.csv` plus calibration note | Measurement lifter | The measured material parameters fall within `P` under stated tolerance and signer policy. |

Read upward, the compressed route is:

```text
measured material parameters
  -> MOSFET switch abstraction
  -> Boolean cell envelope
  -> full-adder gate equations
  -> ALU RTL refinement
  -> ADD8 ISA carry semantics
  -> compiler lowering preservation
  -> checked_add_u8 postcondition
```

Each line has the same shape: `prove k(I)=t`. Each adjacent bridge signs a
claim that the lower-domain predicate is sufficient for the upper-domain
predicate under the exhibit policy.

## White-Room Build Method

Bridgeworks should be built by parallel agents as a design-by-contract exercise,
not as one author hand-crafting a stack with shared hidden assumptions.

The coordinator first writes contract sheets for every layer:

- the native artifact format the layer must produce;
- the exact input assumptions it may rely on;
- the exact output claim it must satisfy;
- the lifter-visible boundary fields;
- required positive and refusal cases.

Then one agent owns each layer. An agent sees its own contract sheet and the
public claims at adjacent boundaries, but not sibling implementations. Its job
is to build the smallest native artifact that fulfills the real contract for
that layer. The agent returns:

- the native artifact;
- a local self-check, when the domain has one;
- a projection fixture proving what the lifter should read;
- a short statement of assumptions and trusted stops.

This matters because Bridgeworks is supposed to demonstrate contract transport,
not coordinated string matching. If the stack closes after independent builders
work only through contracts, the exhibit has shown the primitive in action.

## Runner Shape

The first executable Bridgeworks runner should invoke the Rust `provekit` CLI to
run the lift-plugin surface and mint `.proof` output. Bridgeworks owns exhibit
orchestration and fixtures; the CLI owns ProofIR validation, memento minting,
implication mementos, and deterministic `.proof` bytes.

The first runner should:

1. load the exhibit manifest;
2. call `provekit mint` for the exhibit's lift surface;
3. call `provekit prove` against the emitted project proof graph;
4. compare the observed claim and implication receipts with expected fixtures;
5. emit a compressed top-level receipt for the checked-add postcondition;
6. run mutation cases that fail closed when any layer weakens or drops an
   obligation.

The runner should make the composition visible. It should not merely report
"pass". The useful output is the chain of claims and edges that shows how a
software postcondition is supported by lower-domain contracts.

## Refusal Cases

Bridgeworks needs red cases from the beginning. At minimum:

- software drops the overflow branch;
- lowering ignores carry;
- ISA defines carry as signed overflow instead of unsigned carry;
- RTL returns the right sum but wrong carry;
- netlist swaps an XOR for OR;
- cell envelope omits the noise-margin assumption;
- device-physics claim uses parameters outside the measurement tolerance;
- measurement file is changed without updating the signed calibration claim.

Each refusal should identify the missing or invalid edge, not just a file diff.

## Scope

The first exhibit is a small, honest vertical stack. It does not claim to verify
real commercial silicon, a real compiler, or real semiconductor physics end to
end. It claims something narrower and more important for the Menagerie:

- native artifacts can remain native;
- contracts can be lifted out of each domain;
- edges can compress the verification surface;
- independent builders can satisfy adjacent contracts without whole-stack
  coordination;
- the final software claim can be checked as a content-addressed edge graph.

## Repository Shape

Expected initial layout:

```text
menagerie/bridgeworks/
  README.md
  checked-add-u8/
    manifest.yaml
    contracts/
      software.yaml
      lowering.yaml
      isa.yaml
      rtl.yaml
      gates.yaml
      cells.yaml
      device-physics.yaml
      experiment.yaml
    artifacts/
      software/checked_add_u8.c
      compiler/lowering.trace
      compiler/toy8.asm
      isa/toy8.isa
      rtl/alu.v
      gates/full_adder.blif
      cells/cells.sp
      device-physics/mosfet-switch-paper.md
      experiment/bandgap-measurements.csv
      experiment/calibration-note.md
    mutations/
    expected/
```

The `contracts/` files are scaffolding for the white-room builders and runner.
They are not the source of truth for the native domains. The native artifacts
remain the inputs `I`; the lifters are the projections `k`; the canonicalized
claim IDs are the portable `t` values.

## Implementation Choices For Plan

- Which subset of ProofIR should represent bit-vector addition and carry for
  this exhibit?
- Should the first runner live in the existing Bug Zoo Rust crate pattern or in
  a new Bridgeworks-specific command?
- How much of the paper and measurement lifter should be structured parsing on
  day one versus signed extracted claims plus explicit trusted stops?
- Which checks should be fully mechanical first: source/ISA/RTL/netlist are the
  strongest candidates.

## Success Criteria

- The Bridgeworks README no longer reads as cross-language interop only.
- The checked-add exhibit has a complete contract chain before implementation
  starts.
- Parallel build tasks can be assigned one layer at a time with no sibling
  implementation context.
- The first runnable version emits one compressed receipt for the top software
  claim and one refusal receipt per mutation.
- Every layer can be explained in the same sentence form: `prove k(I)=t`.
