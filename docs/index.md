# ProvekIt documentation

ProvekIt is a toolchain for proving content-addressed claims.

The center use case is software correctness across domains: language to language, package to package, protocol version to protocol version, CI result to supply-chain input closure, and generated repair to re-lifted proof. Cross-platform contract correctness is one expression of that. The common shape is simple: canonicalize a proposition, name it by CID, name the evidence by CID, sign the edge, and fail closed when the graph does not carry the claim.

That linked evidence object is a proofchain: a locally verifiable chain of signed, content-addressed evidence for logically true claims.

Current protocol catalog: **v1.6.3**

Current catalog CID: `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d`

## References

| Topic | Read |
|---|---|
| Current CIDs | [reference/cids.md](reference/cids.md) |
| Protocol extensions | [reference/protocol-extensions.md](reference/protocol-extensions.md) |
| Canonical IR bytes | [reference/ir/canonical-form.md](reference/ir/canonical-form.md) |
| Diagnostic codes | [reference/error-codes.md](reference/error-codes.md) |
| LSP helper protocol | [reference/lsp-plugin-protocol.md](reference/lsp-plugin-protocol.md) |

## Start here

| Goal | Read |
|---|---|
| Install and run the verifier | [quickstart-end-user.md](quickstart-end-user.md) |
| Build or extend a kit | [quickstart-extender.md](quickstart-extender.md) |
| Understand the broad use cases | [explanation/use-cases.md](explanation/use-cases.md) |
| Understand the architecture | [explanation/architecture.md](explanation/architecture.md) |
| Read the protocol/tooling extension map | [reference/protocol-extensions.md](reference/protocol-extensions.md) |
| Look up current CIDs | [reference/cids.md](reference/cids.md) |

## Use ProvekIt in code

| If you write... | Start here | Current shape |
|---|---|---|
| Rust | [tutorials/rust.md](tutorials/rust.md) | canonical CLI and Rust libraries |
| TypeScript | [tutorials/typescript.md](tutorials/typescript.md) | kit and lift adapters; verify via Rust CLI |
| Python | [tutorials/python.md](tutorials/python.md) | kit and pydantic lift; verify via Rust CLI |
| Java / JVM | [tutorials/java.md](tutorials/java.md) | kit, lift adapters, and Java realizer work |
| C# | [tutorials/csharp.md](tutorials/csharp.md) | kit and lift adapters |
| Ruby | [tutorials/ruby.md](tutorials/ruby.md) | kit and lift adapters |
| Zig | [tutorials/zig.md](tutorials/zig.md) | kit and comment-based lift |
| Go | [tutorials/go.md](tutorials/go.md) | kit and validator lift |
| C++ | [tutorials/cpp.md](tutorials/cpp.md) | kit and C++ contracts lift |
| Swift | [tutorials/swift.md](tutorials/swift.md) | kit and CICP conformance |
| C | [tutorials/c.md](tutorials/c.md) | kit and CICP conformance |
| A polyglot stack | [tutorials/polyglot-stack.md](tutorials/polyglot-stack.md) | cross-domain boundary equivalence |

See [reference/per-language-status.md](reference/per-language-status.md) for the live matrix.

## Workflows

| Goal | Read |
|---|---|
| Publish a `.proof` artifact | [how-to/publishing-a-proof.md](how-to/publishing-a-proof.md) |
| Bridge claims across languages or domains | [how-to/cross-domain-bridges.md](how-to/cross-domain-bridges.md) |
| Bind CI results to supply-chain inputs | [how-to/content-addressed-ci.md](how-to/content-addressed-ci.md) |
| Run Bug Zoo specimens | [how-to/bug-zoo.md](how-to/bug-zoo.md) |
| Debug a failed verifier or IDE handshake | [how-to/debugging-a-failed-handshake.md](how-to/debugging-a-failed-handshake.md) |
| Integrate an IDE | [how-to/ide-integration/overview.md](how-to/ide-integration/overview.md) |
| Use the example GitHub workflow | [templates/provekit-example-workflow.yml](templates/provekit-example-workflow.yml) |
| Read CI and operational logs | [operations/logging.md](operations/logging.md) |

## Protocol surface

| Surface | Where it lives |
|---|---|
| Protocol catalog | [../protocol/specs/2026-04-30-protocol-catalog.json](../protocol/specs/2026-04-30-protocol-catalog.json) |
| Protocol Evolution Protocol (PEP) | [../protocol/specs/2026-05-07-protocol-evolution-protocol.md](../protocol/specs/2026-05-07-protocol-evolution-protocol.md) |
| Content-Addressed CI Protocol (CICP) | [../protocol/specs/2026-05-07-content-addressed-ci-protocol.md](../protocol/specs/2026-05-07-content-addressed-ci-protocol.md) |
| Grammar Conformance Protocol (GCP) | [../protocol/specs/2026-05-06-grammar-conformance-protocol.md](../protocol/specs/2026-05-06-grammar-conformance-protocol.md) |
| Truth Discharge Protocol (TDP) | [../protocol/specs/2026-05-06-truth-discharge-protocol.md](../protocol/specs/2026-05-06-truth-discharge-protocol.md) |
| Obligation Realizer Protocol (ORP) | [../protocol/specs/2026-05-06-obligation-realizer-protocol.md](../protocol/specs/2026-05-06-obligation-realizer-protocol.md) |
| Checker Bytecode Protocol (CBP) | [../protocol/specs/2026-05-06-checker-bytecode-protocol.md](../protocol/specs/2026-05-06-checker-bytecode-protocol.md) |
| Fix Receipt Protocol (FRP) | [../protocol/specs/2026-05-06-fix-receipt-protocol.md](../protocol/specs/2026-05-06-fix-receipt-protocol.md) |
| Proof protocol fixtures | [../protocol/conformance/proof-protocol/README.md](../protocol/conformance/proof-protocol/README.md) |
| CICP vectors | [../protocol/conformance/cicp/README.md](../protocol/conformance/cicp/README.md) |
| PEP dogfood transitions | [../protocol/evolution/README.md](../protocol/evolution/README.md) |

## Concepts

| Question | Read |
|---|---|
| What is the central claim? | [explanation/thesis.md](explanation/thesis.md) |
| Why content-addressing instead of a registry? | [explanation/content-addressing-not-registry.md](explanation/content-addressing-not-registry.md) |
| What is a proofchain? | [explanation/proofchain.md](explanation/proofchain.md) |
| Why lift, do not author? | [explanation/lift-not-author.md](explanation/lift-not-author.md) |
| Why is provability monotonic? | [explanation/monotonic-provability.md](explanation/monotonic-provability.md) |
| How do we know two `if`s in different languages are the same construct? | [explanation/cross-language-equivalence.md](explanation/cross-language-equivalence.md) |
| What is out of scope? | [explanation/boundaries.md](explanation/boundaries.md) |
| What is the product surface? | [explanation/product.md](explanation/product.md) |
| What is the short pitch? | [explanation/pitch.md](explanation/pitch.md) |
| What is canonical form? | [reference/ir/canonical-form.md](reference/ir/canonical-form.md) |

## Papers

The papers are sustained arguments, not how-to docs. Start with [papers/README.md](papers/README.md). The newest papers connect directly to the recent protocol/tooling work:

- [After Protocol Specs: How Protocols Actually Evolve](papers/10-after-protocol-specs-how-protocols-actually-evolve.md)
- [After Commits: Proof-Carrying Change as p -> q](papers/11-after-commits-proof-carrying-change.md)
- [Lossy Boundary Compression: Why ProofIR Is Universal Because It Forgets](papers/09-lossy-boundary-compression.md)

## Contribute

| Goal | Read |
|---|---|
| Build from source | [contributing/build.md](contributing/build.md) |
| Add a host language | [contributing/porting-to-a-new-language.md](contributing/porting-to-a-new-language.md) |
| Write a kit | [contributing/writing-a-kit/](contributing/writing-a-kit/) |
| Write a lift adapter | [contributing/writing-a-lift-adapter/](contributing/writing-a-lift-adapter/) |
| Write an LSP plugin | [contributing/writing-an-LSP-plugin.md](contributing/writing-an-LSP-plugin.md) |
| Write a prover backend | [contributing/writing-a-prover-backend.md](contributing/writing-a-prover-backend.md) |
| Propose a protocol change | [contributing/proposing-a-spec-change.md](contributing/proposing-a-spec-change.md) |
| Cut a release | [contributing/release-process.md](contributing/release-process.md) |

## Trust and security

| Question | Read |
|---|---|
| What attacks are in scope? | [security/threat-model.md](security/threat-model.md) |
| What does `binaryCid` catch? | [security/what-binaryCid-catches.md](security/what-binaryCid-catches.md) |
| What does `binaryCid` not catch? | [security/what-binaryCid-does-not-catch.md](security/what-binaryCid-does-not-catch.md) |
| How does multi-dimensional pinning work? | [security/multi-dimensional-pinning.md](security/multi-dimensional-pinning.md) |
| What is the solver trust boundary? | [security/solver-trust.md](security/solver-trust.md) |
| What is the lift adapter trust boundary? | [security/adapter-trust.md](security/adapter-trust.md) |
| How do signatures support non-repudiation? | [security/signature-and-non-repudiation.md](security/signature-and-non-repudiation.md) |
