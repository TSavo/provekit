# ProvekIt: Prove `k(I)=t`

> *Supra omnia, rectum.*
> — T

The name is literal: **Prove `k(I)=t`**. `I` is an implementation artifact, `k` is the canonical projection that reads the contract boundary, and `t` is the truth claim the artifact is supposed to yield. ProvekIt does not ask you to trust the artifact; it asks for signed, content-addressed evidence that applying `k` to `I` produces `t`.

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

**Supply chains compose.** Every dependency's signed contracts compose into your application's verified properties. Dependency confusion becomes arithmetically impossible. SBOMs become meaningful artifacts.

## ProofIR is allowed to be lossy

ProofIR is not a universal language for re-expressing every implementation detail of every programming language. It is a universal language for contract boundaries: preconditions, postconditions, invariants, protocol obligations, value predicates, resource states, signer claims, and the implication edges that connect them.

That is why it can work across domains. A Spring annotation, a Zod validator, an OpenAPI schema, a Rust type invariant, and a ProvekIt-native contract can all collapse to the same canonical predicate when they assert the same boundary fact. The host-language texture can be discarded; the obligation survives.

Once lifted, that boundary is universal, comparable, solvable, translatable, content-addressable, and signable. It has canonical bytes and a CID. It can be carried across languages, repositories, package ecosystems, commits, and time. The contracts were often already in your code; ProvekIt turns them into accountable edges the rest of the graph must satisfy.

## I want to...

| | |
| --- | --- |
| **See it in my language — and every other language at the same time** | [docs/tutorials/polyglot-stack.md](docs/tutorials/polyglot-stack.md) |
| **Understand the move** | [docs/papers/](docs/papers/) — recommended order: paper 03 → 06 → 02 |
| **Extend it / build a kit** | [docs/contributing/](docs/contributing/) |
| **Read the spec** | [docs/papers/02-bluepaper.md](docs/papers/02-bluepaper.md) |
| **Compare to other tools** | [docs/explanation/compared-to/](docs/explanation/compared-to/) |

For more entry points (per-language tutorials, IDE integration, publishing a `.proof`, consuming a `.proof`, threat model, CLI reference, IR reference, spec CIDs), see [docs/index.md](docs/index.md).

## Status

- **Protocol catalog**: v1.4.1 (patch over v1.4.0; v1.4.0 mementos and `.proof` bundles remain valid)
- **Catalog CID**: `blake3-512:dc2f42ff8a4a66289cc19bfbd628898b8bd8e61d2148ecf609324cc2421c5c440a6c0e70e20ffbecabeb78e0253101d72823b7e3ab120a4d56cb67c8e31dc641`
- **Canonical implementation**: Rust (`cargo install provekit`)
- **Conforming implementations**: Rust, TypeScript, Python, Java, C#, Ruby, Zig, Go, C++, Swift, C, PHP. Coverage varies; see [docs/reference/per-language-status.md](docs/reference/per-language-status.md).
- **Conformance gate**: every kit's mint must match a pinned content-addressed CID before `make ci` is green.

The protocol is content-addressed end to end. Each version's canonical name is its own catalog hash. Anyone with the spec bytes can verify that label locally. No central party decides what a version means; the bytes do.

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

`provekit verify-protocol` confirms the local install conforms to the expected protocol catalog CID. `cargo provekit-lift` walks the workspace, runs every registered lift adapter, and emits a `.proof` catalog of signed contract mementos. `provekit prove` runs the three-tier handshake and reports the discharge breakdown. Any of these can fail closed; none requires the network.

For other host languages, see the polyglot-stack tutorial above. The Rust CLI is the canonical implementation; non-Rust kits use it for verification today.

## Building from source

If you are working on ProvekIt itself (kit, lift adapter, prover backend, spec change), see [docs/contributing/build.md](docs/contributing/build.md) for the polyglot Make targets, system dependencies, and per-implementation build commands. The conformance gate (`make ci`) enforces byte-determinism across every implementation.

## License

See [LICENSE](LICENSE).
