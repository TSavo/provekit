# Signal-Driven Formal Verification

provekit's architecture is signal-agnostic. The five-phase pipeline — dependency graph, context assembly, derivation, classification, axiom application — works against any signal of programmer intent. The log statement is the first signal source. It is not the only one.

## Signal Layers

### Layer 1: Log Statements (Intent Signals)

**What:** `console.log`, `logger.info`, `logger.error`
**What they express:** "I care about this moment." "This value matters here."
**What we derive:** Correctness invariants — preconditions, postconditions, conservation laws, bounds, degenerate inputs.
**Runtime opportunity:** Yes — stack frame inspection gives live values for Z3 evaluation.
**Friction:** Zero. Every codebase has thousands. No code changes required.

This is where provekit starts. It's the Trojan horse.

### Layer 2: Type System (Language Signals)

**What:** `const`, `readonly`, `private`, `protected`, type annotations, `never`, `Readonly<T>`, `as const`, `NonNullable<T>`, interface definitions, generic constraints.
**What they express:** Immutability, encapsulation, value constraints, unreachability, shape guarantees.
**What we derive:** Type safety invariants — can the runtime violate what the type system promises?

The key insight: TypeScript types are compile-time only. They are erased at runtime. Every type annotation is a specification that exists in the source code but has zero enforcement in production.

`const x = 5` — can `x` be mutated through prototype pollution or `Object.defineProperty`?
`private secret: string` — can external code access `secret` through bracket notation or `as any`?
`function validate(x: string): never` — can execution return from a function declared `never`?
`readonly balance: number` — can `balance` change after construction?

Each of these is a programmer-written specification. TypeScript partially enforces them. provekit fully proves them. The Z3 check:

```smt2
; Can a const be mutated?
(declare-const value_at_declaration Int)
(declare-const value_at_use Int)
; const means these should be equal across all code paths
(assert (not (= value_at_declaration value_at_use)))
(check-sat)
; sat → the const can be mutated through a reachable code path
```

### Layer 3: Data Flow (Security Signals)

**What:** Sources (user input entry points), sinks (dangerous operations), sanitizers (validation/encoding functions).
**What they express:** Where untrusted data enters, how it flows, where it causes damage.
**What we derive:** Security invariants — taint analysis across the OWASP top 10.

Every vulnerability class is the same shape: tainted data flows from a source to a sink without sanitization. This maps directly to the axiom template pattern:

```smt2
; Generic taint flow check
(declare-const input_tainted Bool)
(declare-const sanitized Bool)
(declare-const reaches_sink Bool)
(assert input_tainted)         ; data enters from untrusted source
(assert (not sanitized))       ; no sanitizer on the code path
(assert reaches_sink)          ; data reaches dangerous operation
(check-sat)
; sat → vulnerability is reachable
```

**Security axiom templates (extending the seed set):**

| Axiom | Source | Sink | Vulnerability |
|---|---|---|---|
| P-RCE | `req.body`, `req.params` | `eval()`, `exec()`, `Function()`, `vm.run()` | Remote Code Execution |
| P-SQLi | User input | String concatenation → `db.query()` | SQL Injection |
| P-XSS | User input | `innerHTML`, `document.write()`, template rendering | Cross-Site Scripting |
| P-PathTraversal | User input | `fs.readFile()`, `fs.writeFile()`, `path.join()` | Path Traversal |
| P-ProtoPollution | User-controlled keys | Object merge, deep clone, recursive assign | Prototype Pollution |
| P-SSRF | User input | `fetch()`, `http.request()`, `axios()` | Server-Side Request Forgery |
| P-Deserialization | Untrusted bytes | `JSON.parse()`, `deserialize()` without schema | Insecure Deserialization |
| P-OpenRedirect | User input | `res.redirect()`, `window.location` | Open Redirect |

Each axiom template is mechanically instantiable: AST identifies sources and sinks, data flow analysis traces the path, Z3 checks if an unsanitized flow exists.

**Where log statements and data flow intersect:**

A `logger.info("Processing request", { userId, query })` near a `db.execute(query)` — that's not just a correctness check. The log statement led us to the variables. The type tells us `query` is a string. The data flow analysis traces `query` back to `req.body`. The Z3 check proves the taint reaches the sink unsanitized. What started as a logging invariant became an SQL injection proof.

### Layer 4: Control Flow (Structural Signals)

**What:** Exhaustive switches, try/catch patterns, guard clauses, early returns, assertion statements.
**What they express:** "All cases are handled." "Errors are caught." "This guard prevents X."
**What we derive:** Completeness invariants.

An exhaustive `switch` in TypeScript with a `default: never` is a specification: "I've handled every case." If the union type expands and the switch doesn't, the `never` is reachable — that's a proven violation.

A `try { ... } catch (err) { logger.error(err) }` is a specification: "I handle errors on this path." If the catch block re-throws, swallows, or handles only a subset of possible errors, the invariant may be incomplete.

An early return `if (!isValid) return null;` is a path condition (already extracted by provekit). Everything after it assumes `isValid === true`. If `isValid` can be circumvented, the assumption breaks.

### Layer 5: Naming (Semantic Signals)

**What:** Function names (`validateOrder`, `sanitizeInput`, `ensureAuthenticated`), variable names (`safeBalance`, `trustedOrigin`, `cleanHtml`, `normalizedPath`).
**What they express:** The programmer's belief about what the function does or what the variable contains.
**What we derive:** Semantic invariants.

`sanitizeInput` — does it actually sanitize? The function name is a contract. If the implementation doesn't strip/encode dangerous characters, the name is a lie. Z3 can check: does any input pass through `sanitizeInput` and still contain dangerous characters?

`safeBalance` — is it actually safe? If `safeBalance` can be negative, the name contradicts the value. The variable name is an invariant the programmer intended but didn't enforce.

`ensureAuthenticated` — does it actually ensure authentication? If the function can return without verifying credentials, the name is a violated contract.

### Layer 6: Comments and TODOs (Explicit Intent)

**What:** `// this should never be null`, `// TODO: handle race condition`, `// FIXME: potential overflow`, `// HACK: temporary workaround`.
**What they express:** Direct statements of belief or known issues.
**What we derive:** Direct invariants.

`// this should never be null` → derive non-null invariant, prove it with Z3.
`// TODO: handle race condition` → the programmer KNOWS about the bug. Derive the race condition formally, prove it's reachable. The TODO becomes a filed issue with a proof.
`// FIXME: potential overflow` → derive the overflow condition, prove whether it's reachable.

TODOs are the most honest signals in code. The programmer already identified the problem. provekit formalizes it and proves it.

## The Architecture Supports All Layers

Phase 1 (Dependency Graph) is unchanged.

Phase 2 (Context Assembly) expands. Instead of finding only log statements, it finds all signals: log calls, type annotations, const/readonly/private declarations, taint sources and sinks, function and variable names with semantic content, comments with assertions or TODOs. Each signal becomes a call site with context.

Phase 3 (Derivation) is already signal-agnostic. The prompt says "at this point in the code, derive what should be true." The LLM derives invariants regardless of whether the signal was a log statement, a const declaration, a function name, or a TODO comment.

Phase 4 (Principles) is unchanged. Novel patterns become new axioms regardless of signal source.

Phase 5 (Axioms) expands. The axiom template library grows to include:
- **Correctness axioms** (P1-P7): from log statements
- **Type safety axioms** (P-Const, P-Private, P-Never, P-Readonly): from type system
- **Security axioms** (P-RCE, P-SQLi, P-XSS, etc.): from data flow
- **Completeness axioms** (P-Exhaustive, P-ErrorHandling): from control flow
- **Semantic axioms** (P-NameContract): from naming
- **Known-issue axioms** (P-TODO): from comments

All verified by the same Z3. All producing the same proof format. All independently verifiable with `echo '...' | z3 -in`.

## The Convergence

Log statements give you correctness proofs.
Type annotations give you type safety proofs.
Data flow gives you security proofs.
Control flow gives you completeness proofs.
Names give you semantic proofs.
Comments give you known-issue proofs.

Same pipeline. Same Z3. Same proofs. Different signals, same math.

A single codebase analyzed across all six layers produces a comprehensive formal verification — correctness, type safety, security, completeness, semantics, known issues — from code that already exists, without writing a single specification.

The specification was always there. In the logs, in the types, in the names, in the comments. In every informal expression of programmer intent. We just didn't have the theorem prover listening to all of them at once.

Now we do.
