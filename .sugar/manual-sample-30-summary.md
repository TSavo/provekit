# #115 step 2: manual-30 gate summary (ROUND 2 / v3)

**Sample salt:** `115-step2-v1`
**Reviewer:** manual-30 labeler (Claude Opus 4.7 1M)
**Date:** 2026-04-27
**Tagger version:** v3 (after architectural fixes from round 1)

## Distribution shift recap

| Stratum    | v1   | v2   | v3   |
|------------|------|------|------|
| recognized | 147  | 114  | 81   |
| pending    | 252  | 285  | 318  |

The v2→v3 step (harness-level dirty-set filter) dropped 33 stable-at-locus shape matches as designed. Recognized count is way down. Now the question is whether the *remaining* recognized matches are TPs.

## Overall counts

| Verdict    | Count |
|------------|-------|
| agree      | 18    |
| disagree   | 12    |
| unclear    | 0     |
| **total**  | **30**|

**Precision = 18 / 30 = 60.0%**

## Verdict: **FAIL** (need >= 27 / 30 = 90%)

Round 1 was 16/30 (53.3%). Round 2 is 18/30 (60.0%). The two architectural fixes shifted the failure mode but did not close the gap. The pending and unknown strata are now bulletproof; the recognized stratum is still broken because the principles fire on **mechanical shape that does not match their semantic claim**, even when the matched node has been changed by the fix.

## Per-stratum breakdown

| Stratum                                | Sampled | Agree | Disagree | Stratum precision |
|----------------------------------------|---------|-------|----------|-------------------|
| expressible-now-pending-principle      | 16      | 16    | 0        | 100.0%            |
| expressible-now-recognized             | 11      | 2     | 9        | **18.2%**         |
| needs-new-relation                     | 1       | 0     | 1        | 0.0%              |
| unknown                                | 2       | 2     | 0        | 100.0%            |

The recognized stratum carries 9 of the 12 disagrees.

## What round-1 fixes accomplished

The two architectural fixes worked exactly as designed:

- **Arithmetic-principle filters (`result_sort != "String"` + `is_in_dirty_set`)** eliminated the v1 string-concat false-positive class. None of the v3 disagrees in this sample are string-`+` confusions on `addition-overflow`, `subtraction-underflow`, or `multiplication-overflow`. (One remaining `subtraction-underflow` co-fire on Bug-246 is the principle being shape-matched to incidental arithmetic at the locus, not a sort-mistake.)
- **Harness dirty-set filter** dropped 33 stable-at-locus matches. The recognized count went from 147 → 81, and we no longer see "principle fires at unchanged code at the locus" disagrees.
- **Pending-stratum precision is 16 / 16 = 100%.** The substrate's capability claims are sound. The four "obviously plausible" capability matches in v1 also hold here.
- **Unknown stratum is 2 / 2 = 100%.** Pure regex-content bugs and parser-failed bugs are correctly admitted as outside our substrate.

## What round-1 fixes did NOT address

Both architectural fixes are *structural*: they verify the matched node is on the changed side of the diff. They do not verify that the principle's *semantic claim* matches the bug's *intent*.

### Failure mode 1: `variable-staleness` shape-matches almost any if-with-assignment

`variable-staleness` accounts for 7 of 11 recognized rows in this sample, and 6 of those 7 are wrong:

| # | Bug                | Actual bug                                        | Why staleness misfires                              |
|---|--------------------|---------------------------------------------------|-----------------------------------------------------|
| 17| eslint/Bug-232     | null-guard `parentElements[0] && ...`             | no fall-through, just a guard                       |
| 20| eslint/Bug-80      | regex content tightening                          | no if-block-with-assignment at all                  |
| 21| express/Bug-23     | array-of-fn handling for `app.use([fn])`          | missing-case widening, not fall-through default     |
| 22| eslint/Bug-196     | wrong-arg + missing multiplication                | THEN-branch value bug, fall-through is fine         |
| 23| eslint/Bug-11      | `Math.max` clamp on negative slice index          | no if-block-with-assignment                         |
| 25| eslint/Bug-323     | broadening else-if narrowing                      | missing-case widening, not fall-through             |
| 26| eslint/Bug-298     | missing-case for class methods                    | original if/else block was REMOVED in fix           |

The principle's stated claim is "fall-through path sees the unmodified value", but the DSL only checks (a) structural nesting of the assignment in the `if` consequent and (b) some other use of the assigned variable outside the `if`. It does not check that fall-through is even reachable (i.e., that the `if` has no `else` branch) or that the use-outside-the-if is the *fall-through reading* of the unmodified default.

### Failure mode 2: `or-chain-extended-by-fix` over-fires on enclosing wrappers

The `was_replaced_by_addition` relation accepts any `added` post node that strictly encloses the unchanged BinaryExpression.

- **#19 hexo/Bug-12**: post adds `.toString()` around `(data.slug || data.title)`. The encloser is a CallExpression on a MemberExpression, not a BinaryExpression. No clause was added.
- **#24 eslint/Bug-184**: fix is `expected = false` → `expected = leadingComments.length > 0`. The matched falsy_default has to be elsewhere in the file's surrounding code; no OR-chain extension exists.

The principle's stated claim is "OR-chain was extended in the production fix; the matched version is missing a clause that the maintainer added", but the relation does not require the encloser to itself be a `BinaryExpression` (or a `falsy_default` truthiness coercion).

### Failure mode 3: arithmetic principles still co-fire on incidental arithmetic

- **#18 eslint/Bug-246**: `subtraction-underflow + variable-staleness`. The actual fix is a comment-scoping rewrite using `getCommentsInNode` + `isLocatedBefore`. The subtraction `commentGroup.length - 1` is in the dirty zone but is incidental to the bug. The dirty-set filter caught the worst class of FP but not this one.
- **#27 eslint/Bug-301**: `multiplication-overflow + variable-staleness`. The fix discriminates `typeof options[parent.type] === "number"` vs string `"first"`. Multiplication is small-scale indent arithmetic, not overflow-prone. The principle has a known capability gap (`value_comparison` not implemented) so it cannot suppress on small-domain operands.

## Disagrees: full list with corrections

- **#17 eslint/Bug-232 (variable-staleness):** null-guard, not staleness → pending
- **#18 eslint/Bug-246 (subtraction-underflow + variable-staleness):** comment-padding rewrite → pending
- **#19 hexo/Bug-12 (or-chain-extended-by-fix):** post added `.toString()` wrapper, not a new OR clause → pending
- **#20 eslint/Bug-80 (variable-staleness):** pure regex content fix → unknown
- **#21 express/Bug-23 (variable-staleness):** missing-case for array-of-fn → pending
- **#22 eslint/Bug-196 (variable-staleness):** wrong-arg + missing multiplication, not fall-through → pending
- **#23 eslint/Bug-11 (variable-staleness):** `Math.max` clamp, no if/assign → pending
- **#24 eslint/Bug-184 (or-chain-extended-by-fix):** `expected = leadingComments.length > 0`, no OR chain → pending
- **#25 eslint/Bug-323 (variable-staleness):** broadening else-if narrowing → pending
- **#26 eslint/Bug-298 (variable-staleness):** missing-case for class methods, if/else REMOVED → pending
- **#27 eslint/Bug-301 (multiplication-overflow + variable-staleness):** missing string-option handling → pending
- **#28 eslint/Bug-244 (needs-new-relation):** pure regex-literal anchoring, not multi-node → unknown

## Most surprising patterns

1. **`variable-staleness` shape-matches are rampant.** 6 of 7 recognized matches in this sample are wrong. The principle as written is essentially "any if-block with an assignment whose target is also used outside". The semantic gate ("fall-through is reachable AND the use-outside is the unmodified default") does not exist.
2. **`or-chain-extended-by-fix` over-fires on wrappers.** The `was_replaced_by_addition` relation accepts any added enclosing post node. Adding `.toString()` around an unchanged OR chain triggers it because the call-expression strictly encloses the preserved BinaryExpression. The relation needs an "encloser is itself a BinaryExpression / falsy_default" predicate.
3. **The pending stratum is now bulletproof.** 16/16 in this sample (and the 4 obviously-plausible capability matches in v1 also held). The substrate's capability claims survived a stress test; the principle library is the bottleneck.

## Recommended next steps

1. **Tighten `variable-staleness` semantically.** Add two requirements: (a) the matched `if` has no `else` clause (so fall-through is reachable), and (b) the use-outside-the-if is on a fall-through path (e.g., dominator analysis or a simple "use is in a sibling/parent statement that follows the if"). Six of seven `variable-staleness` recognized matches in this sample would be filtered out by (a) alone.
2. **Tighten `or-chain-extended-by-fix`.** The `was_replaced_by_addition` relation should require the enclosing post node to itself be a `BinaryExpression` (or carry a `falsy_default` truthiness coercion), not just any added node.
3. **Drop `multiplication-overflow` from the recognized-stratum library until `value_comparison` capability lands.** The principle's documented capability gap (cannot suppress on small-domain operands) means the dirty-set filter alone is not enough. Park it.
4. **Re-run the recognized-only sample after fixing #1 and #2.** Pending and unknown strata are bulletproof; iterate on the recognized stratum in isolation. Predicted ceiling: with `variable-staleness` and `or-chain-extended-by-fix` tightened, recognized precision rises from ~18% to ~60%+, putting overall precision in the 75-85% range; one more pass on the residual will likely clear 90%.
