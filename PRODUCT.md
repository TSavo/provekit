# neurallog: Product Spec

## What It Is

A CLI tool that formally verifies your code from the specifications you already wrote — in your log statements, type annotations, function names, and comments.

## Installation

```
npm install -D neurallog
```

## First Run

```
$ npx neurallog init

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

Deriving contracts for 34 files in dependency order...
  src/utils/validate.ts ............ 12 proofs, 3 violations
  src/db/queries.ts ................ 8 proofs, 5 violations
  src/api/billing.ts ............... 27 proofs, 14 violations
  src/api/orders.ts ................ 18 proofs, 9 violations
  ...

Done. 187 proofs verified. 43 violations found.

Top findings:
  1. src/billing.ts:602  credential exposure in token hint [P6]
  2. src/orders.ts:47    discount can exceed order total [P5]
  3. src/inventory.ts:18 stock can go negative [P1]

Git hook installed. Commits will be verified automatically.
Contracts saved to .neurallog/ — commit this directory.

Run  neurallog report               for the full coverage report
Run  neurallog explain src/billing.ts:602  for any finding
```

That's it. The developer now has formal verification. They didn't write a spec. They didn't learn a tool. They ran one command.

## Daily Use

The developer writes code. They commit. The hook runs.

### When everything's fine:

```
$ git commit -m "refactor pricing logic"

neurallog: verifying 2 changed files...
  ✓ src/pricing.ts: 11 proofs hold
  ✓ src/utils/math.ts: 4 proofs hold
```

The commit lands. The developer didn't think about neurallog.

### When a proof regresses:

```
$ git commit -m "add bulk discount"

neurallog: verifying 2 changed files...
  ✓ src/pricing.ts: 11 proofs hold
  ✗ src/orders.ts:47 — proof regressed

  discount can exceed order total
  Previously proven safe. Your change made it reachable.

  Verify: echo '(declare-const discount Real)
  (declare-const total Real)
  (assert (> discount total))
  (check-sat)' | z3 -in

  Run  neurallog explain src/orders.ts:47  for details

Commit blocked. Fix the issue or: neurallog override --reason "intentional"
```

The developer sees:
- What broke (one sentence)
- That it was previously proven safe (this is a regression, not a pre-existing issue)
- How to verify independently (copy-paste command)
- How to learn more (explain command)
- How to override if intentional

### The explain command:

```
$ neurallog explain src/orders.ts:47

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
- What's wrong (one paragraph, plain english)
- What the code guarantees (path conditions from AST)
- What it doesn't guarantee (the gap)
- The proof (SMT-LIB they can run)
- A suggested fix

### The report command:

```
$ neurallog report

neurallog coverage: src/
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
$ neurallog diff HEAD~5

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
  run: npx neurallog verify --ci

# That's it. Exit 0 or exit 1.
```

Optional: file issues for violations

```yaml
- name: Verify and file issues
  run: npx neurallog verify --ci --issues
```

## Runtime Mode (Optional)

For production monitoring. Add the transport to your logger:

```typescript
import pino from 'pino';
import { neurallog } from 'neurallog/transport';

const logger = pino({}, neurallog());
```

Normal logging works exactly as before. Behind the scenes, neurallog evaluates contracts against live values. Proof entries stream alongside log lines.

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
| One paragraph | `neurallog explain` | They want to understand |
| Full proof | The SMT-LIB block | They want to verify |
| `echo \| z3 -in` | sat or unsat | They trust nothing |
| `.neurallog/` | All artifacts | They're a power user |
| SIGNALS.md | Six signal layers | They want the theory |

Most developers never go past level 2. The tool is invisible when it passes and clear when it fails.

## Configuration

```json
// .neurallog/config.json (created by init)
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
- The dependency graph (unless they inspect .neurallog/)
- Signal layers by name (the tool just "finds more things" over time)

The tool is invisible infrastructure. Like a compiler warning system that gets smarter.

## What Makes It Different

- **No specs to write.** The specs are already in your code.
- **No tests to maintain.** The proofs are derived automatically.
- **No new workflow.** You commit. It verifies.
- **No opinions.** Every finding has a mathematical proof.
- **No trust required.** `echo '...' | z3 -in`. Verify it yourself.
- **No cloud required.** Runs on your machine. Z3 is local.
- **Gets smarter.** New signal layers activate over time. New axioms discovered from your bugs.
- **Exit code.** 0 or 1. That's the API.

## The Pitch

Your log statements already describe your system's behavior. Every one of them is a claim about what's happening. You just never enforced them.

What if you did?

`npm install -D neurallog && npx neurallog init`
