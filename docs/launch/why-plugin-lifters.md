# The Plugin Lifter Is The Adoption Surface

Sugar's substrate is bounded. The canonical IR is finite. Content addressing is finite. The proof file format is finite. The catalog is signed and frozen. Once specced, the protocol substrate does not grow.

The universe of things people want to verify is unbounded. Software contracts. Sensor telemetry. Scientific consensus. Legal attestations. Identity assertions. FDA forms. SOC2 controls. SystemVerilog assertions. Banking transaction validity. Supply chain provenance. Medical device interlock. Aircraft control software. Insurance claim adjudication. Any line of business code that filters, joins, asserts, or aggregates data is a candidate proposition. Anyone could add a domain tomorrow.

The plugin lifter is the only architectural element that lets the bounded substrate serve the unbounded universe.

## Without the plugin lifter

Every new authoring surface gets built into the Rust CLI. Every annotation library, every test framework, every domain-specific predicate language, every notation system humans have ever invented for expressing predicates: all of it lives inside one binary.

The matrix is N languages × M annotation libraries × K test frameworks × L domain DSLs. The matrix has no ceiling. Whoever ships the Rust CLI becomes the bottleneck for every adoption decision the protocol's user base ever makes.

A pharma company wants to verify FDA-form invariants; they file a feature request and wait. A bank wants to lift its SOC2 controls; they file a feature request and wait. A chip vendor wants SystemVerilog assertion lifting; feature request, wait. The protocol's adoption ceiling is whatever the core team can ship in a quarter.

The "any verifiable proposition" claim is aspirational under this model. Operational reality is "any verifiable proposition we got around to writing a lifter for." Those are different protocols.

## With the plugin lifter

A pharma company writes an FDA-forms lifter in Python. They drop a manifest at `~/.config/provekit/lift/fda-forms/manifest.toml` and a `.provekit/config.toml` in their project root saying `[authoring] surface = "fda-forms"`. `provekit mint` discovers it, dispatches over JSON-RPC stdio, gets canonical IR back, mints. Their FDA forms become content-addressed propositions in the proof DAG. They never spoke to the protocol's authors.

A bank writes a SOC2-controls lifter. Same shape. Their auditors verify content-addressed `.proof` files instead of reviewing PDFs.

A chip vendor writes a SystemVerilog-assertions lifter. Their assertions get the same handshake algorithm that the parseInt cross-language demo uses. The verifier doesn't know it's silicon and doesn't need to know.

A legal firm writes a contract-clause lifter. Plain-English clauses become canonical IR via NLP + structured templates. The DAG records which clauses were active when an agreement was signed.

A sensor manufacturer writes a telemetry lifter. Each reading mints a memento citing the upstream calibration mementos. Tampering breaks the chain.

None of these required involvement from anyone shipping Sugar. The protocol substrate didn't grow. The plugin lifter dispatched.

## LINQ is the existence proof

Every modern host language ships proposition syntax already.

C# has LINQ. `xs.All(x => x > 0)` *is* `∀x ∈ xs. x > 0`. The translation to canonical IR is mechanical. About 200 lines of Roslyn AST walking. One person, a weekend.

JavaScript and TypeScript have `Array.filter`, `every`, `some`, `find`. Same translation. Babel AST.

Python has list comprehensions and `any` / `all`. `libcst` AST.

Java has Streams. JavaParser AST.

Kotlin has collection ops. Same.

Rust has iterator combinators. Same.

SQL has `WHERE`. Same.

Every business-logic codebase already contains thousands of lines of predicate syntax that lift mechanically into canonical first-order propositions. The contracts are already written. They live in production code as queries, filters, comprehensions, assertions, where-clauses.

Without the plugin lifter, Sugar has to ship a Roslyn-based C# AST walker plus a Babel-based JS AST walker plus a libcst-based Python walker plus a JavaParser-based Java walker plus a SQL-grammar parser plus an unbounded surface in one binary. The matrix again. Unbounded.

With the plugin lifter, each is an independent process speaking JSON-RPC. Each is a few hundred LOC of native-AST-walking + canonical IR emission. Anyone writes one in any language. The Rust CLI dispatches.

**The trojan horse goes deeper than annotation libraries.** Sugar's adoption surface includes every line of LINQ, every Stream, every Pydantic validator, every Zod schema, every assert in every test, every WHERE clause in every query. The contracts are already in the source. Lifters recognize them. Plugin lifter dispatch makes the recognition pluggable.

## The architectural punchline

Sugar has two parts.

The first part is bounded: canonical IR, content addressing, signing, the proof DAG. This is the protocol. It is finite, specced, signed, frozen.

The second part is unbounded: every host language's predicate syntax, every annotation library, every domain's notation system, every notation system not yet invented. This is the world.

The plugin lifter is the seam between the two.

Without it, Sugar is a tool: bounded protocol, bounded set of supported surfaces, bounded adoption ceiling.

With it, Sugar is a protocol in the LSP / MCP / SMTP sense: bounded substrate, unbounded ecosystem, anyone speaks it, no one owns it.

The substrate scales because it doesn't grow.

The ecosystem scales because the seam dispatches to it.

The protocol scales because the seam scales.

That is why the plugin lifter is critical.

It is the architectural mechanism that lets a finite, signed, content-addressed protocol serve every domain humans will ever want to verify, without the protocol's authors ever needing to know about those domains.

It is how Sugar becomes a substrate instead of a tool.

It is the only place in the architecture where the bounded protocol meets the unbounded universe.

Everything else (the canonical IR, the catalog, the foundation key, the verifier, the bluepaper's constant-time theorem) is the protocol substrate doing its job.

The plugin lifter is what makes that substrate worth having.
