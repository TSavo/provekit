# ProvekIt documentation

ProvekIt is a proof supply chain for existing software ecosystems.

It turns language-native evidence, including tests, assertions, contracts,
schemas, validators, framework annotations, and boundary/library sugar, into
portable ProofIR or protocol claims. Kits own the language and package-manager
details. The CLI stays language-agnostic: it loads `.proof` artifacts, speaks
RPC to kits, composes normalized claims, emits proof bundles, and proves the
assembled obligations.

The center use case is assembled correctness. Two packages can each pass their
own checks and still make contradictory claims when used together. ProvekIt
makes those claims meet in one content-addressed `.proof` DAG and fails closed
when the graph cannot carry the composed claim.

A proofchain is the linked evidence object formed by that DAG: canonical claims,
CIDs, signatures, witnesses, attestations, and verifier policy. Previously
minted commitments can often be rechecked cheaply by CID equality and signature
verification, while semantic proving still happens when a claim is minted,
changed, or newly composed.

The canonical CLI embeds its current protocol catalog. Verify your local binary
with `provekit verify-protocol`; see [reference/cids.md](reference/cids.md) for
spec CID background.

## References

| Topic | Read |
|---|---|
| Current CIDs | [reference/cids.md](reference/cids.md) |
| Protocol extensions | [reference/protocol-extensions.md](reference/protocol-extensions.md) |
| Canonical IR bytes | [reference/ir/canonical-form.md](reference/ir/canonical-form.md) |
| Diagnostic codes | [reference/error-codes.md](reference/error-codes.md) |
| Materialize concept citations | [reference/materialize.md](reference/materialize.md) |
| LSP helper protocol | [reference/lsp-plugin-protocol.md](reference/lsp-plugin-protocol.md) |

## Start here

| Goal | Read |
|---|---|
| Install and run the verifier | [quickstart-end-user.md](quickstart-end-user.md) |
| Build or extend a kit | [quickstart-extender.md](quickstart-extender.md) |
| Understand the broad use cases | [explanation/use-cases.md](explanation/use-cases.md) |
| Understand the product surface | [explanation/product.md](explanation/product.md) |
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
| Swift | [tutorials/swift.md](tutorials/swift.md) | kit conformance |
| C | [tutorials/c.md](tutorials/c.md) | kit conformance |
| A polyglot stack | [tutorials/polyglot-stack.md](tutorials/polyglot-stack.md) | cross-domain boundary equivalence |

See [reference/per-language-status.md](reference/per-language-status.md) for the live matrix.

## Workflows

| Goal | Read |
|---|---|
| Publish a `.proof` artifact | [how-to/publishing-a-proof.md](how-to/publishing-a-proof.md) |
| Bridge claims across languages or domains | [how-to/cross-domain-bridges.md](how-to/cross-domain-bridges.md) |
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
| Grammar Conformance Protocol (GCP) | [../protocol/specs/2026-05-06-grammar-conformance-protocol.md](../protocol/specs/2026-05-06-grammar-conformance-protocol.md) |
| Truth Discharge Protocol (TDP) | [../protocol/specs/2026-05-06-truth-discharge-protocol.md](../protocol/specs/2026-05-06-truth-discharge-protocol.md) |
| Obligation Realizer Protocol (ORP) | [../protocol/specs/2026-05-06-obligation-realizer-protocol.md](../protocol/specs/2026-05-06-obligation-realizer-protocol.md) |
| Checker Bytecode Protocol (CBP) | [../protocol/specs/2026-05-06-checker-bytecode-protocol.md](../protocol/specs/2026-05-06-checker-bytecode-protocol.md) |
| Fix Receipt Protocol (FRP) | [../protocol/specs/2026-05-06-fix-receipt-protocol.md](../protocol/specs/2026-05-06-fix-receipt-protocol.md) |
| Proof protocol fixtures | [../protocol/conformance/proof-protocol/README.md](../protocol/conformance/proof-protocol/README.md) |
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
| How do kits and the CLI divide responsibility? | [explanation/architecture.md](explanation/architecture.md) |
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
