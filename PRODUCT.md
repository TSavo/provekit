# provekit: Product Spec

## What It Is

A CLI tool that derives SMT-LIB claims from the informal specifications already in your code — log statements, type annotations, function names, comments — and checks them with Z3, a runtime harness, and (when available) your existing test suite. Findings are calibrated by how many of the three oracles agree; "proven by Z3 alone" is a weaker signal than "proven by Z3, corroborated by runtime, corroborated by existing tests." The UX surface is designed around that calibration.

## Installation

```
npm install -D provekit
```

## First Run

```
$ npx provekit init

Scanning your codebase...

Found 247 signals across 34 files:
  189 log statements
   32 typed function signatures
   14 TODO/FIXME comments
   12 assertion-style error throws

What would you like to do?

  1. Preview — show what would be analyzed (instant, no LLM)
  2. Analyze — derive proofs and show findings (~10 min for 247 signals)
  3. Full setup — analyze + install git hook

> 3

Deriving contracts for 34 files in dependency order (this step calls the
configured LLM and will cost tokens)...
  src/utils/validate.ts ............ 12 Z3-proven, 3 violations, 2 encoding-gaps
  src/db/queries.ts ................ 8 Z3-proven, 5 violations
  src/api/billing.ts ............... 27 Z3-proven, 14 violations, 1 encoding-gap
  src/api/orders.ts ................ 18 Z3-proven, 9 violations
  ...

Done. 187 Z3 verdicts, 43 violations, 3 encoding-gaps flagged for review.

Top findings (high-confidence — corroborated by runtime harness):
  1. src/billing.ts:602  credential exposure in token hint [P6]
  2. src/orders.ts:47    discount can exceed order total [P5]
  3. src/inventory.ts:18 stock can go negative [P1]

Encoding-gap findings (Z3 said safe, runtime disagreed — encoder bug):
  1. src/math.ts:44      0/0 claim proves unsat, runtime returns NaN [P2]

Git hook installed. Commits will be verified against cached contracts
(no LLM, Z3 only).
Contracts saved to .provekit/ — commit principles; artifacts are ignored.

Run  provekit report               for the full coverage report
Run  provekit explain src/billing.ts:602  for any finding
```

The developer ran one command and got a pipeline output calibrated to
how much they should trust each finding. They didn't write a spec. They
didn't learn the tool. They didn't get the word "formal" attached to
their code without qualification.

## Daily Use

The developer writes code. They commit. The hook runs.

### When everything's fine:

```
$ git commit -m "refactor pricing logic"

provekit: verifying 2 changed files...
  ✓ src/pricing.ts: 11 proofs hold
  ✓ src/utils/math.ts: 4 proofs hold
```

The commit lands. The developer didn't think about provekit.

### When a verdict regresses:

```
$ git commit -m "add bulk discount"

provekit: verifying 2 changed files...
  ✓ src/pricing.ts: 11 Z3-verified contracts hold
  ✗ src/orders.ts:47 — Z3-verified claim regressed

  discount can exceed order total (high-confidence violation —
  runtime harness reproduces the arithmetic path).
  Previously Z3 proved this unreachable under the existing code.
  Your change made the counterexample reachable.

  Verify the Z3 verdict yourself: echo '(declare-const discount Real)
  (declare-const total Real)
  (assert (> discount total))
  (check-sat)' | z3 -in
  ; sat

  Run  provekit explain src/orders.ts:47  for the full harness output

Commit blocked. Fix, or: provekit override --reason "intentional"
```

The developer sees:
- What broke (one sentence)
- That it was previously proven safe (this is a regression, not a pre-existing issue)
- How to verify independently (copy-paste command)
- How to learn more (explain command)
- How to override if intentional

### The explain command:

```
$ provekit explain src/orders.ts:47

┌─────────────────────────────────────────────────┐
│  discount can exceed order total                │
│  Signal: console.log(`Applied discount: ${d}`)  │
│  Principle: Semantic Correctness                 │
│  Status: VIOLATION (sat)                         │
└─────────────────────────────────────────────────┘

The code at line 47 applies a discount to the order total
without checking that the discount doesn't exceed the total.
When discount > orderTotal, the customer pays a negative amount.

Path conditions at line 47:
  1. order.items.length > 0    (guard at line 32)
  2. couponCode validated      (check at line 38)
  3. discount > 0              (computed at line 44)

These are guaranteed true. What's NOT guaranteed:
  discount <= orderTotal

Proof (Z3 confirmed reachable):

  (declare-const discount Real)
  (declare-const orderTotal Real)
  (declare-const netAmount Real)
  (assert (> orderTotal 0))
  (assert (> discount 0))
  (assert (= netAmount (- orderTotal discount)))
  (assert (< netAmount 0))
  (check-sat)
  ; sat — negative payment is reachable

Verify yourself:
  echo '<above>' | z3 -in

Suggested: add guard at line 46
  if (discount > orderTotal) discount = orderTotal;
```

The explain command gives the developer everything:
- What's wrong (one paragraph, plain English)
- What the code guarantees at the signal point (path conditions from AST)
- What Z3 says is not guaranteed (the gap)
- The SMT block Z3 ran on (re-runnable via `echo ... | z3 -in`)
- The runtime harness outcome (whether runtime reproduces or refutes
  the counterexample — a refuted counterexample means the SMT encoded
  a gap that doesn't exist in the actual code)
- The test-oracle outcome if your project has tests for this function
- A suggested fix

### The report command:

```
$ provekit report

provekit coverage: src/
──────────────────────────────────────────
Signals found:              247
  ├─ Strong proofs:         143  (58%)
  ├─ Violations:             43  (17%)
  ├─ Weak (no cross-file):   38  (15%)
  ├─ Trivial:                23  (10%)

By signal type:
  Log statements:     189 signals → 112 proofs, 31 violations
  Type annotations:    32 signals →  18 proofs,  7 violations
  TODO/FIXME:          14 signals →   8 proofs,  3 violations
  Error throws:        12 signals →   5 proofs,  2 violations

Files with most violations:
  src/api/billing.ts       14 violations
  src/api/orders.ts         9 violations
  src/services/payment.ts   7 violations

Since last commit: 2 new proofs, 1 violation fixed, 0 regressions
Since last week:   12 new proofs, 4 violations fixed, 1 regression
```

### The diff command:

```
$ provekit diff HEAD~5

Proof changes since HEAD~5:

  + src/pricing.ts:23    NEW: unit price is positive (proven)
  + src/pricing.ts:45    NEW: total = sum of line items (proven)
  ~ src/orders.ts:47     CHANGED: discount guard added (was violation, now proven)
  - src/billing.ts:112   REMOVED: function deleted
  ! src/inventory.ts:18  REGRESSION: quantity check removed (was proven, now violation)
```

Code diff shows what changed. Proof diff shows what it means for correctness.

## CI Integration

```yaml
# GitHub Actions
- name: Verify proofs
  run: npx provekit verify --ci

# That's it. Exit 0 or exit 1.
```

Optional: file issues for violations

```yaml
- name: Verify and file issues
  run: npx provekit verify --ci --issues
```

## Runtime Mode (Optional)

For production monitoring. Add the transport to your logger:

```typescript
import pino from 'pino';
import { provekit } from 'provekit/transport';

const logger = pino({}, provekit());
```

Normal logging works exactly as before. Behind the scenes, provekit evaluates contracts against live values. Proof entries stream alongside log lines.

When a violation fires in production:
- The proof entry includes the values that triggered it
- If `--issues` is configured, a GitHub issue is filed automatically
- The issue includes the proof, the values, and a verification command

## Progressive Disclosure

The developer sees exactly as much as they need:

| Level | What they see | When |
|---|---|---|
| Nothing | ✓ after commit | Everything's fine |
| One line | ✗ proof regressed at file:line | Something broke |
| One paragraph | `provekit explain` | They want to understand |
| Full proof | The SMT-LIB block | They want to verify |
| `echo \| z3 -in` | sat or unsat | They trust nothing |
| `.provekit/` | All artifacts | They're a power user |
| SIGNALS.md | Six signal layers | They want the theory |

Most developers never go past level 2. The tool is invisible when it passes and clear when it fails.

## Configuration

```json
// .provekit/config.json (created by init)
{
  "signals": ["logs", "types"],          // which signal layers to analyze
  "hook": "pre-commit",                   // when to verify
  "model": "sonnet",                      // LLM for derivation
  "strict": false,                        // block commits on violations?
  "ci": true,                             // exit 1 on violations in CI?
  "issues": false                         // auto-file GitHub issues?
}
```

Defaults work for everyone. Power users tune.

## The Verification Dial (Internal, Not User-Facing)

The user doesn't see "levels." They see behavior:

- **Just installed:** scan shows findings, nothing enforced
- **Hook enabled, strict: false:** warnings on commit, doesn't block
- **Hook enabled, strict: true:** blocks commits on proof regression
- **CI enabled:** blocks PRs on violations
- **Runtime transport:** catches violations in production
- **Issues enabled:** auto-files bugs with proofs

The user turns up enforcement gradually. They never see the word "dial."

## What the User Never Sees

- Z3 (unless they want to verify a proof)
- SMT-LIB (unless they ask for the proof)
- Axiom templates (implementation detail)
- Hoare logic (implementation detail)
- The five-phase pipeline (implementation detail)
- The LLM's reasoning (unless --verbose)
- The dependency graph (unless they inspect .provekit/)
- Signal layers by name (the tool just "finds more things" over time)

The tool is invisible infrastructure. Like a compiler warning system that gets smarter.

## What Makes It Different

- **No specs to write.** The informal specs are already in your code — logs, type annotations, function names, comments. provekit extracts and formalises them. The formalisation is LLM-produced, which means it can be wrong, which is why the tool runs a harness and a test oracle to check.
- **No tests to maintain.** Contracts re-derive on code change. Contract derivation uses an LLM and costs money. Verification against already-derived contracts is free.
- **No new workflow.** You commit. The hook runs Z3 against cached contracts (no LLM at commit-time). Derivation happens on demand or in CI.
- **Every finding is a re-runnable Z3 verdict.** `echo '...' | z3 -in` verifies the math Z3 did. It does not verify that the SMT block faithfully models your TypeScript — that's the harness's job. The difference matters; the tool's UX labels it.
- **Verify step runs locally.** Z3 is local, offline, deterministic. Derivation and harness synthesis call the configured LLM provider; your code does leave your machine for those steps.
- **Gets more efficient over time.** New AST-pattern principles are synthesised from recurring bugs under adversarial validation. Once a principle is in the library, its future matches are mechanical — no LLM — which means per-contract cost drops as the library matures.
- **Exit code.** 0 or 1. That's the CI API. The confidence-tier information is in the JSON artifacts for tooling that wants finer distinctions.

## The Pitch

Your log statements, type annotations, function names, and TODO comments describe the behaviour your code is supposed to have. provekit turns that informal specification into a checkable one — an LLM writes the SMT encoding, Z3 checks it, a runtime harness tests whether the encoding faithfully models your code, and your existing tests cross-validate when available.

The central honesty: the LLM's encoding can be wrong, and the tool actively looks for the cases where it is. You don't get mathematical certainty. You get calibrated confidence with the disagreements surfaced rather than hidden.

`npm install -D provekit && npx provekit init`
