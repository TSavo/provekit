# Protocol Evolution Witnesses

This tree carries checked-in PEP dogfood transitions.

Each version directory contains:

- predecessor and successor catalog snapshots;
- a catalog diff;
- bootstrap policy and verifier descriptors;
- a `ProtocolEvolutionBodyClaim`;
- a TDP-shaped witness for that body;
- pinned CIDs for the transition artifacts.

Current transitions:

| Transition | Purpose |
|---|---|
| [v1.6.1](v1.6.1/README.md) | Catalog PEP itself as an extension-only protocol over v1.6.0. |
| [v1.6.2](v1.6.2/README.md) | Catalog CICP as an extension-only protocol over v1.6.1. |

Core verification does not execute PEP. Core verification checks signed
bytes, CIDs, references, and catalog attestations. PEP-aware tooling can
then admit or refuse the transition under policy.
