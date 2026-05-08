# Bridgeworks Checked-Add Walkthrough

Run these scripts from this directory or from anywhere in the repo:

```sh
./00-start-here.sh
./01-map-stack.sh
./02-show-native-contracts.sh
./03-lift-to-proofir.sh
./04-show-bridge-edges.sh
./05-mint-proof-dag.sh
./06-walk-proof-cids.sh
./07-break-experiment.sh
./08-break-device-physics.sh
./09-break-cells.sh
./10-break-gates.sh
./11-break-rtl.sh
./12-break-isa.sh
./13-break-compiler.sh
./14-break-software-identity.sh
./15-break-software-witness.sh
./16-run-whole-exhibit.sh
```

Each script is self-contained and uses temp directories for generated artifacts.
The first run prepares local binaries under the repo `target/` directory; after
that the scripts invoke `provekit`, `provekit-bridgeworks`, and kit binaries
directly.
`02` uses `provekit lift --identify-only` to show native contract identities.
`03` and `04` use `provekit lift` to show the full lifted ProofIR response.
The lowered C witness is generated at runtime by `provekit lower --mode witness`;
it is not checked in.
