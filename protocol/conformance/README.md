# Protocol Conformance Test Vectors

This tree carries protocol-level vector corpora shared by every
implementation.

- `proof-protocol/`: `.proof` bundle conformance fixtures and expected
  verdicts.
- `cicp/`: Content-Addressed CI Protocol body-claim vectors. Each
  language library should derive the pinned CIDs for passing vectors
  and fail closed on refusal vectors.

The Rust implementation is the reference oracle. Other language kits may
use native data types, but the conformance boundary is the canonical CID
over the vector body.
