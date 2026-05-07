# ProvekIt: Prove `k(I)=t`

> *Supra omnia, rectum.*
> — T

The name is literal: **Prove `k(I)=t`**. `I` is an implementation artifact, `k` is the canonical projection that reads the claim boundary, and `t` is the truth claim the artifact is supposed to yield. ProvekIt does not ask you to trust the artifact; it asks for signed, content-addressed evidence that applying `k` to `I` produces `t`.

ProvekIt is a toolchain for proving software correctness across domains. A domain can be a language, package ecosystem, protocol catalog, CI supply-chain closure, proof-file consumer, or generated repair. Cross-platform contract correctness is one use case. The general move is: name the proposition by CID, name the evidence by CID, sign the edge, and fail closed when the graph does not carry the claim.

Every if-statement is a contract — a guarantee about state, time, and place. Get any of it wrong, and the whole contract breaks. This is the bug class that exists in all upstream code; it's why if-statements exist at all.

But contracts don't travel. Not across domains, not even within them. The check fires locally and disappears — three function calls deep, your code has no idea what guarantees the leaf function requires. Contracts live INSIDE the code, not beneath it. Nothing demands all code conform.

## What if I told you that type of bug was impossible?

What if every if-statement weren't just a check — what if it were a demand on every upstream caller to respect it at compile time?

Not just within the function. Not just within the language. Across languages. Across platforms. Across domains.

What would change? How would software engineering be different? How would supply chains operate?

---

## ProvekIt is the substrate that makes that true.

Every if-statement, assertion, and type signature becomes a signed, binding demand on every upstream caller — enforced at compile time, across every language, across every domain you cross.

**Bug classes vanish.** NullPointerException is no longer a runtime event. Neither is use-after-free, SQL injection, path traversal, or any bug class your error handlers exist to catch.

**Software engineering shifts.** The artifact is the proof. Code is one implementation. Refactoring becomes proof-preserving rewrite. AI becomes a contract-implementation generator.

**Supply chains compose.** Every dependency's signed contracts compose into your application's verified properties. CI results bind to exact proof/toolchain/input closures. Dependency confusion becomes arithmetically impossible. SBOMs become meaningful artifacts.

## ProofIR is allowed to be lossy

ProofIR is not a universal language for re-expressing every implementation detail of every programming language. It is a universal language for claim boundaries: preconditions, postconditions, invariants, protocol obligations, value predicates, resource states, signer claims, CI blast radii, grammar conformance claims, realizer outputs, and the implication edges that connect them.

That is why it can work across domains. A Spring annotation, a Zod validator, an OpenAPI schema, a Rust type invariant, and a ProvekIt-native contract can all collapse to the same canonical predicate when they assert the same boundary fact. The host-language texture can be discarded; the obligation survives.

Once lifted, that boundary is universal, comparable, solvable, translatable, content-addressable, and signable. It has canonical bytes and a CID. It can be carried across languages, repositories, package ecosystems, commits, and time. The contracts were often already in your code; ProvekIt turns them into accountable edges the rest of the graph must satisfy.

## I want to...

| | |
| --- | --- |
| **See it in my language — and every other language at the same time** | [docs/tutorials/polyglot-stack.md](docs/tutorials/polyglot-stack.md) |
| **See a bug class collapse to the same bytes across languages** | [docs/explanation/bug-zoo.md](docs/explanation/bug-zoo.md) |
| **Understand the move** | [docs/papers/](docs/papers/) — recommended order: paper 03 → 06 → 02 |
| **Extend it / build a kit** | [docs/contributing/](docs/contributing/) |
| **Read the spec** | [docs/papers/02-bluepaper.md](docs/papers/02-bluepaper.md) |
| **Understand the new protocol/tooling surface** | [docs/reference/protocol-extensions.md](docs/reference/protocol-extensions.md) |
| **Bind CI results to supply-chain inputs** | [docs/how-to/content-addressed-ci.md](docs/how-to/content-addressed-ci.md) |
| **Run the Bug Zoo / dropper realizer lab** | [docs/how-to/bug-zoo.md](docs/how-to/bug-zoo.md) |
| **Compare to other tools** | [docs/explanation/compared-to/](docs/explanation/compared-to/) |

For more entry points (per-language tutorials, IDE integration, publishing a `.proof`, CICP, Bug Zoo, protocol extensions, threat model, and spec CIDs), see [docs/index.md](docs/index.md).

## Status

- **Protocol catalog**: v1.6.2 (patch over v1.6.1; catalogs the Content-Addressed CI Protocol as an extension-only protocol)
- **Catalog CID**: `blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f`
- **Canonical implementation**: Rust (`cargo install provekit`)
- **Conforming implementations**: Rust, TypeScript, Python, Java, C#, Ruby, Zig, Go, C++, Swift, C, PHP. Coverage varies; see [docs/reference/per-language-status.md](docs/reference/per-language-status.md).
- **Protocol evolution**: PEP dogfoods catalog transitions as signed, content-addressed body-claims under `protocol/evolution/v1.6.1/` and `protocol/evolution/v1.6.2/`.
- **Content-addressed CI**: CICP binds CI results to exact source, protocol catalog, kit/toolchain, config, and accepted witness inputs. Reuse is allowed only when that closure is byte-identical.
- **Bug Zoo / realizers**: `provekit zoo` checks lab, exposed, dropped, and wild specimens; dropped specimens are accepted only after realizer output is re-lifted and bound to a fix receipt.
- **Conformance gate**: catalog CIDs, proof-protocol fixtures, CICP vectors, self-contract attestations, and per-kit tests must agree before CI is green.

The protocol is content-addressed end to end. Each version's canonical name is its own catalog hash. Anyone with the spec bytes can verify that label locally. No central party decides what a version means; the bytes do.

## Bug Zoo

Bug Zoo is the executable lab for the claim above. Each specimen runs in an
isolated host-language environment, uses that language's own compiler/kit to
map source to a witnessed bug output, then asks `provekit zoo` to verify that
the canonical ProofIR signature is byte-identical across surfaces and
languages.

In shorthand:

```text
k_lang(I) = t
```

`k_lang` is the language compiler as a ProvekIt kit/lifter, `I` is the source,
and `t` is the witnessed output: canonical ProofIR bytes, CID, and receipt.
Different languages can disagree in syntax, runtime behavior, and exception
type while still compiling to the same witnessed `t`.

The current null-boundary receipts show TypeScript and C# lifting the same
missing edge:

```text
maybe_null(name) => non_null(name)
```

to the same ProofIR CID:

```text
blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4
```

Read [docs/explanation/bug-zoo.md](docs/explanation/bug-zoo.md), or run:

```sh
(cd implementations/rust && cargo build -p provekit-cli)
implementations/rust/target/debug/provekit zoo bug-zoo/species --all
```

| Kit | Self-contracts | Lift-plugin-protocol bridges | LSP plugin |
|---|---|---|---|
| Rust | full conformance | full (source of truth) | shipping |
| Go | full conformance | in progress | planned |
| C# | full conformance | not started | shipping |
| Ruby | in progress | not started | shipping |
| Zig | in progress | not started | shipping |
| Python | full conformance | in progress | shipping |
| TypeScript | full conformance | in progress | planned |
| C++ | full conformance | not started | planned |
| Java | full conformance | not started | planned |
| Swift | full conformance | not started | planned |
| C | full conformance | not started | planned |
| PHP | in progress | not started | planned |

## Install

This project is **build-from-source only**. Crates.io publishing is on the roadmap; until then see [docs/quickstart-end-user.md](docs/quickstart-end-user.md) for build instructions.

The core binary is:

```sh
cargo install --path implementations/rust/provekit-cli
```

`provekit verify-protocol` confirms the local install conforms to the expected protocol catalog CID. `cargo provekit-lift` walks the workspace, runs every registered lift adapter, and emits a `.proof` catalog of signed contract mementos. `provekit prove` runs the three-tier handshake and reports the discharge breakdown. `provekit proof`, `provekit protocol`, `provekit ci`, and `provekit zoo` cover proof-file conformance, PEP transitions, CICP supply-chain admission, and Bug Zoo specimens. Any of these can fail closed; none requires the network.

For other host languages, see the polyglot-stack tutorial above. The Rust CLI is the canonical implementation; non-Rust kits use it for verification today.

## Building from source

If you are working on ProvekIt itself (kit, lift adapter, prover backend, spec change), see [docs/contributing/build.md](docs/contributing/build.md) for the polyglot Make targets, system dependencies, and per-implementation build commands. The conformance gate (`make ci`) enforces byte-determinism across every implementation.

## License

Source files use SPDX headers where present. A repository-level license file has not been added yet.
