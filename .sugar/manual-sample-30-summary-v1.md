# #115 step 2: manual-30 gate summary

**Sample salt:** `115-step2-v1`
**Reviewer:** manual-30 labeler (Claude Opus 4.7 1M)
**Date:** 2026-04-27

## Overall counts

| Verdict | Count |
| --- | --- |
| agree | 16 |
| disagree | 14 |
| unclear | 0 |
| **total** | **30** |

**Precision = 16 / 30 = 53.3%**

## Verdict: **FAIL** (need >= 27 / 30 = 90%)

The mechanical-tagger-v1 fails the precision gate by a wide margin (~36 points
short). The `expressible-now-recognized` stratum is broken end-to-end: every
single sampled row in that bucket is mistagged. The other strata are roughly
healthy.

## Per-stratum breakdown

| Stratum | Sampled | Agree | Disagree | Stratum precision |
| --- | --- | --- | --- | --- |
| expressible-now-pending-principle | 16 | 15 | 1 | 93.8% |
| expressible-now-recognized | 11 | 0 | 11 | **0.0%** |
| needs-new-relation | 1 | 0 | 1 | 0.0% |
| unknown | 2 | 1 | 1 | 50.0% |

## What went wrong (disagree rows)

### The systemic failure: arithmetic principles fire on string `+`

`addition-overflow`, `subtraction-underflow`, and `multiplication-overflow` all
match on AST `BinaryExpression` operator without distinguishing **numeric
addition** from **string concatenation**. Half the disagrees are this.

- **karma/Bug-22**: `URL_REGEXP = new RegExp('http' + config.hostname + ...)` is regex string assembly. addition-overflow misfires on string `+`.
- **eslint/Bug-51**: bug splits one sentinel regex into three; the matched `+` is string concat in regex literal.
- **eslint/Bug-64**: bug adds `requiresTrailingSpace` helper. The `+` is string concat in fixer text `(prefix + parenthesizedSource + (suffix ? " " : ""))`.
- **hexo/Bug-12**: bug adds `.toString()` coercion before slugize. The `+` is string concat `'^' + escapeRegExp(slug)`.
- **eslint/Bug-49**: bug rewrites object-shorthand fixer ranges. The `+` is string concat in `keyPrefix + keyText + sourceCode.text.slice(...)`.

### The second systemic failure: principle fires at unchanged code at the locus

Audit says "principles hit at locus" but the matched arithmetic isn't what the
fix changed. The principle has no "is the matched node actually dirty in the
diff?" filter.

- **eslint/Bug-232**: bug is a null-guard `parentElements[0] && ...`. There is no arithmetic in the fix at all; addition-overflow + multiplication-overflow are completely unrelated.
- **eslint/Bug-246**: bug rewrites comment scoping with new helpers. `blockStart + 2` and `blockEnd - 2` are at the locus but unchanged. Principles fire on stable arithmetic.
- **eslint/Bug-60**: bug is "missing baseline indent additive offset" (`indentSize * options...` → `getNodeIndent(node).goodChar + indentSize * options...`). The multiplication is at the locus but the bug class is "missing addend," not multiplication overflow.
- **eslint/Bug-182**: bug guards against fixing block comments containing leading `/`. `commentGroup.length - 1` is at the locus and unchanged. subtraction-underflow misfires.
- **hessian.js/Bug-6**: bug filters synthetic `this$N` keys. `byteBuffer.position() - 1` is unchanged at the locus.

### Wrong-locus falsy-default

- **express/Bug-21**: bug is missing `req.params` in the save/restore wrap; fix introduces `restore()` helper. The `parentUrl = req.baseUrl || ''` line is unchanged but is what `falsy-default` matched on. Same "principle fires at unchanged dirty-zone code" failure mode.

### Regex content outside substrate

- **eslint/Bug-80** (pending-principle row): bug is `(.+?)` → `([^{}]+?)`. Pure regex literal change, no logic change. Substrate has no regex-pattern capability. Should have been `unknown`.
- **eslint/Bug-244** (needs-new-relation row): bug is `/set(?:Timeout|Interval)|execScript/` → `/^(setTimeout|setInterval|execScript)$/`. Pure regex anchoring; no multi-node relation needed. Should have been `unknown`.

### Parser robustness

- **pencilblue/Bug-7** (unknown row): typo fix `protoype` → `prototype` in fully readable JS. Tagger said "parser failed on every changed file." Should have been `expressible-now-pending-principle`. The audit reason is honest about the failure but the classification is wrong on substrate criteria.

## Tagger failure modes (in order of impact)

1. **No operand-type filter on arithmetic principles.** addition-overflow / subtraction-underflow / multiplication-overflow match `BinaryExpression` op without checking whether either operand is plausibly numeric. Half the recognized-stratum misses are string concat firing the addition-overflow principle.
2. **No "matched node is in dirty set" filter.** Principles can fire on arithmetic that exists at the locus but is unchanged by the fix. The audit line says "hit at locus" without confirming the principle's matched node was edited.
3. **Regex-content bugs misrouted.** Regex literal changes get classified as recognized / pending-principle / needs-new-relation rather than honestly admitted as outside substrate (`unknown`).
4. **Parser failures classified as `unknown` shape.** When the parser fails to index a file, the tagger shrugs to `unknown`, but the bug may be a substrate-shaped JS bug. Should distinguish "parser failed" from "shape genuinely exotic."

## Recommended next steps

1. Add operand-type filter (literal numeric, or known-numeric binding) to the three overflow principles before the precision gate is rerun. This alone moves ~6 rows from disagree to agree.
2. Add `is_in_dirty_set($matched)` clause to all DSL principles so they only fire on diff-changed nodes (or run principles in dirty-only mode by default for mining contexts).
3. Decide policy for regex-content bugs: either add a `regex_pattern` capability or formalize the "regex change → unknown" route.
4. Investigate the pencilblue parser failure separately; the file is fully parseable JS.

After (1) and (2), rerun `sample-30.ts` with the same salt and re-evaluate the
gate. The pending-principle stratum is already at 93.8%, so once recognized is
fixed the overall number should clear 90%.
