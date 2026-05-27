# Probe: the inductive bug class defeats SMT but not a proof assistant

This is a **probe**, not a species. It answers one falsifiable question:

> Is there a bug class that the SMT seats (z3, cvc5, vampire) provably cannot
> catch, but an induction-capable proof assistant (coq, lean) can?

If yes, a proof-assistant seat in the solver portfolio is *earned* — it catches
bugs nothing else can. If no, that seat is capacity ahead of demand.

## The obligation

A recursively-defined `sum` with a closed-form postcondition:

```
sum(0) = 0
sum(n) = n + sum(n-1)   for n > 0
goal:   for all n >= 0,  2 * sum(n) = n * (n + 1)
```

This is the shape a real production contract takes on any recursive/loop-bearing
function: "the accumulator equals this closed form for all inputs." Proving it
requires **induction on n** — there is no quantifier-free certificate.

## Result (run the files yourself)

| seat | input | verdict | why |
|------|-------|---------|-----|
| z3 | `sum.smt2` | **unknown** | SMT cannot do induction over the recursion |
| cvc5 | `sum.smt2` | **timeout** | same |
| vampire | `sum.smt2` | **time limit** (saturation) | same |
| coq | `sum.v` | **exit 0 — discharged** | `induction n; simpl; lia` |

```sh
z3 -smt2 -T:10 sum.smt2          # => unknown
cvc5 --lang=smt2 --tlimit=10000 sum.smt2   # => timeout
vampire --input_syntax smtlib2 --time_limit 10 sum.smt2  # => time limit
coqc sum.v                       # => exit 0 (proof checks)
```

## What this proves, and what it does NOT

**Proves:** the inductive bug class is real and SMT-defeating, and a proof
assistant catches it. The cheapest seat that does so is **coq-with-induction**,
which is already installed — *not* lean + mathlib. mathlib is earned only when
an obligation defeats coq-with-induction too (a genuine higher-order /
dependent-type / category-theory property); none has been exhibited yet.

**Does NOT yet exist in ProvekIt:** the machinery to *lift* this from a real Go
function and discharge it through the portfolio. That is a feature arc, scoped
below.

## The feature arc this justifies ("After Induction")

1. **IR**: a recursive-function-definition node (Fixpoint-shaped). The IR today
   has no way to express `sum(n) = n + sum(n-1)`.
2. **Lifter**: lift a recursive Go/Rust function into that node, and emit the
   inductive obligation `forall n. P(f(n))`.
3. **Emitters**:
   - coq: emit `Fixpoint f ... + induction <recursion-var>; simpl; lia`
     (today the coq seat emits only `intros. lia.`, which cannot do induction).
   - lean: emit the recursive def + `induction`/`omega`.
   - smt-lib: `define-fun-rec` (will still return unknown — that is the point).
4. **Induction-variable selection**: the emitter must know which parameter to
   induct on (the recursion parameter). This is the genuinely hard part of
   proof-assistant tactic emission.

Until (1)-(4) land, the proof-assistant seat is wired (see the lean seat's
graceful-degrade behavior) but dormant for this class. This probe is the
standing evidence that building the arc catches real bugs the SMT seats cannot.
