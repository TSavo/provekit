# Bug Zoo

Bug Zoo is ProvekIt's executable laboratory.

Each species is a small, realistic specimen with four possible states:

- `lab/`: normal code or metadata that passes its ordinary host checks.
- `exposed/`: the same bug species lifted through one or more native surfaces until ProofIR exposes the missing contract edge.
- `dropped/`: proof-first plan plus language-dropper projection that closes the edge, accepted only after re-lift verifies closure.
- `wild/`: real OSS specimens pinned by advisory, commit, and affected path.

The zoo is not a patch archive. Historical fixes are context only. The receipt is independent rediscovery: can ProvekIt lift the latent contract boundary, make the missing `p => q` edge visible, and, where a dropper exists, synthesize a verified closure?

For dropped specimens, the preferred receipt stack is proof plan -> language dropper -> re-lift -> fix receipt. Proofless dropping is allowed only when specimen policy marks it as degraded evidence.

ProofIR is allowed to be lossy here. Specimens compare contract boundaries, not host-language implementation detail.
