# The Path to Default-On

**The strategic companion to `docs/launch/the-pieces-on-the-table.md`.** The architectural derivation establishes what the system is. This document establishes how it wins.

By T. Savo, 2026-05-03

## The thesis

Sugar wins when it is the on-by-default compile-time correctness check in every new project. Not as a feature a developer chooses to install. Not as an architectural commitment a team adopts after evaluation. As the editor's red squiggle that surfaces predicate-level violations the moment the developer types a call site, in the IDE they already use, against the language they already write.

The architectural derivation in `the-pieces-on-the-table.md` shows the system can do this. This document shows why it has to.

## The conceptual shift that makes adoption tractable

A traditional FFI declaration is a **requirement**. The C `extern void foo(int);`, the Rust `#[link]` attribute, the Java JNI signature, the Python `ctypes.CFUNCTYPE`. Without the declaration the compiler refuses; the toolchain enforces presence; the bridge is part of the source; the developer authors it; absence breaks the build.

A Sugar bridge is a **demand**. The caller's postcondition implicitly demands that the callee's precondition is established at the call site. The demand is observable in the source (the annotation on the callee, the flow of data into the caller's argument), but nothing in the source declares it. The lifter extracts it. The linker records it. The bridge is a parallel artifact running alongside the binary, in the `.proof` bundle, content-addressed.

That shift is load-bearing for adoption.

### Retrofit is automatic

Any existing codebase that has annotations becomes a Sugar subject the moment the lifter walks it. Java `@NotNull`, C# `[Range]`, Python type hints, Rust `assert!`, Go validator tags, OpenAPI schemas, ActiveModel validators. The developer adds nothing. The developer changes nothing. The developer doesn't necessarily know Sugar is running. The substrate observes demands and produces a `.proof` recording what it found.

This is the difference between a tool that requires a green-field project and a tool that turns up productive work on day one against legacy code with annotations going back a decade.

### No vendor lock

Removing Sugar from a project is deletion of the `.sugar/` directory and the LSP plugin. The source code is untouched. The annotations stay because they were always going to stay (they are host-language artifacts). The bridges go away because they were never authored. Nothing in the host language ever depended on them. Exit cost is zero.

The exit-cost argument is for the migration phase. The win condition is the default-on phase, where exit doesn't come up because the tool is part of the daily workflow and nobody removes a working compile-time check.

### Federated trust over the same source

Two consumers can lift the same source and derive different bridges if they apply different policies. A strict consumer demands `pre = (n > 0)`; a permissive consumer accepts `pre = (n != null)`. Both are valid demands; both produce different `linkBundleCid` values; both content-address their own posture. The substrate doesn't pick which is right. It records each consumer's view and lets them compose under §10 of the manifesto.

This is what enables Sugar to be useful at the org-policy level (CI gates per team), the supply-chain level (consumer pins per dependency), and the personal level (an individual developer's stricter local checks) without forking the source or fragmenting the ecosystem.

### Backwards-compatibility shifts under the framing

Removing a `@NotNull` annotation from a Java method does not break upstream callers' compile. The compile still succeeds because the annotation was never a syntactic requirement. What it does is invalidate upstream callers' previously-derived bridges: their `targetContractCid` no longer resolves to a contract demanding non-null. The linker emits `linker-error` mementos pointing at the upstream call sites. The downstream change becomes visible to the upstream substrate in a way the language toolchain cannot make visible. Annotation removal becomes a contract change, with tooling to track it across the dependency graph.

## The historical pattern

Tools that became default-on share five properties. Sugar has the architecture for all five; the work to ship each is concrete and bounded.

### ESLint

ESLint became default in JavaScript projects because IDEs auto-installed it for JS files and frameworks bundled `eslint-config-{react, next, vue}`. Once the editor surfaced linting errors before the test suite did, ESLint stopped being a choice. The path was: IDE integration first, framework bundling second, scaffold defaults third.

### Prettier

Prettier became default by ending a real config fight. Once `prettier --write` was wired into pre-commit hooks, opting out meant fighting your own team. The path was: opinionated defaults that didn't ask for input, then editor-on-save integration, then pre-commit hook templates.

### Black

Black became default in Python by being opinionated and PEP 8-aligned, then by getting bundled into every modern Python scaffold (`pyproject.toml` boilerplate, GitHub Action templates, `mypy` configs). The path was: zero-config opinion, then ecosystem template inclusion.

### rust-analyzer

rust-analyzer became default because rustup ships it and IntelliJ + VS Code auto-detect it. Rust developers do not choose to install it; it is just there. The path was: official-toolchain bundling, then IDE auto-detection, then language-server-protocol standardization.

### TypeScript

TypeScript became default in JavaScript projects because the compiler's errors are about real bugs developers wanted catching. The friction of writing types was less than the friction of debugging without them. The path was: sufficient bug-catching value to overcome the writing friction, then framework adoption (Angular, Next, NestJS), then IDE first-class support.

The common shape: the tool ships as part of the language scaffold or the IDE; the false-positive rate is low enough to trust on every keystroke; the speed is fast enough to leave on at typing time; the bugs it catches are real ones that hurt before shipping.

## The five properties needed for default-on

Sugar's architecture supports each. The work to ship each is concrete.

### 1. LSP push at typing speed

This is the cake, not the icing. It is the thing that puts the diagnostic in front of the developer in their normal workflow, on the keystroke. Without LSP, Sugar is a one-off audit tool. With LSP, it is the editor's compiler.

The architecture supports it: the linker derives bridges incrementally; only changed CIDs need re-derivation; the LSP plugin in each kit pushes `linker-error` mementos as `publishDiagnostics`. The work to ship it: per-kit LSP plugin polish (today gaps in rust LSP plugin are closed via PR #117; python and ruby had keyword/import bugs closed via PR #116; zig and swift have residual gaps).

Per-host FFI resolvers are the kit-local frontends to the universal cross-language linker. Each kit lifter ships its own FFI resolver alongside its LSP plugin: Go's cgo resolver parses the cgo preamble and maps `C.foo()` calls to the correct kit prefix (PR #127); Python's ctypes resolver (PR #131) parses `CDLL`, `cdll.LoadLibrary`, `PyDLL`, and `WinDLL` load paths to emit kit-prefixed `targetSymbol` values; Java's JNI resolver (PR #132, spec #114 R3) parses `System.loadLibrary` and `System.load` calls to map `native` method call sites to kit-prefixed `targetSymbol` values; .NET's P/Invoke resolver (PR #133, spec #114 R3) walks `[DllImport]` and `[LibraryImport]` attributes to map `extern` call sites to the target kit; and Ruby's Fiddle and ffi gem resolver (spec #114 R3) walks `dlload`/`extern` Fiddle declarations and `ffi_lib`/`attach_function` ffi gem declarations to emit kit-prefixed `targetSymbol` values, with renamed bindings correctly tracking native names through the alias. The resolver's output is a kit-prefixed `targetSymbol` (e.g. `rust-kit:foo`) that the linker resolves against the union of all loaded contracts. The linker itself is language-agnostic and unchanged; only the kit-local resolver changes per host language. This is the architectural reason cross-language predicate verification does not require a special case for each language pair: the resolver is the kit-specific translation layer, and the linker is the universal compositor.

### 2. False-positive rate under control

Predicate-level verification is conservative by default. "I cannot prove `post_caller ⊃ pre_callee`" is a different signal from "the call is wrong." Tooling needs to express the difference clearly in the diagnostic, with concrete next-action guidance: add a guard, tighten the contract, annotate the postcondition.

The opacity-manifest work that landed in PR #110 is the start of this surface. Vacuous-true predicates get a manifest entry instead of producing a noisy red squiggle. The work to ship: extend opacity-manifest coverage across all lift adapters, write the diagnostic-formatter that produces actionable next-action text, run on dogfood codebases until the false-positive rate is empirically low.

### 3. Speed at typing time

Content-addressing is the structural answer. Hash-cached verification, only changed CIDs touched, the rest is `O(1)` lookups. The substrate's verification cost bounds at hash count per the architectural derivation, §12 of the manifesto.

The work to ship: the linker pass benchmark on real-world projects. The first concrete data point comes from tonight's rust↔go smoke fixture; if its single linker derivation is sub-100ms, the LSP push reuses the same hash cache and inherits the latency budget. If not, the optimization work is bounded (parallel discharge across call edges, persistent CID cache between runs).

### 4. Scaffold integration

Once the architecture is proven and LSP works, the wedge is per-language scaffold defaults: `cargo sugar init`, `go mod init` with sugar defaults, `dotnet new` templates that include the `.sugar/` layout, GitHub Actions templates that include `sugar prove` as a CI gate.

Each scaffold integration is a small follow-up. Once one major language ships it as a default, the others follow because developers ask for parity. The natural first target is Rust: Sugar owns the contract substrate in Rust; rust-analyzer is the most extensible LSP backend; the cargo plugin story is well-trod.

### 5. Bundled with the language toolchain

The far-end aspiration: rustup, the Go toolchain, dotnet SDK, etc. include the per-kit lifter and LSP plugin out of the box. At that point the developer never installs Sugar; it just turns up in their IDE the same way rust-analyzer does. That is the on-by-default state. It takes years and integrations and a track record of catching real bugs to get there, but the architectural path to it is unobstructed because the substrate doesn't ask for source changes.

## The roadmap implied by the five properties

The order is not interchangeable. The five properties have a forced sequence because each enables the next.

**Phase 1 (now, tonight): architectural proof.** The rust↔go smoke fixture demonstrates the linker pass end-to-end. The `.proof` bundle records the predicate-level cross-language correctness verification. Sir reviews; the architectural claim is empirically validated.

**Phase 2 (next two weeks): LSP push polish.** Per-kit LSP plugin closes its remaining gaps. Diagnostic surface formatter implemented. The IDE shows red squiggles for cross-language contract violations on the keystroke. Sir tests on a polyglot project of his own choosing.

Step 3b of the LSP+linker path is complete (PR: `feat(lsp): daemon-client mode in sugar-lsp`): `sugar-lsp` now has a daemon-client mode alongside its existing per-plugin subprocess mode. When `--daemon-socket <path>` is passed, `did_open` / `did_change` route through `sugar-linkerd` instead of the per-plugin path. `publishDiagnostics` delivers the daemon's `LinterError` set to the editor. The rust IDE path from source change to red squiggle is now end-to-end wired for the `rust` kit. Multi-kit dispatch (file extension -> kit routing) is the next follow-up step.

**Phase 3 (next two months): false-positive control.** Run on real codebases (the platform monorepo per task #132 is the natural candidate). Catalog every false positive. Each false positive is either an opacity-manifest entry, a lifter improvement, or a contract-language enrichment. The empirical false-positive rate goes down with each iteration. Goal: under 1% on a typical Java/Scala/Kotlin/Go/Python codebase.

**Phase 4 (next six months): rust scaffold integration.** Ship `cargo sugar init`. Ship `cargo sugar prove` as a CI gate. Ship the rust LSP plugin as a rustup component. Document the workflow in The Rust Programming Language and rustlang.org/learn. Get into Rust language-team discussions about toolchain inclusion.

**Phase 5 (year-plus): toolchain inclusion.** Once Rust ships Sugar as a default, propose inclusion in the next-most-receptive ecosystem. Likely candidates in order: Go (gopls already exists; cgo is the cross-language hook); .NET (csharp-ls + DataAnnotations are mature); Python (pylsp + type-hints are mature). Each successive ecosystem is easier because the architectural pattern is proven.

## The known operational issue, recorded

The rust call-edge agent's report on PR #121 surfaced cross-platform non-determinism in `sugar-lift`'s workspace scan: macOS and Linux produce different `contractSetCid` values for byte-identical source. Likely root cause is directory walk order (APFS vs ext4 `readdir` order) or path canonicalization (case-insensitive vs case-sensitive filesystems). The fix is at the lifter level: sort the file walk output, normalize paths to relative POSIX form per spec #120's Locus rules.

This is a v0 issue, not a v1 one. The architectural claim is intact within-machine. The cross-machine claim from §11 of the manifesto requires the fix; without it, two Sugar users on different platforms will compute different `linkBundleCid` values for the same project, and federated trust under spec #94 breaks across platform boundaries. Tracked as a follow-up; Docker is the operational containment if needed; the source-level fix is one PR.

## What this document is

The architectural derivation answers "what is the system." This document answers "how does it become daily." Together they bracket the launch posture: derivation establishes the architectural ground, this establishes the adoption gradient, the seven specs and the manifesto sections constitute the normative substrate underneath both.

The default-on outcome is the test of whether the architectural claim is real outside its own derivation. If Sugar becomes the editor's compiler in the language ecosystems we ship to, the architecture is empirically vindicated. If it does not, the architecture was correct but the polish or the integration or the false-positive control fell short. The win condition is operational, not theoretical, and the path to it is the five properties in sequence.

---

By T. Savo, 2026-05-03. Companion to `docs/launch/the-pieces-on-the-table.md`.
