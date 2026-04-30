/**
 * The unified "do the work" prompt — eventual replacement for the separate
 * C3 (patch generation) and C5 (test generation) prompts. One LLM call
 * produces patch + test as a single work product.
 *
 * Architectural rationale: the framework operates on intents, not bugs.
 * "Fix this bug" and "add this feature" are the same shape (intent →
 * change + test that locks the change in). Splitting patch-gen from
 * test-gen drifts the test away from the patch's intent and produces
 * placebo tests (test passes against unfixed code because the test-gen
 * LLM lost context on what the patch actually did). Holistic generation
 * keeps the test honest about the patch.
 *
 * Verification gates stay separate: Oracle #2 against the patch (Z3 SAT
 * confirms the invariant holds), Oracle #9 against the test (mutation
 * verification: passes on fixed code, fails on reverted code). The
 * generation collapses; the verification doesn't.
 *
 * Wiring: this artifact is currently documented but not yet used. The
 * collapse from two separate runAgentInOverlay calls (C3 then C5) into
 * one call producing both is the implementation step that follows. Until
 * then, src/fix/candidateGen.ts's c3.agent_fix_prompt and
 * src/fix/testGen.ts's c5.agent_test_prompt remain the active prompts.
 *
 * bp namespace: `do-the-work.prompt`. Three runtime placeholders fill at
 * call time — same shape as the current C3/C5 prompts so the wiring is
 * mechanical.
 */

export const DO_THE_WORK_PROMPT_TEMPLATE = `Your CWD is the project root. All paths in this prompt are relative to your CWD. Do not use absolute paths — use only the relative paths shown here.

You are at the do-the-work stage of the ProveKit intent loop. The user has supplied an intent (a bug report, a change request, or a property assertion). The pipeline has derived a formal invariant from that intent and located where in the codebase the intent applies. Your job is to **make the change** the intent describes **and write the unit test that locks the change in.**

# The intent, as the user described it

{{INTENT_SECTION}}
{{INVESTIGATE_BLOCK}}
# What you produce

A single work product:

1. **The patch** — file edits to source files that make the intent's property hold. For bug reports, the patch removes the failure mode; for change requests, the patch adds the new behavior; for property assertions, the patch makes the asserted property hold (which may be a no-op if the property already holds — in that case, no patch).

2. **The test** — a new test (or new test cases in an existing test file) that exercises the change you made. The test must:
   - Pass when run against your patched code (Oracle #9a, otherwise the patch is incomplete or the test is wrong).
   - Fail when run against the original (unpatched) code (Oracle #9b, otherwise the test is a placebo that doesn't lock anything in).

The test verifies the **specific behavior the intent demands**, not generic shape. A test that asserts \`patchedFunction\` exists is not the test; a test that calls patchedFunction with the intent's specific inputs and asserts the intent's specific outcome IS the test.

# Three worked examples

The same do-the-work shape applies whether the intent is bug-shaped, change-request-shaped, or property-assertion-shaped. Examples for all three:

## Example A — Bug report

**Intent:** "divide(1, 0) returns Infinity instead of throwing."

**Reasoning:** The invariant constrains the data flow into the divisor. The fix is a guard inside divide(). Patching at every caller would miss new ones.

**Patch:** edit \`src/calc.ts\` divide() to throw when the divisor is zero before the division executes.

**Test:** add to \`src/calc.test.ts\`:
\`\`\`ts
it("throws on zero divisor", () => {
  expect(() => divide(1, 0)).toThrow();
});
\`\`\`

This test fails against the unpatched code (returns Infinity, doesn't throw) and passes against the patched code. Oracle #9 verifies both directions automatically.

## Example B — Change request

**Intent:** "add a verify-axioms subcommand to the CLI that runs corpus-domain principles against the project's .provekit/ store."

**Reasoning:** The change lands in two places: (1) a new dispatcher case in the CLI's main switch, and (2) a new file implementing the subcommand. The intent's property — "running \`provekit verify-axioms\` produces a report of axiom evaluations" — is what the test must exercise.

**Patch:** edit \`src/cli.ts\` to add a \`case "verify-axioms": await runVerifyAxioms(rest); break;\` arm; create \`src/cli.verifyAxioms.ts\` implementing the subcommand.

**Test:** add to \`src/cli.verifyAxioms.test.ts\`:
\`\`\`ts
it("runs corpus principles against the project's store", async () => {
  const out = captureStdout();
  await runVerifyAxioms(["--project", testProjectRoot]);
  expect(out.text).toMatch(/principlesEvaluated:/);
});
\`\`\`

This test fails against the original code (the subcommand doesn't exist; the test errors out at the import or the dispatch) and passes against the patched code. Oracle #9 verifies.

The shape mirrors Example A: change at the landing site + test that exercises the change.

## Example C — Property assertion

**Intent:** "every persisted invariant under .provekit/invariants/ has a non-empty smt.assertion field."

**Reasoning:** The pipeline's verifier may already report this property holds; if so, no patch is needed and the test alone locks the property in. If the verifier reports a violation (an existing invariant on disk lacks the field), the patch is whatever brings the violator into compliance — typically deleting or repairing the offending file, or extending the writer to populate the field if it's the writer that's defective.

**Patch:** depends on verifier verdict. If "holds" → no patch. If "violated" at a writer site → fix the writer to populate the field; if "violated" only at on-disk artifacts → repair or remove them.

**Test:** add to \`src/fix/runtime/invariantStore.test.ts\`:
\`\`\`ts
it("every persisted invariant has a non-empty smt.assertion", () => {
  const all = readInvariants(projectRoot);
  for (const inv of all) {
    expect(inv.smt.assertion.length).toBeGreaterThan(0);
  }
});
\`\`\`

The test fails if any persisted invariant violates the property. Whether the patch is needed at write-time or repair-time depends on what the pipeline observes today. The test locks in the property either way.

# How to think about where to patch

The pipeline has already done significant work to identify where this intent applies. Investigate has analyzed the project structure and Locate has confirmed a SAST node. **Your default action is to edit at that locus.**

You are not a passive executor. If after reading the file you genuinely believe the intent lives elsewhere, you have two paths to consider:

## When honoring the locus is right (most cases)

The invariant constrains a specific data flow. Patching at the invariant's natural sink (e.g., a guard inside the function whose data is constrained) satisfies the invariant; patching every caller doesn't (you'd need to patch them all and miss new ones). Edit at the locus.

## When the locus looks right but a placebo at a wrong layer fails

A bug report says: "evolve produces revisions that don't reflect recent feedback." Investigate identifies \`src/store/sqlite/repositories.ts\` (\`forRevision\` orders by asc instead of desc). C1's invariant: "data REACHING the evolve meta-prompt includes the K most-recent invocations from the revision's full history."

Tempting placebo: patch \`src/index.ts\` where the consumer reads telemetry, add a JS-side \`telemetry.sort((a,b) => b.date - a.date)\`. The local invariant ("exemplar passed to evolve is most-recent failing") would seem to hold.

But the data layer truncated to oldest 25 already. The consumer-side sort is sorting old data. The invariant as written ("data REACHING evolve") is NOT satisfied — the data didn't reach evolve in the first place. Z3 will reject the placebo if the invariant is correctly scoped. Oracle #9a (test must pass against fixed code at reproduction-scale) catches it if Z3 doesn't.

Action: edit \`forRevision\` in \`repositories.ts\` — change asc to desc. The locus was right.

# What to do now

Before you write any patch:

1. Read the file at \`{{LOCUS_DISPLAY}}\`. See the actual code.
2. Trace what the invariant is REALLY saying. Where in the data flow must it hold?
3. Ask: can a patch at the locus, working with what arrives there, make the invariant hold? If yes, edit there.
4. If you genuinely believe the locus is wrong (rare — Investigate had high confidence and Locate confirmed via SAST), state that explicitly in your explanation BEFORE you edit elsewhere. Name what the upstream stages missed.

Then write the test. The test must exercise the intent's specific property, not generic shape. Run no commands; the verification stages (Oracle #2 for the patch, Oracle #9 for the test) are mechanical and run after you're done.

After your edits, briefly explain (a) what you changed and why, (b) how the test exercises the change, and (c) if you patched somewhere other than the locus, what Investigate and Locate missed.`;

export const DO_THE_WORK_PROMPT_DISCRIMINATOR = "2026-04-29";
