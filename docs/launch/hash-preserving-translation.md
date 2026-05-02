# Hash-Preserving Translation

The lift layer is not a translator. It is a hash-preserving projection. N host-language annotation notations converge into ONE canonical IR, and by content addressing, semantically-equivalent annotations across languages become byte-identical at the substrate. Cross-language semantic equivalence isn't a feature ProvekIt implements. It's a property of content addressing applied to a canonical IR. This essay derives that claim from first principles in five steps.

## 1. Pluralism is structural

Every host language has many ways to express contracts. Java alone has thirteen mainstream notations: JSR-380 Bean Validation, JML, Cofoja, Hibernate validator constraints, Jackson schemas, Spring Web request constraints, Spring Security expressions, Swagger annotations, JPA constraints, OpenAPI Validator, Checker Framework qualifiers, Error Prone matchers, JUnit assertions. TypeScript has Zod, io-ts, runtime assertions, hand-rolled type guards, ArkType, valibot. Rust has `#[contract]` macros, Kani harnesses, Creusot specs, Prusti annotations, Flux refinements, doc-comment ensures/requires. Each notation captures different semantic dimensions and is loved by a different community.

ProvekIt does not pick one. It admits all. Each is a separate **lifter**, written in the host language because each needs the host's parser and AST, spawned via JSON-RPC by the Rust CLI per `protocol/specs/2026-04-30-lift-plugin-protocol.md`. A project's `.provekit/config.toml` declares which lifters it uses. The CLI dispatches files to all applicable lifters and unions the results.

This pluralism is not a feature added to the framework. It IS the framework's lift layer. The seam is built to be plural because the world is plural.

## 2. The IR is canonical

All lifters emit the same shape. A formula tree, a term tree, a sort tree. The grammar is fixed in `protocol/specs/2026-04-30-ir-formal-grammar.md`. Different host notations, same IR atoms.

The IR has a canonical encoding. JCS (RFC 8785) sorts object keys lexicographically, strips whitespace, locks integer formats. BLAKE3-512 produces a CID over the canonical bytes. The CID is the IR's identity.

When two lifters see semantically-equivalent annotations in different host languages, they produce the same IR atoms. Same IR atoms, same JCS bytes. Same JCS bytes, same CID. The framework defines the IR; lifters target it; canonicalization makes the targeting deterministic at the bit level.

## 3. Equivalence is content addressing

Take a Java annotation:

```java
public String parse(@NotNull String x) { ... }
```

And a TypeScript schema:

```typescript
const schema = z.string().nonNullable();
```

Both lifters lower the precondition on `x` to the same IR atom:

```json
{
  "kind": "atomic",
  "name": "isPresent",
  "args": [{"kind": "var", "name": "x"}]
}
```

Same JCS bytes. Same CID, say `blake3-512:abc123...`. The two annotations do not *translate to each other*. There is no bilateral converter. They do not *get compared by a translator*. There is no comparison step. They simply become the same content at the substrate, by hash.

This is what hash-preserving translation means. The translation step (host AST to IR) is per-language. The equivalence step is content addressing. Two artifacts are equivalent because their canonical bytes hash to the same CID, not because some component decided they were equivalent and emitted a binding. The substrate never reasons about cross-language semantics. It hashes bytes.

The consequence is structural. To add a new host language, you write a lifter that targets the same IR. You do not register correspondences with other languages. You do not negotiate equivalence. The IR is the only thing any lifter ever talks to, and CIDs do the rest.

## 4. Proofs compose across languages

Once the IR is canonical and content-addressed, proofs travel by CID. If Java's `parseInt(s)` is proven to satisfy the postcondition `isPresent(out)` (CID `blake3-512:abc...`), and a TypeScript function bridges to that same `parseInt`, the TS-side caller can rely on `isPresent(out)` by CID lookup against the contract memento store. No re-translation. No re-proof. The proposition is identical because the bytes are.

This is what `BridgeDeclaration` in `protocol/specs/2026-04-30-ir-formal-grammar.md` formalizes: a forward-pin from one language layer to another, where the pin is **CID equality**. Two layers do not agree on semantics. They agree on a hash. The hash is the contract.

A discharge memento that proved `isPresent(out)` for the Java implementation is admissible for a TS caller, an OCaml caller, a Rust caller, or a SQL caller, because all of them refer to the same IR atom by the same CID. The proof's binding to the proposition is a CID. The proposition's binding to the call site is a CID. Composition is hash equality, not translation.

This collapses what would otherwise be an N-by-N matrix of cross-language verifiers into a single content-addressed corpus. The IR is the universal vocabulary; proofs are vocabulary-tagged; lookup is by hash.

## 5. The IR is the universal language for contracts

The IR is not a serialization format. It is not an interchange format. It is the canonical predicate calculus for ProvekIt, independent of any host syntax.

Every way humans express "this isn't null" in any programming language gets absorbed into one IR atom with one CID. `@NotNull` in Java, `nonNullable` in TypeScript, `Option::Some` in Rust, `?T` in Zig, `std::optional` in C++, `nil` checks in Go: today they are six separate things that happen to mean the same. With ProvekIt they ARE the same, by hash, by memento, by verification. The framework absorbs the entire programming-language ecosystem into one proposition vocabulary.

The dedup is the ecosystem play. Every annotation library in every language, every test assertion, every where-clause, every type guard, every refinement, every pre/post-condition collapses into a finite alphabet of IR atoms. The alphabet is the canonical predicate calculus. The corpus is the proof DAG. The lifters are the projection.

ProvekIt does not host annotation libraries. It absorbs them. What goes in is N host notations; what stays is one canonical vocabulary, reused by every consumer who reads the substrate.

## What this means for you

If you accept this framing, the developer move is direct. When you ship a Java function with `@NotNull`, you are not writing a Java-internal artifact. You are contributing to a global content-addressed corpus where TypeScript callers, Rust callers, OCaml callers, and SQL callers can rely on your assertion **by CID lookup**, not by translation.

Your annotation is hash-equivalent to every other notation in every other language that means the same thing. The TS team that wrote `z.string().nonNullable()` and the Rust team that wrote `Option::Some` are contributing to the same atom. The discharge memento your proof producer signs against `isPresent(x)` is admissible against every CID-equal call site, in every language, forever.

You are not writing in Java. You are writing in the IR, with Java syntax. The lifter projects. Content addressing identifies. The substrate composes. That is the entire move.
