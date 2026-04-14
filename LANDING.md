# neurallog.app

## Your code already has a formal specification. You just call it logging.

Every `console.log` you've ever written was a claim about what should be true. You just never enforced it.

neurallog does.

```
npm install -D neurallog
npx neurallog init
```

One command. No specs to write. No tests to maintain. No new workflow. Your log statements become formal proofs. Your commits get verified. Your bugs get found — with mathematical certainty.

---

## How it works

You write code the way you always have:

```typescript
logger.info(`Reserving ${quantity} of ${productId}`);
```

neurallog reads the code around that log statement. It understands what should be true. It writes a formal proof. Z3 — a theorem prover — verifies it.

If the proof holds: ✓. Your code is mathematically correct at that point.

If it doesn't: you get the exact violation, a one-line verification command, and a suggested fix.

```
✗ src/orders.ts:47 — discount can exceed order total

  verify: echo '...' | z3 -in

  Suggested: if (discount > orderTotal) discount = orderTotal;
```

Don't trust the tool. Run the proof yourself. It's math.

---

## What happens next

The log statement is the gateway drug.

Once neurallog is in your repo, it starts reading more than your logs. It reads your types — `const`, `private`, `readonly`. It reads your function names — does `sanitizeInput` actually sanitize? It reads your comments — that `// TODO: handle race condition` becomes a filed issue with a formal proof.

It reads your data flow. User input through your API into your database queries. Taint analysis. SQL injection, XSS, path traversal — proven reachable or proven safe. Not scanned. Proven.

You installed a logging tool. You got a formal verification platform.

---

## The numbers

We pointed neurallog at a real production billing file. 813 lines. 14 log statements.

- **92 properties formally proven** — verified by Z3, independently reproducible
- **76 violations found** — each with a mathematical proof of reachability
- **1 credential exposure** — a token hint that leaks the full Bearer token for short tokens
- **2 new verification axioms discovered** — patterns the tool taught itself

From `logger.info` calls. That someone wrote for audit purposes.

---

## The git hook

```
$ git commit -m "add bulk discount"

neurallog: verifying...
  ✓ src/pricing.ts: 11 proofs hold
  ✗ src/orders.ts:47 — proof regressed

Commit blocked.
```

Your commits are formally verified. The hook runs Z3 — no AI, no cloud, no network. Pure math on your machine. Milliseconds.

When everything's fine, you don't even notice it's there.

---

## The exit code

neurallog has one API: exit 0 or exit 1.

- `neurallog verify` in your git hook
- `neurallog verify --ci` in your pipeline

That's the entire integration. One line in your config. Zero ongoing maintenance. The proofs live in `.neurallog/`, committed alongside your code, reviewed in your PRs.

---

## What makes it different

**No specs to write.** Your code already has specifications. They're in your log statements, your types, your function names, your comments. neurallog reads them.

**No tests to maintain.** Proofs are derived automatically from your code. When the code changes, the proofs re-derive. No test rot. No flaky tests. Math doesn't flake.

**No opinions.** Every finding is a Z3 proof. `echo '...' | z3 -in`. Don't trust us. Don't trust the AI. Trust the math.

**No cloud for verification.** Z3 runs on your machine. The git hook has no external dependency. The proofs are local. Your code never leaves your machine.

**Gets smarter over time.** neurallog discovers new verification patterns from real bugs. The axiom library grows. New signal layers activate. Your log statements were the first thing it read. They won't be the last.

---

## The verification dial

You control how much enforcement you want:

**Advisory** — show findings, don't block anything. See what neurallog would catch. Build confidence.

**Warn** — show warnings on commit. The developer sees regressions but isn't blocked. Training wheels.

**Enforce** — block commits on proof regression. Block PRs on violations. The standard for teams that ship.

**Runtime** — evaluate proofs against live values in production. Catch violations with real data. File issues automatically with mathematical proofs attached.

Start wherever you're comfortable. Turn it up when you trust it.

---

## Signal plugins

neurallog reads your code through signal generators — plugins that find points where invariants should exist.

**Built-in signals:**
- Log statements — `console.log`, `logger.info`
- Type annotations — `const`, `private`, `readonly`, `never`
- Function names — `sanitizeInput`, `validateOrder`, `ensureAuth`
- Comments — `// TODO`, `// FIXME`, `// should never be null`
- Error throws — `throw new Error("balance cannot be negative")`
- Data flow — taint from user input to dangerous sinks

**Write your own:**
```typescript
neurallog.registerSignal({
  name: "finance",
  findSignals(source, ast) {
    // flag every currency multiplication for overflow checking
  }
});
```

The pipeline is signal-agnostic. New signals produce new proofs. Same Z3. Same git hook. Same exit code.

---

## For security teams

Every vulnerability class is the same shape: tainted data flows from a source to a sink without sanitization. neurallog proves these flows — or proves they're safe.

| Vulnerability | What neurallog proves |
|---|---|
| SQL Injection | User input reaches query construction unsanitized |
| XSS | User input reaches DOM insertion unencoded |
| RCE | User input reaches eval/exec |
| Path Traversal | User input reaches filesystem operations |
| SSRF | User input reaches outbound HTTP requests |

Not scanned. Proven. With Z3. `echo '...' | z3 -in`.

Bug bounty programs that require proof submissions eliminate AI slop overnight. The triage cost goes to zero. The signal is the math.

---

## For compliance

The proof log is a continuous, machine-verified record of what your software provably did. Not what you tested. Not what you think it did. What it provably did.

- **SOC2:** "Here are the formal proofs that every billing log statement held for every transaction this quarter."
- **PCI-DSS:** "Here is the mathematical proof that card data never reaches an unsanitized log."
- **GDPR:** "Here is the proof that PII handling follows the specified invariants."

The auditor runs `echo '...' | z3 -in`. The proof verifies. The audit is done.

---

## Pricing

**Free forever:** `neurallog verify`. Z3 runs on your machine. The git hook costs nothing. Your proofs are yours.

**Pay for derivation:** `neurallog derive` uses an LLM to read your code and produce contracts. Pay per derivation, or bring your own model.

**The verification is free. The intelligence costs money. Once derived, proofs are free to check forever.**

---

## The thesis

Every programmer who has ever written a log statement was writing a formal specification. They just didn't have the theorem prover listening.

Now they do.

```
npm install -D neurallog
npx neurallog init
```

neurallog.app — a logger that fixes your code.
