# ProvekIt docs IA proposal

Draft for review. Not committed. Not in `docs/` until you've redirected.

## What's wrong with the current docs

The thesis claims a polyglot, content-addressed protocol. The docs ship a Rust manual.

| Claim in README/THESIS | What docs deliver today |
|---|---|
| "verify a petabyte of behavior across any dependency graph" | one Rust tutorial, no consumer-of-dependencies walkthrough |
| "lift, don't author" — works with proptest, contracts, zod, class-validator, fast-check, pydantic, Bean Validation, JML, DataAnnotations, LINQ, dry-validation, rspec, active_model | one tutorial, Rust + proptest + contracts only |
| "cross-domain verification for free" via bridges | zero worked examples; the parseInt bridge is invented prose |
| "compile-time errors, red squigglies" via LSP plugins | five plugins ship (Rust, Python, Zig, Ruby, C#); zero install docs; zero IDE wire-up docs |
| "conforming implementations exist or are planned for Rust, TS, Go, C++, etc." | five-language polyglot in the tree, but contributor on-ramp is "read the source" |
| "the protocol is the kit standard" | kit standard is a bare CID, mentioned once at the bottom of a status matrix |

The problem isn't doc quantity. It's IA. There is no map. A user lands on README and gets pitched the thesis, then handed `cargo install`. Anyone who isn't a Rust dev with proptest annotations falls off immediately.

## Audiences (the matrix the IA must serve)

**Per-language users.** Eleven host languages today: Rust, TS, Python, Java, C#, Ruby, Go, C++, Zig, Swift, C. Each has a different toolchain (cargo / pnpm / pip / Maven / dotnet / gem / go / cmake / zig / swift / cmake), different annotation libraries, different LSP integration story, different CI idiom. A "pick your language" landing is the front door.

**Per-role users within a language.** Within each language, three roles need different docs:
- *Application developer*: writes code, wants squigglies + a build-time gate.
- *Library/package author*: ships a `.proof` alongside their package.
- *Consumer of dependencies*: verifies someone else's `.proof`.

These have overlapping but distinct journeys. A library author needs key management, signing identity, publication policy. A consumer needs the handshake interpretation, the discharge breakdown, the failure modes.

**Cross-language users.** The polyglot stack case. TS frontend + Python ML + Rust backend + Go gateway, all bridging to shared reference contracts. This case validates the thesis. Without a worked tutorial it remains a slogan.

**Contributors / porters.** Three sub-audiences:
- *New-language porter*: "I want to add Kotlin / Elixir / OCaml."
- *New-adapter author*: "I want to write a lift adapter for io-ts / valibot / icontract."
- *New-prover author*: "I want a Lean / TLA+ / CBMC backend instead of Z3."

Each of these is a real contributor traffic stream. None has a doc today.

**Operators / DevOps.** CI integration, build-script wiring, implication server operation, key rotation, monitoring discharge fraction over time, IDE rollout across a dev org.

**Decision-makers / evaluators.** Comparing to SLSA, Sigstore, in-toto, SCITT, SBOM formats. Or to Coq, F*, Lean, Kani, Prusti, Dafny, TLA+. Each comparison needs a shared frame: when ProvekIt complements, when it competes, what it does NOT do.

**Security / supply-chain professionals.** Threat model. What `binaryCid` catches. What it doesn't. What Z3 as TCB means. What adapter trust means. What signature non-repudiation buys.

**Researchers / formal-methods.** IR semantics, lattice tractability theorem, JCS canonicalization, signed memento envelope schema, monotonicity argument.

**Skeptics.** "What's the cold-start? Where do reference contracts come from? What's the lock-in? What's the exit path?" These are first-class doc questions, not appendix material.

## The IA: Diátaxis + contributing + examples + reference contracts

Diátaxis (Procida) gives four quadrants: **tutorials** (learning), **how-to** (task), **reference** (lookup), **explanation** (understanding). ProvekIt needs two extra trees: **contributing** (the polyglot story is a contributor story, not just a user story) and **examples** (worked end-to-end demos that prove the thesis).

Plus one specific tree most projects don't have: **reference contracts** — the curated bridge anchors (ref-parseInt-v1, ref-malloc-v1, ref-ieee754-arithmetic-v1, etc.) that make cross-domain transfer work. These need their own home; they're a product surface, not a doc afterthought.

```
docs/
├── index.md                              ← "what brought you here?" landing
│
├── tutorials/                            ← LEARNING (zero → first .proof)
│   ├── 00-five-minute-protocol-tour.md   ← language-agnostic protocol intro
│   ├── rust.md
│   ├── typescript.md
│   ├── python.md
│   ├── java.md
│   ├── csharp.md
│   ├── ruby.md
│   ├── zig.md
│   ├── go.md                             ← "v1.2 preview" if not shipping
│   ├── cpp.md                            ← same
│   ├── swift.md                          ← same
│   ├── c.md                              ← same
│   └── polyglot-stack.md                 ← LOAD-BEARING: cross-language demo
│
├── how-to/                               ← TASK (recipes)
│   ├── ide-integration/
│   │   ├── overview.md                   ← matrix of {plugin × editor}
│   │   ├── vscode.md
│   │   ├── neovim.md
│   │   ├── jetbrains.md
│   │   └── emacs.md
│   ├── ci-integration/
│   │   ├── github-actions.md
│   │   ├── gitlab.md
│   │   ├── jenkins.md
│   │   └── buildkite.md
│   ├── publishing-a-proof.md
│   ├── consuming-a-proof.md
│   ├── pinning-a-binary.md               ← supply-chain anchor
│   ├── authoring-contracts.md            ← per-language ergonomics
│   ├── lifting-existing-annotations.md   ← per-source-library coverage
│   ├── cross-domain-bridges.md           ← #[provekit::implement(target=...)]
│   ├── implication-servers.md
│   ├── debugging-a-failed-handshake.md
│   ├── interpreting-the-discharge-fraction.md
│   ├── managing-keys.md                  ← signing, rotation, revocation
│   └── migrating-to-provekit.md          ← gradual adoption
│
├── reference/                            ← LOOKUP
│   ├── cli/                              ← one page per subcommand
│   │   ├── prove.md
│   │   ├── verify-protocol.md
│   │   ├── lift.md
│   │   ├── dump.md
│   │   ├── hash.md
│   │   ├── search.md
│   │   ├── ask.md
│   │   ├── implicate.md
│   │   └── init.md
│   ├── ir/
│   │   ├── grammar.md                    ← CDDL, CID-anchored
│   │   ├── canonical-form.md             ← JCS, BLAKE3-512
│   │   ├── primitives.md                 ← Term, Formula, Sort
│   │   └── declarations.md               ← Contract, Bridge, Evidence
│   ├── proof-bundle/
│   │   ├── format.md
│   │   ├── members.md
│   │   ├── signatures.md
│   │   └── binaryCid.md
│   ├── handshake/
│   │   ├── algorithm.md
│   │   ├── tier-1-hash-equality.md
│   │   ├── tier-2-cached-implication.md
│   │   ├── tier-3-solver-fallback.md
│   │   └── failure-modes.md
│   ├── kit-standard.md                   ← what every kit must implement
│   ├── lift-plugin-protocol.md           ← NDJSON over stdio
│   ├── lsp-plugin-protocol.md
│   ├── per-language-status.md            ← (existing, stays here)
│   ├── per-adapter-coverage.md           ← per-library: what's seen, what's missed
│   ├── lattice-tractability.md           ← the theorem
│   ├── error-codes.md
│   ├── config/
│   │   ├── provekit.config.yaml.md
│   │   └── publish-policies.md
│   ├── conformance-fixtures.md
│   └── cids.md                           ← table of all spec CIDs at HEAD
│
├── explanation/                          ← WHY
│   ├── thesis.md                         ← (existing THESIS.md, moved)
│   ├── architecture.md                   ← (existing, moved)
│   ├── product.md                        ← (existing, moved)
│   ├── lift-not-author.md
│   ├── content-addressing-not-registry.md
│   ├── monotonic-provability.md
│   ├── cross-domain-verification.md
│   ├── cold-start.md                     ← honest bootstrap discussion
│   ├── boundaries.md                     ← what ProvekIt is NOT
│   └── compared-to/
│       ├── coq-fstar-lean.md
│       ├── kani-prusti-creusot.md
│       ├── dafny-tla-alloy.md
│       ├── slsa-sigstore-in-toto-scitt.md
│       ├── sbom-formats.md
│       └── runtime-testing-frameworks.md
│
├── contributing/                         ← PORTER ON-RAMP
│   ├── overview.md
│   ├── porting-to-a-new-language.md      ← 90% of contributor traffic
│   ├── writing-a-kit/
│   │   ├── 01-conformance-first.md       ← start with fixtures
│   │   ├── 02-canonicalizer.md
│   │   ├── 03-claim-envelope.md
│   │   ├── 04-proof-envelope.md
│   │   ├── 05-self-contracts.md
│   │   └── 06-bridge-IR.md
│   ├── writing-a-lift-adapter/
│   │   ├── 01-pick-a-source-library.md
│   │   ├── 02-walk-the-AST.md
│   │   ├── 03-emit-canonical-IR.md
│   │   ├── 04-conformance-test.md
│   │   └── 05-publishing.md
│   ├── writing-an-LSP-plugin.md
│   ├── writing-a-prover-backend.md       ← Lean / Coq / TLA+ / CBMC
│   ├── adapter-coverage-rubric.md
│   ├── proposing-a-spec-change.md        ← protocol governance
│   └── release-process.md
│
├── operations/                           ← DEVOPS
│   ├── running-an-implication-server.md
│   ├── ci-cookbook.md
│   ├── monitoring-and-metrics.md
│   ├── key-management.md
│   └── troubleshooting.md
│
├── security/                             ← TRUST / THREAT MODEL
│   ├── threat-model.md
│   ├── supply-chain.md
│   ├── signature-and-non-repudiation.md
│   ├── what-binaryCid-catches.md
│   ├── what-binaryCid-does-not-catch.md
│   ├── solver-trust.md                   ← Z3 as TCB
│   ├── adapter-trust.md
│   └── reporting-vulnerabilities.md
│
├── governance/                           ← PROTOCOL VERSIONING / SOCIAL
│   ├── protocol-versions.md
│   ├── catalog-cid-pinning.md
│   ├── deprecation-policy.md
│   ├── conformance-claims.md
│   └── governance-philosophy.md
│
├── examples/                             ← END-TO-END WORKED EXAMPLES
│   ├── rust-crate-with-proptest.md
│   ├── npm-package-with-zod.md
│   ├── python-with-pydantic.md
│   ├── java-with-bean-validation.md
│   ├── polyglot-microservices.md
│   ├── parseInt-cross-domain.md          ← THE canonical bridge example
│   └── supply-chain-attack-demo.md       ← prove what binaryCid catches
│
├── reference-contracts/                  ← CURATED BRIDGE ANCHORS
│   ├── README.md                         ← what they are, why they matter, how to add
│   ├── ecma262-parseInt.md
│   ├── ecma262-parseFloat.md
│   ├── posix-malloc.md
│   ├── ieee754-arithmetic.md
│   └── ...
│
└── glossary.md                           ← CID, memento, bridge, kit, adapter, lift, handshake, tier, sort, declaration, etc.
```

## Tutorial shape (uniform across languages)

Every per-language tutorial follows the same six-step shape so a polyglot dev can context-switch without re-learning the doc structure:

1. **What you'll have at the end.** Concrete artifact: a `.proof` file with N contracts, one bridge, an LSP showing red squigglies in your editor.
2. **Prerequisites.** Toolchain, OS, optional Z3.
3. **Install.** `cargo install` / `pnpm add` / `pip install` / `dotnet tool install` / `gem install` / etc.
4. **Lift or author your first contract.** Per-language: which annotation library does the tutorial use? proptest / zod / pydantic / DataAnnotations / Bean Validation / active_model / etc.
5. **Run prove. Read the discharge breakdown.** Same output shape regardless of host language; this is a Rust CLI subprocess for now.
6. **Wire your IDE and CI.** Links to how-to/ide-integration/ and how-to/ci-integration/.

## Migration plan (existing → new tree)

| Existing file | New home |
|---|---|
| `README.md` (root) | rewrite — front door only |
| `THESIS.md` (root) | `docs/explanation/thesis.md` |
| `ARCHITECTURE.md` (root) | `docs/explanation/architecture.md` |
| `PRODUCT.md` (root) | `docs/explanation/product.md` |
| `PITCH.md` (root) | merge into README + `docs/explanation/thesis.md` |
| `LANDING.md` (root) | replaced by `docs/index.md` |
| `MANIFESTO.md` | `docs/explanation/manifesto.md` (consider whether it survives at all) |
| `RETROSPECTIVE.md`, `POSTMORTEM.md`, `SIGNALS.md`, `SPEC.md` | move to `docs/internal/` (these are project-meta, not user-docs) |
| `docs/QUICKSTART.md` | DELETE (it's a 9-line tombstone) |
| `docs/getting-started.md` | rename to `docs/tutorials/rust.md`, scope it explicitly to Rust |
| `docs/per-language-status.md` | `docs/reference/per-language-status.md` |
| `docs/lift-adoption-paths.md` | split: per-source-library content → `docs/reference/per-adapter-coverage.md`; adoption guidance → `docs/how-to/lifting-existing-annotations.md` |
| `docs/library-integration.md` | `docs/how-to/publishing-a-proof.md` (after restructuring) |
| `docs/LOGGING.md` | `docs/operations/troubleshooting.md` (or its own ops/logging.md) |

## README rewrite (front door, not manual)

```markdown
# ProvekIt

Verify a petabyte of behavior by comparing 64 bytes.
ProvekIt is a content-addressed protocol for behavioral verification across the dependency graph.

## I want to...

| Goal | Start here |
|---|---|
| Try it in Rust | docs/tutorials/rust.md |
| Try it in TypeScript | docs/tutorials/typescript.md |
| Try it in Python | docs/tutorials/python.md |
| ... (one row per shipping kit) | ... |
| See the polyglot demo | docs/tutorials/polyglot-stack.md |
| Get red squigglies in my IDE | docs/how-to/ide-integration/overview.md |
| Add my language | docs/contributing/porting-to-a-new-language.md |
| Write a lift adapter | docs/contributing/writing-a-lift-adapter/ |
| Understand the thesis | docs/explanation/thesis.md |
| Compare to SLSA / Sigstore / SBOM | docs/explanation/compared-to/ |
| Look up a CLI flag | docs/reference/cli/ |
| Look up an IR node | docs/reference/ir/ |

## What is ProvekIt?

[one-screen explanation, 3-4 paragraphs, ends with link to docs/explanation/thesis.md]

## Status

- Protocol catalog: v1.1.0 (CID `blake3-512:9d57c5e4...`)
- Languages: docs/reference/per-language-status.md
- Conformance: every kit's mint must match a pinned catalog CID; CI gates on this.

## Building from source

[link out to docs/contributing/build.md — full polyglot Make targets, system dependencies]
```

The README's job is routing. The manual lives in `docs/`.

## Phasing (what to write first)

**Tier 0 — bleed-stop, days, not weeks:**
- README rewrite per above.
- `docs/index.md` "what brought you here?" landing.
- Delete `docs/QUICKSTART.md`.
- Move existing root docs (THESIS, ARCHITECTURE, PRODUCT) into `docs/explanation/`.
- Rename `docs/getting-started.md` → `docs/tutorials/rust.md`, scope title, add per-language siblings as stubs.

This alone closes the "polyglot thesis vs. Rust-only docs" credibility gap.

**Tier 1 — close the consumer gap, weeks:**
- Per-language tutorials for every shipping kit: TypeScript, Python, Java, C#, Ruby, Zig (Rust already done).
- `docs/how-to/ide-integration/overview.md` matrix + per-editor configs for the five shipping LSPs.
- `docs/contributing/porting-to-a-new-language.md` + `docs/contributing/writing-a-kit/` (six-step series).
- `docs/tutorials/polyglot-stack.md` — the worked cross-domain demo. This validates the thesis.
- `docs/explanation/compared-to/` — at minimum SLSA/Sigstore/in-toto/SCITT and Kani/Prusti/Coq comparisons.

**Tier 2 — depth, quarter:**
- How-to recipes (CI cookbook per platform, publishing, consuming, key management, debugging handshake).
- Reference build-out (CLI subcommand pages, IR grammar pages, proof bundle pages, handshake tier pages).
- Examples library (parseInt cross-domain, supply-chain attack demo, npm-package-with-zod).
- Security tree (threat model, what binaryCid catches, solver trust, adapter trust).
- `docs/contributing/writing-a-lift-adapter/` (five-step series).

**Tier 3 — full coverage:**
- Operations (implication server, monitoring, key rotation).
- Governance (protocol versioning, deprecation, conformance claims, philosophy).
- Reference contracts library — its own curated tree, eventually its own repo.
- Per-prover-backend contributing docs.
- Per-editor IDE integration guides for all five plugins × four editors.

## Maintenance posture

**Single source of truth, no duplication.** The CDDL grammar is the source of truth for IR; reference docs cite the grammar by CID. The kit standard is the source of truth for kit obligations; contributing docs cite it by CID. The protocol catalog is the source of truth for what v1.1.0 means; every doc that makes a version-sensitive claim cites the catalog CID it documents.

**Version coupling.** Don't fork docs per version. Have docs at HEAD reference protocol CIDs explicitly. A reader can check `provekit version` and see whether they match.

**Per-language tutorials use uniform shape.** Six steps, same headings, same example structure. Polyglot devs can context-switch without re-learning IA.

**Examples are executable.** Each `docs/examples/*` should ship a runnable demo in `examples/`. Doc and code stay in sync because the doc is built from / pinned to the example's CID.

**Reference contracts are a product, not docs.** They need a curation process, a versioning story, a governance model. The `reference-contracts/` tree is a placeholder for what should eventually be its own repo.

## What this proposal is NOT

Not 100 docs to write today. The IA is the deliverable; the writing is phased. Tier 0 is days of work. Tier 1 is weeks. Tier 2 and 3 grow with adoption.

Not a freeze on existing content. Some root docs (THESIS, ARCHITECTURE) are good and just need to move. Some (QUICKSTART tombstone) need to die. Most of the gap is net-new files in trees that don't exist yet.

Not a rebrand. The thesis stands. The IA serves the thesis instead of undercutting it.
