# Protocol Extension Surface

This page maps the post-v1.6 protocol/tooling surface. It separates cataloged protocol properties from draft companion specs and executable workflows.

## Current Catalog

| Item | Value |
|---|---|
| Catalog version | `v1.6.6-2026-05-26` |
| Catalog CID | `blake3-512:809ed1ebd538f206beb9df6de712f502fbcd310ee52d76c34afecec6455259d49cd7d288eb761d5aac9ebbd3643ae4dfe09bc9c7f2aea23e57720df6085c6640` |
| Catalog file | [../../protocol/specs/2026-04-30-protocol-catalog.json](../../protocol/specs/2026-04-30-protocol-catalog.json) |
| Signed attestation | [../../.provekit/catalog-signatures/v1.6.6.json](../../.provekit/catalog-signatures/v1.6.6.json) |

## Cataloged Extensions

| Key | Spec | CID | Meaning |
|---|---|---|---|
| `protocol-evolution-protocol` | [PEP](../../protocol/specs/2026-05-07-protocol-evolution-protocol.md) | `blake3-512:d8827f89df20e5be38c4d5de851fe4e55420dcd6cacfd9b98f458c53e64e6ba07349e29f8da2fbab6cb7195b297c3704a70f489c020e3f55c96ef702c4a09949` | Protocol catalog transitions become signed, content-addressed body-claims. |
| `lift-plugin-protocol` | [Lift Plugin Protocol](../../protocol/specs/2026-04-30-lift-plugin-protocol.md) | `blake3-512:f2b856a8010b0f95cdd9961e0c367b003b1de7be39b6668db7f96cfe884a99f153609a846be39ad4a4f40a3bb778fecf2b0e24908b94411f32be165473045055` | `provekit lift` and `provekit package inspect` use the same configured lifter RPC; identify-only package inspection emits content-addressed package, CI, contract, and `.proof` rails. |

## Draft Companion Specs

These are protocol working notes in `protocol/specs/`. They are content-addressed by raw bytes, but they are not catalog properties in v1.6.4 unless listed above.

| Spec | Raw-byte CID | Role |
|---|---|---|
| [Extension Protocols](../../protocol/specs/2026-05-06-extension-protocols.md) | `blake3-512:15793792a06920f8008663d7fb8735606b331ff7df32ef360ff97456bd23fc7c14b3ede07fd124a8f8b010a249764e7d62f00bc15fbcdfef320c5462b2407f42` | Names the extension-protocol posture: body conventions over a stable core. |
| [Truth Discharge Protocol (TDP)](../../protocol/specs/2026-05-06-truth-discharge-protocol.md) | `blake3-512:c8fd24f1a5addc7b07290f50d24fe108422aa500207772dd0b990753b7acafa5bac4ad7fd26f49f8a3460719e9290620b6120246e3a667a5ed8765406b0041ad` | Common positive-witness shape for extension body-claims. |
| [Grammar Conformance Protocol (GCP)](../../protocol/specs/2026-05-06-grammar-conformance-protocol.md) | `blake3-512:84195382a699c1ef0d91d5ac22fe6f298eae9e7d0c9effc078fb03503c7d86b57db403f24f337dccf17c99d62d7df14179903db4e279bf92b0fce90fd17b1373` | Witnesses that a signed body conforms to a grammar and optional ProofIR invariants. |
| [Checker Bytecode Protocol (CBP)](../../protocol/specs/2026-05-06-checker-bytecode-protocol.md) | `blake3-512:469ddd6bd75c912e7542ed85e2e47a8c61709ea23db027543b7150eff7cdfa3507fb8f4cf57979d86178bc9297265478f0f872f3dc59ab0e4b64a1f131b34c41` | Names executable checker artifacts without making core verification execute bytecode. |
| [Obligation Realizer Protocol (ORP)](../../protocol/specs/2026-05-06-obligation-realizer-protocol.md) | `blake3-512:33c3f0fef422ff0ec616afe9b235ff007275a27b0b5a8df84b2cf9d793d4916d33f14fe96fd605ab9a425ae0ec4ec07489780490f6180dd0cea74c244f6deb14` | Defines witnessers, droppers, monitors, proof plans, and realizer RPC. |
| [Fix Receipt Protocol (FRP)](../../protocol/specs/2026-05-06-fix-receipt-protocol.md) | `blake3-512:57dab6ad40f1189479fb976043fd8dfcf7f223638e06ae888bc8e1a755f7b92ae1e036f3fbdb7266ce6164a9573b828609bc9e2ec2295ba6efb8415bfd32ed62` | Records closure receipts for changed behavior. |
| [ProofIR Realization Compiler](../../protocol/specs/2026-05-06-proofir-realization-compiler.md) | `blake3-512:24b055345e4eb53dd6a1f4983370071e8df30c40b6e027977df5073d2ebee7fe74917c941e17114b216d01141b2473b363d2eb82612def8b94c940721761e0f3` | Describes compilation from ProofIR obligations to realization artifacts. |

## Tool Surfaces

| Command | Purpose | Primary docs |
|---|---|---|
| `provekit protocol evolve/check-evolution` | Emit or verify PEP body/witness artifacts for catalog transitions. | [../../protocol/evolution/v1.6.4/README.md](../../protocol/evolution/v1.6.4/README.md) |
| `provekit package inspect` | Dispatch to the configured lifter with `options.layer = "identify-only"` and require a `package-inspection-document`. | [../../protocol/specs/2026-04-30-lift-plugin-protocol.md](../../protocol/specs/2026-04-30-lift-plugin-protocol.md) |
| `cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- [--all]` | Run self-contained Bug Zoo specimens through host checks, exhibits, link exhibits, fixed pairs, equivalence checks, and scoped composition checks. | [../how-to/bug-zoo.md](../how-to/bug-zoo.md) |

## Trust Boundary

The core verifier does not execute PEP, GCP, CBP, ORP, parsers, checker bytecode, droppers, or realizer code. Core verification checks bytes, CIDs, signatures, and core memento/header rules. Extension-aware tooling may evaluate extension bodies under explicit policy and then emit another signed/content-addressed witness.
