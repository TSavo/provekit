# neurallog.app

## Your code already has informal specifications scattered through it. neurallog finds them, formalises them, and checks them — and tells you when the formalisation was wrong.

Every `console.log`, every `// TODO: handle the race`, every `validateOrder` function name, every type annotation — these are claims you've written about what's supposed to be true. neurallog extracts them, translates them to SMT-LIB via an LLM, checks them with Z3, and then runs the real code against Z3's witnesses to catch the cases where the translation was lossy.

```
npm install -D neurallog
npx neurallog init
```

One command. No specs to write. The mechanism is LLM-plus-Z3-plus-runtime — described honestly below so you know what you're getting.

---

## How it actually works

You write code the way you always have:

```typescript
logger.info(`Reserving ${quantity} of ${productId}`);
```

neurallog runs five phases:

1. **Signals.** Tree-sitter parses your source and extracts signal points — logs, type annotations, function names with semantic patterns (`^validate`, `^sanitize`), comments.
2. **Contract derivation.** An LLM reads each signal's code context and emits an SMT-LIB encoding of the property that should hold. This step uses an LLM and costs money.
3. **Z3 check.** Z3 verifies the SMT block. `unsat` on the negated goal means the property is consistent with the LLM's model of the code. Fast, deterministic, offline.
4. **Runtime harness.** For each proven property, neurallog takes Z3's witness (the concrete input Z3 used), loads the real function, and executes it. If the function's runtime behaviour contradicts the property, that's an **encoding gap** — the LLM's translation of your code was wrong.
5. **Test-suite cross-reference.** When your project has existing tests, neurallog invokes the ones that reference the target function via your test runner (vitest, jest, mocha, node --test) and compares their outcomes to the harness verdict.

A claim Z3 proves, the harness confirms, and existing tests corroborate is filed as **high-confidence proven**. A claim Z3 proves but the harness refutes is filed as an **encoding gap** — a bug in the formalisation, not the code. Disagreements are annotated, not hidden.

---

## What's a proof, really?

The Z3 verdict is a proof about the SMT-LIB encoding. The encoding was produced by an LLM. The LLM can be wrong. When it is, Z3 will happily prove a property about a function that doesn't exist in your codebase.

This is the central, unsolved problem in the LLM-plus-SMT verification genre. Most tools pretend it isn't there. neurallog is built around catching it.

**What's true:** If Z3 says `unsat`, no counterexample exists *within the SMT-LIB block the LLM wrote*. You can re-run it yourself with `echo '...' | z3 -in`. The math is checkable.

**What's not true:** That the block faithfully models your TypeScript. That's the runtime harness's job to empirically test.

**What happens when the encoding is wrong:** neurallog's runtime harness executes the real function with Z3's witness inputs. If the function does something different from what Z3 predicted, you get a finding labelled `encoding-gap` with:

- the claim Z3 proved
- the concrete input used
- the observed runtime behaviour
- a one-line diagnostic from the LLM judge explaining the discrepancy

We have an empirical example of this in the tool's own repo history: Z3 proved `divide(a, b)` protected against division-by-zero. The harness ran `divide(0, 0)` and got `NaN` — JavaScript doesn't throw on `0/0`. Z3 had proved a claim about idealised integer arithmetic; the code ran in IEEE 754. The harness caught the mismatch.

This is the signal we sell you. The "mathematical certainty" pitch in most LLM-plus-SMT tools is overblown; we call it out so you can calibrate what the findings actually mean.

---

## The git hook

```
$ git commit -m "add bulk discount"

neurallog: verifying...
  [template] 23 mechanical proofs (no LLM) in 4s
  [entailment] ✓ preconditions propagate
  [strength] 18/23 claims load-bearing, 5 vacuous flagged
  Commit proceeds.
```

The git hook runs the mechanical `verify` phase against already-derived contracts on disk. **This path does not call an LLM.** Z3 is all that runs: milliseconds to seconds, offline, deterministic.

Contract derivation — the LLM-costly part — runs on demand (`neurallog derive`, or in CI), not on every commit. Your hook-time experience is local, fast, and free.

---

## The numbers

**Caveat before numbers:** the figures below were observed during a pre-refactor pipeline run against a real production billing file. They were logged to the terminal, not persisted as versioned contract artifacts, and were not re-verified under the current pipeline. Treat them as illustrative of what the tool can find, not as reproducible benchmarks. A re-run against a committed artifact is pending.

On an 813-line billing file with 14 log statements:

- **27 Z3-proven invariants** across nine functions — each re-runnable via `z3 -in`.
- **31 reachable violations** with concrete counterexamples — each with a witness Z3 produced.
- **2 of those were independently significant real bugs:**
  - A credential-exposure pattern: `authHeader.slice(7, 15)` leaks the entire Bearer token for tokens of 8 characters or fewer.
  - An audit-observation atomicity gap: the log line describing a cross-tenant access is separated from the DB query serving it, so the audit record can describe a different state than what was actually served.
- **2 novel verification principles** proposed by the tool's principle-synthesis loop. Both were validated against adversarial examples in a separate LLM pass before being added to the principle store.

The harness and test-oracle layers shipped after this run. A re-run with those layers would additionally produce encoding-gap findings we haven't yet measured.

---

## What makes it different

**No tests to maintain — but the proofs derive via an LLM.** We won't pretend the LLM is free. Derivation costs money. Steady-state verification (`neurallog verify`) doesn't.

**No certainty claims.** Most tools in this genre advertise "mathematical proof of correctness." Z3 proves the SMT-LIB encoding is consistent. The encoding is LLM-produced. We say this out loud instead of hoping you won't ask.

**Encoding-gap detection.** Unique among LLM-plus-SMT tools we know of. The runtime harness empirically tests whether the SMT models the code faithfully. When it doesn't, you get the finding and the diagnostic. Most of the genre has no such layer — their soundness gap is structural and unobservable.

**Your code goes to the LLM.** Contract derivation sends source to the configured provider (Anthropic, OpenAI, OpenRouter, etc.). If that matters for your codebase, run the derivation step on a self-hosted model — the provider interface is pluggable. The verify step does not send code to the LLM.

**Self-growing principle library.** When a bug pattern is observed enough times, the tool proposes a new atomic principle (AST pattern plus SMT template). The principle is adversarially validated — a different model tries to break it with false positives and negatives — and only the validated ones enter the store. Over time the mechanical-template coverage grows, reducing the per-contract LLM cost.

---

## The verification dial

You control how much enforcement you want:

**Advisory** — show findings, don't block anything.
**Warn** — show warnings on commit; the developer sees regressions but isn't blocked.
**Enforce** — block commits on proof regression. Block PRs on high-confidence violations.
**Runtime** — enable runtime observation (`NEURALLOG_OBSERVE=1`) to capture real values at signal points. Those values feed back into verification as a Daikon-style third input source alongside Z3 models.

You can gate high-confidence violations separately from low-confidence ones. Confidence is set by whether the runtime harness corroborated Z3's verdict, whether existing tests cross-referenced, and whether the LLM judge flagged the harness as rigged.

---

## Signal plugins

neurallog reads your code through signal generators — plugins that find points where invariants should exist.

**Built-in signals:**
- AST patterns — branches, loops, try/catches, dangerous calls, arithmetic on parameters, falsy-default traps, non-null assertions, mutations, throws
- Log statements — `console.log`, `logger.info`, pino, winston
- Type annotations — `readonly`, literal types, strict-null types
- Function names — `sanitizeInput`, `validateOrder`, `ensureAuth`, 30+ patterns
- Comments — TODO/FIXME/HACK in function bodies
- LLM-mediated (opt-in, slower) — arbitrary function analysis

Signal generation is AST-first and works without an LLM. LLM-based signal generation is available opt-in for cases the AST patterns miss.

---

## For security teams

The tool proves taint-like properties the same way it proves anything else: Z3 checks the LLM's encoding, the runtime harness tests the encoding against real execution. A SQL-injection claim Z3 "proves" doesn't become a proof that your code is safe from SQL injection; it becomes a proof about the LLM's model of your code. The harness tells you whether to trust that model.

Use cases where that calibration is acceptable:

| Vulnerability | What the tool produces |
|---|---|
| SQL Injection | A Z3 verdict on whether the LLM-encoded taint flow from user input to query string is sat or unsat, a runtime harness attempting to exercise the flow, and a judge verdict on whether the harness faithfully tests the claim. |
| XSS | Same structure: LLM encodes the DOM-insertion flow, Z3 checks, harness executes. |
| Path Traversal | Same structure against filesystem-op sinks. |

What this is **not**: a replacement for a dedicated SAST tool that has been tuned for years on known vulnerability corpora. What it is: a cross-validating layer that can catch gaps in the SAST output by empirically running the checks the SAST tool merely pattern-matched.

---

## For compliance

neurallog produces a verifiable audit trail: for each contract, the SMT-LIB block, Z3's verdict, the runtime harness outcome, and the judge's verdict are all stored under `.neurallog/`. An auditor can re-run `z3 -in` against any specific block, replay a harness against the current code, and see the full history of contract evolution in git.

What this is **not**: a regulator-accepted formal-methods certification. Compliance frameworks that require formal verification usually require a specific tool chain (Coq, Isabelle, Dafny, TLA+) whose soundness is itself certified — not an LLM-plus-SMT loop whose central honesty is that the LLM can be wrong.

What this **is**: higher-quality evidence than raw test coverage. "These 847 contracts about our billing code were Z3-proven, runtime-corroborated, and test-cross-referenced, with three disagreements flagged for review" is a defensible statement. "The math proves our billing code is correct" is not, and we won't let you make it on our behalf.

---

## Pricing

**Free forever:** `neurallog verify`. Z3 runs on your machine against already-derived contracts. The git hook and local development cost nothing after initial derivation.

**Pay for derivation:** `neurallog derive` and the harness-synthesis layer use LLMs. You pay per run, or bring your own model keys. Contract derivation is the expensive step; once derived, verification is free forever until the code changes.

**What you're paying for when you pay:** the LLM work that reads your code and emits SMT encodings, the LLM work that synthesizes runtime harnesses, the LLM judge that audits those harnesses and cross-references outcomes. Our margin is on making the LLM calls efficient — caches at every layer, hashes that invalidate only on real change, parallel synthesis — not on your per-contract cost.

---

## The thesis

Every programmer who has written a log statement, named a function `validateOrder`, or added a TODO has written an informal assertion about their code's intended behaviour. A growing subset of the program-verification community is trying to get LLMs to translate those assertions into SMT and let solvers check them.

The core weakness of that approach is that LLM translations can be lossy. We agree with the critique. We built the tool around catching the lossy cases rather than hiding behind the mathematics of the solver.

If that's the verification tradeoff you're willing to live with — three calibrated oracles instead of one advertised certainty — neurallog is for you.

```
npm install -D neurallog
npx neurallog init
```

neurallog.app
