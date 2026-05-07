# Bug Zoo

Bug Zoo is where ProvekIt proves the bug story against executable software.

A normal bug corpus says: this project failed, and this patch fixed it. Bug Zoo asks for a stronger receipt: can ProvekIt rediscover the missing obligation from the code, framework metadata, schemas, or annotations already present; name that obligation by CID; and accept a repair only after the repaired artifact re-lifts to a closed proof graph?

That turns software history into testable infrastructure. A null check, SQL-safety guard, path confinement rule, authorization edge, or resource-state transition stops being folklore in a changelog and becomes a content-addressed boundary claim the substrate can search, compare, sign, and reuse.

The zoo is not a patch archive. Historical fixes are context. The durable artifact is independent rediscovery: ProvekIt found the missing `p => q` edge, proved which source surfaces express it, and, where a dropper exists, accepted generated code only after re-lift proved the edge was closed.

## Why It Matters

Bug Zoo makes the big claim falsifiable.

- Lifters are tested against real bug shapes: did the lift preserve the boundary that mattered?
- Droppers are tested against closure, not plausibility: did the generated host artifact re-lift to the required ProofIR?
- Cross-domain correctness is tested directly: can different source surfaces collapse to the same claim boundary?
- The corpus prioritizes the substrate: recurring missing edges deserve first-class predicates and stronger kit coverage.

## Current Specimen

The default specimen is:

```text
bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence
```

It is intentionally small: ordinary Java checks pass, then two different Java surfaces expose the same non-null boundary. ProvekIt sees the missing edge:

```text
maybe_null(name) => non_null(name)
```

The dropped variant uses a Java ORP realizer to emit a native edge-closing shape. The transform is not trusted because it was generated. It is accepted only after the Java lifter reads the changed artifact back and the fix receipt binds the source artifact, transformed artifact, post-lift ProofIR, closure witness, and policy.

## Specimen States

Each species can carry four states:

- `lab/`: normal code or metadata that passes its ordinary host checks.
- `exposed/`: the same bug species lifted through one or more native surfaces until ProofIR exposes the missing contract edge.
- `dropped/`: proof-first plan plus language-dropper projection that closes the edge, accepted only after re-lift verifies closure.
- `wild/`: real OSS specimens pinned by advisory, commit, affected path, and evidence.

Run the default specimen:

```sh
provekit zoo
```

Run all checked-in specimens:

```sh
provekit zoo --all
```

## Receipt Stack

For dropped specimens, the preferred receipt stack is:

```text
proof plan -> language dropper -> realizer output -> re-lift -> closure witness -> fix receipt
```

Proofless dropping is allowed only when specimen policy marks it as degraded evidence.

ProofIR is allowed to be lossy here. Specimens compare contract boundaries, not host-language implementation detail.
