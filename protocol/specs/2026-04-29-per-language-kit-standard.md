# Sugar: per-language kit standard

> Author: shared session 2026-04-29 (T + Claude). The interface every
> host-language kit must implement to participate in the framework.

## Why this spec exists

The framework is invariant under host-language choice; the work
clothes are local. Every host language needs its own complete kit:
IR library, AST canonicalizer, LLM prompt set, producer integrations,
diagnostic translator, IDE integration. Without a fixed contract,
kits cannot be authored independently, kits cannot interoperate via
the swarm, and "point sugar prove at a Rust repo and it just
works" cannot be achieved.

This is LSP's architecture inverted to verification. LSP defined a
universal protocol; each language community owns its language server.
Sugar defines a universal proof substrate; each language community
owns its kit.

This spec fixes:
- The components every kit MUST provide.
- The contract each component implements (interfaces, not
  implementations).
- Kit identity, versioning, distribution, and content-hashing.
- The kit registry and how the framework discovers kits.
- Reference sketches for Rust and COBOL kits to prove the pattern
  generalizes from "high-end strongly-typed language" to "decades-old
  weakly-typed language."

## What a kit IS

A kit is a content-addressed bundle of language-specific
implementations of the framework's universal interfaces. A kit binds
to a host language identified by:
- A canonical name (`typescript`, `rust`, `cobol`, `lisp`, …).
- A set of file extensions and language-server identifiers it claims
  to handle.
- A canonical kit version (semver) and content-hash (sha256-prefix-32).

A kit's CID is the content-hash of its canonical manifest plus its
component artifacts. Two kits with byte-identical components have the
same CID; mementos produced under a kit reference the kit's CID for
reproducibility.

## Components every kit MUST provide

```yaml
kit:
  name: <host-language-name>      # canonical, lowercase
  version: <semver>
  cid: <hex32>                     # computed by the framework at load
  
  components:
    irLibrary: <component-ref>     # the host-language authoring surface
    astCanonicalizer: <component-ref>
    promptSet: <component-ref>
    producers: [<component-ref>]   # 1+ host-language producer integrations
    diagnosticTranslator: <component-ref>
    ideIntegration: optional <component-ref>
    fileExtensions: [<string>]     # what files this kit handles
    
  metadata:
    description: <string>
    maintainers: [<string>]
    license: <SPDX-id>
    sourceRepo: <URL>
    publishedKey: <CID>            # ed25519 public key memento for kit signing
```

Each component is itself a content-addressed artifact. The kit's CID is
deterministic over its component CIDs, so kits with the same components
have the same CID across publishers.

### 1. IR library

The host-language authoring surface for IR primitives. What developers
import to express invariants in their host language.

**Required exports** (named by their semantic role; surface syntax
varies by language):

- `contract(name, { pre?, post?, inv?, outBinding? })` constructor
  — creates a behavior contract for a function-shaped binding,
  carrying any combination of precondition, postcondition, and
  inductive invariant. At least one of `pre`/`post`/`inv` MUST be
  provided. `outBinding` defaults to `"out"`. The contract is the
  unit of behavior specification per the memento envelope grammar
  spec's contract role.
- `must(name, formula)` convenience alias — equivalent to
  `contract(name, { pre: formula })`. Retained because the
  precondition-only case is overwhelmingly common; the alias keeps
  the simple case syntactically compact while the full surface
  goes through `contract`.
- `forAll`, `exists` quantifiers over typed sorts.
- `implies`, `and`, `or`, `not` boolean connectives.
- `eq`, `gt`, `gte`, `lt`, `lte`, `ne` — comparison primitives.
- `out()` — references the function's return value within a `post`
  formula. Compiles to a `var` term whose `name` equals the enclosing
  contract's `outBinding`.
- Branded type helpers: `NonZero<T>`, `NonEmpty<T>`, `Sorted<T>`,
  `Validated<T, schema>` — the type-dialect surface.
- Scope helpers: `function`, `module`, `class`, `transition`, etc. —
  for binding contracts to code locations.

The IR library is the *only* required new dependency a kit imposes
on the host language. It's installable as the host's normal package
(npm, cargo, etc.).

**Acceptance:** a developer in the host language can write at least
the following kinds of contract without leaving the host language's
type system or syntax:
- A precondition-only contract: `must("non-empty-input", forAll(s => length(s) > 0))`.
- A pre + post contract: `contract("parseInt", { pre: ..., post: forAll(s => gt(out(), num(0))) })`.
- A type-dialect contract using a branded type (`NonZero<T>`).
- A library-dialect contract using a quantifier (`forAll(x => ...)`).
- A composed contract using boolean connectives.

The library carries no implementation — it produces *data structures*
(IR formulas as host-language values) that downstream components
canonicalize and translate.

### 2. AST canonicalizer

Translates host-language AST + IR-formula values into the canonical
FOL form whose hash is the propertyHash and whose structure
producers compile to backend syntax.

**Required interface:**

```typescript
interface AstCanonicalizer {
  /** Hash a piece of code identified by an AST node. */
  bindingHashFromAst(node: HostAstNode): string;  // hex16
  
  /** Hash an IR formula expressed in the host's IR library. */
  propertyHashFromFormula(formula: HostIrValue): string;  // hex16
  
  /** Canonicalize an IR formula to the AST that downstream producers consume. */
  formulaToCanonicalAst(formula: HostIrValue): CanonicalFolAst;
  
  /**
   * Identify the scope of a binding — file, function, region, module —
   * so the bindingHash captures "what code this claim is about" rather
   * than "what arbitrary span the AST node happens to cover."
   */
  scopeOf(node: HostAstNode): BindingScope;
}
```

**Cross-language guarantee:** two host-language formulas expressing
the same FOL claim canonicalize to AST-byte-identical
`CanonicalFolAst`. Their `propertyHash` matches. The framework's
swarm-level cross-validation works because Rust's `forall(x: u32)
where x > 0` and TypeScript's `forAll<number>(x => x > 0)` and
COBOL's `IF X > 0` produce the same canonical formula, hence the
same propertyHash.

**Acceptance:** a property authored in two different host languages
expressing the same logical claim produces the same propertyHash.
The framework's `crossValidate()` operation finds them as comparable
mementos.

### 3. Prompt set

The LLM-producer prompts customized for the host language. Each
prompt is itself a content-addressed artifact (a memento with
`kind: prompt`).

**Required prompts:**

- `intent-from-diff` — given (diff, commit-message, linked-tickets),
  propose an IR formula expressing the intent of the change.
- `repair-from-failure` — given (intent, code, failed memento with
  counterexample, IR formula), propose a fix.
- `formula-cross-validate` — given (proposed formula, intent, code),
  produce a verdict on whether the formula expresses the intent.
- `idiom-aware-author` — author IR formulas using host-language
  idioms (typeclasses for Haskell, traits for Rust, ABEND for COBOL,
  `Carp::Assert` for Perl, …).

Each prompt is a teaching prompt (per the project's prompt-writing
discipline): it explains stakes, gives good/bad examples, names what
to read, makes the quiet part loud, includes a cut list. Prompts are
versioned by content-hash; a kit that ships an updated prompt bumps
the prompt's CID and the kit's CID.

**Acceptance:** an LLM running with the kit's prompts on the kit's
example fixtures produces well-formed IR formulas expressing the
intent of representative diffs. Quality is measured by cross-
validation agreement across at least two LLMs run against the same
prompts.

### 4. Producer integrations

The host language's existing tools wrapped as framework producers.
Each producer is a Stage (or, in rare cases, an Action; see the
Stages-vs-Actions spec).

**Required minimum:** at least one type-checker producer or
equivalent compile-time correctness checker. Without this, the
host's mandate-able floor (`tsc passes` / `cargo check passes` /
elaborator succeeds) cannot be enforced.

**Recommended:** lint, test runner, formal prover, behavioral
property test runner, SAST.

Each producer:
- Implements `Stage<I, O>` (or `Action<I, R>`).
- Emits mementos in the universal claim envelope schema.
- Uses a producer-id of the form `<tool>@<version>` (e.g., `tsc@5.4.2`,
  `clippy@0.1.84`).
- Emits the appropriate evidence variant for its tool category
  (`type-check-pass`, `lint-pass`, `test-pass`, `z3-model`, etc.).

**Acceptance:** running the kit's producers against a representative
codebase produces valid mementos that compose into a walkable proof
DAG. The mandate-able floor (`<host's compiler> passes`) is
expressible as a composite memento that holds when all required
producers' verdicts are `holds`.

### 5. Diagnostic translator

Converts memento failures to the host language's native diagnostic
format so developers see violations in their familiar tooling
register.

**Required interface:**

```typescript
interface DiagnosticTranslator {
  /** Memento → host's diagnostic format. */
  translate(memento: Memento, context: DiagnosticContext): HostDiagnostic;
  
  /** Optional: surface a memento as an LSP-protocol diagnostic. */
  toLspDiagnostic?(memento: Memento): LspDiagnostic;
  
  /** Optional: surface as host's compiler-error format. */
  toCompilerError?(memento: Memento): CompilerErrorString;
}
```

For TypeScript: red squiggle in tsserver protocol. For Rust:
`error[E0SUGAR]: violation: <property>` in rustc's diagnostic
format. For Lisp: condition raised at the REPL. For COBOL: ABEND code
in JCL output. For Perl: `Carp::Assert` failure in the dev's familiar
format.

**Acceptance:** a violated memento surfaces as a diagnostic in the
host language's IDE / REPL / terminal in a way that's
indistinguishable in register from the host's native errors.

### 6. IDE integration (optional but strongly recommended)

A language-server-protocol implementation (or editor-extension
equivalent) that surfaces mementos as live diagnostics, displays the
proof DAG inline, runs cross-validation in-IDE, and proposes IR
formulas as code actions.

For most host languages this is a thin layer over the diagnostic
translator. For some (Lisp's REPL, COBOL's IDE landscape) the
"IDE" surface is more bespoke.

**Acceptance:** the developer sees Sugar's verdicts in their
editor at edit time, with hover details that name the producer that
verified each claim and the witness it provided. Code actions surface
LLM-proposed IR formulas; the developer accepts/rejects with one
keystroke; accepted proposals are committed as mementos.

## Kit identity, distribution, content-hashing

A kit is published as a content-addressed bundle. The bundle's
manifest:

```yaml
manifest:
  kit-name: rust
  kit-version: "0.1.0"
  schemaVersion: "1"
  
  components:
    irLibrary: { artifact: sugar_ir-0.1.0.tar.gz, cid: hex32 }
    astCanonicalizer: { artifact: canonicalizer-0.1.0.wasm, cid: hex32 }
    promptSet: { artifact: prompts-0.1.0.tar.gz, cid: hex32 }
    producers:
      - { name: rustc, artifact: rustc-producer.wasm, cid: hex32 }
      - { name: clippy, artifact: clippy-producer.wasm, cid: hex32 }
      - { name: miri, artifact: miri-producer.wasm, cid: hex32 }
    diagnosticTranslator: { artifact: diag-rust-0.1.0.wasm, cid: hex32 }
    ideIntegration: { artifact: rust-analyzer-extension.tar.gz, cid: hex32 }
  
  fileExtensions: [".rs"]
  
  signedBy: <maintainer-public-key-cid>
  signature: <ed25519-signature-of-manifest>
```

The kit's CID is `sha256(canonicalize(manifest))`. The manifest is
canonicalized as JSON sorted-keys; the bundle is identified by that
hash.

Distribution is via the swarm. A kit's CID is announced; consumers
fetch the components by their individual CIDs; integrity is verified
by hash-comparison. Multiple maintainers can publish "the same" kit
(byte-identical) and converge on the same CID; competing kits for
the same host language have different CIDs and exist in parallel.

Cross-kit cross-validation works because every kit produces mementos
in the universal claim envelope. Mementos from `rust-kit-A@0.1.0` and
`rust-kit-B@0.2.0` for the same property compose; if they disagree,
that's a quality signal.

## Kit registry

The framework discovers kits via a registry. The registry is itself
a memento — content-addressed, signed by the framework's well-known
key (or by community consensus).

```yaml
kit-registry:
  version: 1
  publishedAt: 2026-04-29T...
  kits:
    - name: typescript
      latestVersion: "0.5.2"
      latestCid: <hex32>
      maintainer: <pubkey-cid>
      fileExtensions: [".ts", ".tsx"]
    - name: rust
      latestVersion: "0.1.0"
      latestCid: <hex32>
      maintainer: <pubkey-cid>
      fileExtensions: [".rs"]
    - name: cobol
      latestVersion: "0.0.1-alpha"
      latestCid: <hex32>
      maintainer: <pubkey-cid>
      fileExtensions: [".cbl", ".cob", ".cobol", ".cpy"]
    - name: lisp
      ...
```

The framework does not ship a global kit catalog. Project
and user configuration register kit aliases and plugin surfaces; manifests
then describe the RPC command for each surface. A future `sugar kits
list` command must enumerate configured or discovered entries, not a
compiled-in language list. `sugar kits install rust` would fetch and
verify the rust kit, then write project/user config and lock metadata.
`sugar prove` does not parse source extensions to decide language
semantics; configured kits parse their own languages and speak RPC to the
language-agnostic CLI.

A repo can pin specific kit versions in `.sugar/kits.lock`:

```yaml
typescript: { version: "0.5.2", cid: hex32 }
rust: { version: "0.1.0", cid: hex32 }
```

Pinning ensures reproducibility — re-running the framework against
the same repo at the same kit lock produces identical mementos.

## Reference sketch: Rust kit

To prove the pattern works for a strongly-typed modern language:

```yaml
kit-name: rust
components:
  irLibrary:
    name: sugar_ir
    crate-published-at: crates.io/sugar_ir
    exports:
      - property!{} proc-macro
      - forall!{} proc-macro
      - NonZero<T>, NonEmpty<T>, Sorted<T> branded types
      - assert::not_eq!, assert::greater_than! macros
  
  astCanonicalizer:
    parses-via: syn (the standard Rust syntax tree library)
    canonicalizes-via: walks the syn AST + the proc-macro-expanded
      sugar_ir invocations; emits FOL AST in framework-canonical form
  
  promptSet:
    intent-from-diff: teaches LLM about Rust idioms — ownership,
      lifetimes, traits, the typestate pattern, error handling
      via Result, Option, panic-vs-Err, clippy's preferences
    repair-from-failure: includes Rust's specific failure shapes
      (lifetime mismatch, borrow checker failures, type errors)
  
  producers:
    - rustc: every successful `cargo check` is a memento with
        kind: type-check-pass, producedBy: rustc@1.84
    - clippy: every clean `cargo clippy --pedantic` is a memento
        with kind: lint-pass
    - miri: runtime-soundness memento for unsafe code
    - kani: SMT-backed property verification → z3-model / z3-unsat
        evidence variants
    - cargo-test: behavioral mementos
    - proptest / quickcheck: property-test mementos with witness inputs
  
  diagnosticTranslator:
    surfaces-as: rustc-style diagnostic with error code E0SUGAR
      and `note: violation: <property>` lines
  
  ideIntegration:
    via: rust-analyzer extension that consumes the framework's
      memento store and surfaces violations in VS Code / Helix /
      JetBrains
  
  fileExtensions: [".rs"]
```

The Rust kit's distinctive features:
- Strong type system carries most of the verification load. The
  mandate-able floor (`cargo check + clippy::pedantic` passes) is
  *much* richer than TypeScript's `tsc --strict`.
- Procedural macros are the natural meta-IR mechanism — `forall!{}`
  expands to a value-level IR formula at compile time.
- Branded types like `NonZero<T>` integrate cleanly with Rust's type
  system and trait coherence.
- Existing tools (rustc, clippy, miri, kani) are first-class
  producers; nothing new to install for the developer.

## Reference sketch: COBOL kit

To prove the pattern works for a 60-year-old weakly-typed legacy
language with a vastly different cultural and tooling ecosystem:

```yaml
kit-name: cobol
components:
  irLibrary:
    distributed-as: SUGAR.cpy COPYBOOK plus a small set of
      conventional code patterns
    primitives:
      - 'PERFORM ASSERT-X' paragraph convention with named bug
          conditions in 88-level data definitions
      - taint markers on data-name fields via standardized
          comments / preprocessor macros
      - violation reporting via standardized DISPLAY + ABEND-ROUTINE
  
  astCanonicalizer:
    parses-via: GnuCOBOL's parser or a vendor's COBOL AST library
      (IBM, Micro Focus offer them)
    canonicalizes-via: the canonicalizer normalizes COBOL's many
      equivalent expressions of "this should be true" — IF/THEN
      conditions, EVALUATE WHEN clauses, REDEFINES contracts,
      LEVEL-88 condition names — into the same FOL form
  
  promptSet:
    intent-from-diff: teaches LLM about COBOL idioms — DIVISION
      structure, COPYBOOK semantics, EXEC SQL, CICS preferences,
      ABEND conventions, mainframe-specific conventions like
      JCL change tickets being the primary diff metadata
    repair-from-failure: includes mainframe failure shapes
      (S0C7 data exception, file status codes, EXEC SQL
      sqlcodes)
    legacy-archive-mining: extra prompt for ingesting ticket
      archives (IBM Service Management, Endevor change records,
      Panvalet histories) since git is not the primary VCS for
      most legacy COBOL
  
  producers:
    - cobol-compiler (vendor-specific): every clean compile is a
        type-check-pass-style memento
    - runtime-instrumentation: behavioral mementos from prod /
        regression suite execution traces
    - taint-analysis: pattern-match memento via static taint flow
        rules
    - z3-via-translator: arithmetic-bound assertions translated to
        SMT-LIB by the canonicalizer; Z3 verifies; produces
        z3-model / z3-unsat mementos
    - mutation-test: behavioral memento for "test fails on
        mutated original" pattern
    - regulatory-rule-pattern-match: SAST-style producer that fires
        on patterns mandated by SOX, FFIEC, PCI-DSS, etc.
  
  diagnosticTranslator:
    surfaces-as: ABEND code in JCL output with detail in
      SYSOUT / SYSPRINT; integrated into IBM watsonx Code
      Assistant or vendor IDE
  
  ideIntegration:
    via: IBM Z Open Editor extension; Micro Focus Visual COBOL;
      web-based mainframe modernization tooling
  
  fileExtensions: [".cbl", ".cob", ".cobol", ".cpy", ".jcl"]
```

The COBOL kit's distinctive features:
- The host language has almost no type system. Verification load
  shifts heavily to producers: runtime instrumentation, taint
  analysis, regulatory pattern matchers, LLM proposal +
  cross-validation.
- The IR substrate is COBOL's own conditional logic: `IF / EVALUATE /
  PERFORM ASSERT-X` patterns are the IR. The SUGAR.cpy COPYBOOK
  provides standardized macros and 88-level condition names so the
  framework can find them.
- The diff source is often NOT git. Mainframe shops use Endevor,
  Panvalet, ChangeMan, IBM Service Management. The kit's
  legacy-archive-mining workflow ingests these and converts to a
  normalized diff form before the LLM proposes intent.
- Diagnostic register is mainframe-native: ABEND codes, JCL output,
  SYSPRINT. Developers experience the framework through the
  mainframe-conventions register.
- Producer pool leans heavily on LLM-based intent extraction +
  cross-validation because there's no rich type system to lean on.
  This is exactly where the "even the dumbest LLM can write COBOL"
  insight pays off — the LLM producer pool is the load-bearing
  verification surface.

The two reference sketches together prove the kit standard
generalizes: a Rust kit (modern, strongly-typed, rich tooling) and a
COBOL kit (legacy, weakly-typed, alien tooling) both fit the same
interface. Anything in between (TypeScript, Python, Lisp, Perl, Java,
C, …) trivially fits.

## Acceptance test for the kit standard

The standard is correct when:

1. A Rust kit can be authored against this spec, produces valid
   mementos in the universal claim envelope, and `sugar prove`
   against a Rust repo "just works."
2. A COBOL kit can be authored against this spec (substantially more
   work for the producer pool, but no architectural changes
   required), produces valid mementos, and `sugar prove` against
   a COBOL repo "just works."
3. Mementos from the Rust kit and the COBOL kit are
   cross-comparable at the wrapper level. A property expressible in
   both kits produces matching `bindingHash`/`propertyHash` (within
   the limits of canonical form equivalence) and divergent verdicts
   surface as cross-validation signals.
4. A new kit (e.g., Python) can be authored without modifying the
   framework's universal core. Only the kit's components are new.
5. The kit registry mechanism distributes kits via content-hashed
   bundles; consumers verify integrity by hash-comparison; multiple
   competing kits for the same language can coexist; pinning
   ensures reproducibility.

When all five hold, the framework has achieved its acceptance bar:
**point sugar prove at a Rust codebase or a COBOL codebase, and
it just works.**

## What this enables

With the kit standard fixed:
- Rust kit, COBOL kit, Python kit, Lisp kit, Perl kit can be authored
  in parallel by independent communities.
- Each kit's producer pool is its own; the framework's universal
  core stays small.
- Cross-language cross-validation (the same property expressed in
  multiple host languages, verified by each language's producer
  pool) becomes mechanical.
- The mainframe-first market is unblocked: a Tier-1 bank's pilot can
  fund the COBOL kit's authorship, and the framework's core stays
  invariant.
- The whitepaper's central claim — *the framework rides every host
  language ever made* — is operationally verifiable, not just
  rhetorically asserted.

The kit standard is the acceptance bar made concrete.
