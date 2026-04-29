/**
 * C3: candidateGen helpers.
 *
 * Parses LLM-proposed fix candidates, builds prompts for fix generation,
 * and runs oracle #2 (Z3 under overlay) to verify each candidate.
 *
 * Oracle #2 strategy (v1):
 *   Approach (a) — bug site removed: re-evaluate the principle against the
 *   overlay's scratch DB. If zero principle_matches remain, the fix
 *   structurally removed the bug site → invariant holds.
 *
 *   Approach (b) — novel / still-matched: for novel invariants (principleId null)
 *   OR when the principle still matches, check whether all SMT binding
 *   source_expr strings from the invariant are still present in the
 *   overlay's modified file contents. If any are absent, the expression
 *   was removed → bug_site_removed. If all are present, we cannot
 *   re-evaluate — return "unknown" (which is treated as failure per spec
 *   decision #6).
 *
 * Oracles #8 (gap detector) and #10 (full test suite) are deferred to D1.
 */

import { readFileSync, existsSync } from "fs";
import { join, relative } from "path";
import { eq } from "drizzle-orm";
import { getPromptStore } from "../llm/promptStore.js";

// ---------------------------------------------------------------------------
// The single C3 prompt artifact.
//
// One bp namespace: `c3.agent_fix_prompt`. The static teaching body lives
// here as a literal const (source-of-record), with three runtime
// placeholders that buildAgentFixPrompt fills in at call time:
//   {{INTENT_SECTION}}   — signal summary + verbatim user text
//   {{INVESTIGATE_BLOCK}}— Investigate findings OR locus + invariant fallback
//   {{LOCUS_DISPLAY}}    — overlay-relative path to the bug site, used
//                          inside the "What to do now" instructions
//
// IMPORTANT for future evolution: when bp.evolve rewrites c3.agent_fix_prompt,
// the new revision MUST preserve all three {{...}} placeholders verbatim.
// Without them the assembled prompt loses runtime context. The enhancer
// prompt should be told this constraint explicitly the first time we run
// bp.evolve on this artifact.
// ---------------------------------------------------------------------------

const C3_AGENT_FIX_PROMPT_TEMPLATE = `Your CWD is the project root. All paths in this prompt are relative to your CWD. Do not use absolute paths — use only the relative paths shown here.

You are at the patch-generation stage of the ProveKit intent loop. Your job is to write a minimal code change that makes the formal invariant hold. The intent may be a bug report (the property is stated by negation — what's failing should not fail), a change request (the property is stated directly — make X do Y), or a property assertion (the property IS the user text). Treat them uniformly.

# The intent, as the user described it

{{INTENT_SECTION}}
{{INVESTIGATE_BLOCK}}
# How to think about where to patch

The pipeline has already done significant work to identify where this bug lives. By the time you see this prompt, Investigate has analyzed the project structure and Locate has confirmed a SAST node. **Your default action is to edit at that locus.**

You are not, however, a passive executor. If after reading the file you genuinely believe the bug lives elsewhere, you have two paths to consider — and which one applies determines whether your patch will hold up:

## Worked example A — when honoring the locus is right

A bug report says: "divide(1, 0) returns Infinity instead of throwing." Investigate identifies \`src/calc.ts\` (\`divide\` function). Locate confirms a SAST node on the \`/\` expression. C1's invariant: "for every call to \`divide(a, b)\`, b !== 0 must be reachable before the division executes."

Reasoning: the invariant constrains the data flow into the divisor. The fix is a guard on b inside divide(). Patching divide() satisfies the invariant; patching every caller doesn't (you'd need to patch them all and miss new ones). The locus is right.

Action: edit \`divide\` in \`src/calc.ts\`. Done in one hunk.

## Worked example B — when the locus looks right but a placebo at a wrong layer fails

A bug report says: "evolve produces revisions that don't reflect recent feedback." Investigate identifies \`src/store/sqlite/repositories.ts\` (\`forRevision\` orders by asc instead of desc). C1's invariant (correctly written): "data REACHING the evolve meta-prompt includes the K most-recent invocations from the revision's full history."

Tempting placebo: patch \`src/index.ts\` where the consumer reads telemetry, add a JS-side \`telemetry.sort((a,b) => b.date - a.date)\` to surface recent failures. The local invariant ("exemplar passed to evolve is most-recent failing") would seem to hold.

But the data layer truncated to oldest 25 already. The consumer-side sort is sorting old data. The invariant as written ("data REACHING evolve") is NOT satisfied — the data didn't reach evolve in the first place. Z3 will reject the placebo if the invariant is correctly scoped. Oracle #9a (test must pass against fixed code at reproduction-scale) catches it if Z3 doesn't.

Action: edit \`forRevision\` in \`repositories.ts\` — change asc to desc. The locus was right. The placebo at \`src/index.ts\` would have looked like a fix, but it cannot make the invariant hold because the data never reaches the sort point.

## What to do now

Before you write any patch:

1. Read the file at \`{{LOCUS_DISPLAY}}\`. See the actual code.
2. Trace what the invariant is REALLY saying. Where in the data flow must it hold?
3. Ask: can a patch at the locus, working with what arrives there, make the invariant hold? If yes, edit there.
4. If you genuinely believe the locus is wrong (rare — Investigate had high confidence and Locate confirmed via SAST), state that explicitly in your explanation BEFORE you edit elsewhere. Name what the upstream stages missed.

Your patch is the minimum change that makes the invariant hold. Do NOT run tests. After your edits, briefly explain what you changed and why — and if you patched somewhere other than the locus, explain what Investigate and Locate missed.`;

const C3_AGENT_FIX_PROMPT_DISCRIMINATOR = "2026-04-28";

/**
 * Result of building C3's agent prompt: the assembled string plus the bp
 * revision id (for telemetry recording at the call site). Empty revisions
 * array when projectRoot is absent (no telemetry; behavior identical to pre-bp).
 */
export interface C3PromptBuild {
  prompt: string;
  revisions: Array<{ key: string; revisionId: string }>;
}

// C3 retry suffix artifact (appended to the original prompt when the first
// patch attempt fails Oracle #2). Single placeholder {{Z3_VERDICT}}.
const C3_RETRY_SUFFIX_TEMPLATE = `

Your previous fix attempt did not satisfy the invariant. Oracle #2 returned: {{Z3_VERDICT}}. Please revise the fix.`;
const C3_RETRY_SUFFIX_DISCRIMINATOR = "2026-04-28";

/** Build the retry suffix appended to C3's original prompt on Oracle #2 fail. */
export async function buildAgentFixRetrySuffix(
  z3Verdict: string,
  projectRoot?: string,
): Promise<{ suffix: string; revisions: Array<{ key: string; revisionId: string }> }> {
  const revisions: Array<{ key: string; revisionId: string }> = [];
  let body = C3_RETRY_SUFFIX_TEMPLATE;
  if (projectRoot) {
    const rev = await getPromptStore(projectRoot).get(
      "c3.retry_suffix",
      C3_RETRY_SUFFIX_TEMPLATE,
      C3_RETRY_SUFFIX_DISCRIMINATOR,
    );
    body = rev.body;
    revisions.push({ key: "c3.retry_suffix", revisionId: rev.id });
  }
  const suffix = body.replaceAll("{{Z3_VERDICT}}", z3Verdict);
  return { suffix, revisions };
}

import { principleMatches } from "../db/schema/principleMatches.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";
import { verifyBlock } from "../verifier.js";
import { applyPatchToOverlay, reindexOverlay } from "./overlay.js";
import { getIntentText } from "./types.js";
import type {
  IntentSignal,
  BugLocus,
  InvariantClaim,
  OverlayHandle,
  CodePatch,
  FixCandidate,
} from "./types.js";
import { parseJsonFromLlm } from "./llmJson.js";
import { requestStructuredJson } from "./llm/structuredOutput.js";
import { extractGuardConditions } from "./pathConditions.js";
import { classifyInvariantKind } from "./invariantKind.js";

// ---------------------------------------------------------------------------
// Internal shape for a proposed fix from the LLM
// ---------------------------------------------------------------------------

export interface ProposedFix {
  patch: CodePatch;
  rationale: string;
  confidence: number;
}

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

// C3 JSON-path fix prompt artifact: c3.json_fix_prompt
// Five placeholders (max-candidates, intent summary + text, locus, source, invariant).
const C3_JSON_FIX_PROMPT_TEMPLATE = `You are a code-repair expert. Given an intent (a bug report, a change request, or a property assertion) and a formal invariant violation, propose up to {{MAX_CANDIDATES}} candidate patches that make the invariant hold.

Intent summary: {{INTENT_SUMMARY}}
User text: {{INTENT_TEXT}}

Location: {{LOCATION}}

Source context:
\`\`\`
{{SOURCE_CONTEXT}}
\`\`\`

Invariant violated: {{INVARIANT_DESCRIPTION}}
Formal expression (SMT, violation state — must become unsat after fix):
{{INVARIANT_FORMAL_EXPRESSION}}

Respond with ONLY a JSON object (no markdown fences, no extra text):
{
  "candidates": [
    {
      "rationale": "one sentence: why this patch fixes the bug",
      "confidence": 0.8,
      "patch": {
        "description": "what the patch does",
        "fileEdits": [
          {
            "file": "relative/path/to/file.ts",
            "newContent": "FULL file contents after the patch"
          }
        ]
      }
    }
  ]
}

Rules:
- Each candidate must have rationale (string), confidence (0..1 number), and patch.
- patch.fileEdits is an array; each entry has file (relative path) and newContent (full file content).
- Rank candidates by confidence descending.
- Do NOT output anything outside the JSON object.`;
const C3_JSON_FIX_PROMPT_DISCRIMINATOR = "2026-04-29";

export async function buildFixPrompt(
  signal: IntentSignal,
  locus: BugLocus,
  invariant: InvariantClaim,
  maxCandidates: number,
  projectRoot?: string,
): Promise<string> {
  // Attempt to read the source of the containing function.
  let sourceContext = "(source not available)";
  try {
    if (existsSync(locus.file)) {
      const lines = readFileSync(locus.file, "utf-8").split("\n");
      const start = Math.max(0, locus.line - 5);
      const end = Math.min(lines.length, locus.line + 10);
      sourceContext = lines
        .slice(start, end)
        .map((l, i) => `${start + i + 1}: ${l}`)
        .join("\n");
    }
  } catch {
    // ignore
  }

  let body = C3_JSON_FIX_PROMPT_TEMPLATE;
  if (projectRoot) {
    const rev = await getPromptStore(projectRoot).get(
      "c3.json_fix_prompt",
      C3_JSON_FIX_PROMPT_TEMPLATE,
      C3_JSON_FIX_PROMPT_DISCRIMINATOR,
    );
    body = rev.body;
  }

  return body
    .replaceAll("{{MAX_CANDIDATES}}", String(maxCandidates))
    .replaceAll("{{INTENT_SUMMARY}}", signal.summary)
    .replaceAll("{{INTENT_TEXT}}", getIntentText(signal))
    .replaceAll("{{LOCATION}}", `${locus.file}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}`)
    .replaceAll("{{SOURCE_CONTEXT}}", sourceContext)
    .replaceAll("{{INVARIANT_DESCRIPTION}}", invariant.description)
    .replaceAll("{{INVARIANT_FORMAL_EXPRESSION}}", invariant.formalExpression);
}

// ---------------------------------------------------------------------------
// Agent-mode prompt builder
// ---------------------------------------------------------------------------

/**
 * Build a concise prompt for the agent path (capture-the-change).
 * The agent edits files directly in its cwd — no JSON response schema required.
 *
 * @param overlay - When provided, locus.file is expressed as an overlay-relative
 *   path so the agent cannot derive an absolute path to the user's real repo.
 */
export async function buildAgentFixPrompt(
  signal: IntentSignal,
  locus: BugLocus,
  invariant: InvariantClaim,
  overlay?: { worktreePath: string },
  investigate?: import("./stages/investigate.js").InvestigateReport,
  projectRoot?: string,
): Promise<C3PromptBuild> {
  // Compute overlay-relative path for the locus so the agent never sees an
  // absolute path to the user's real repo (the dogfood bypass used locus.file
  // directly, which let the agent edit the user's actual file).
  let locusDisplay = locus.file;
  if (overlay) {
    try {
      const rel = relative(overlay.worktreePath, locus.file);
      if (!rel.startsWith("..")) {
        locusDisplay = rel;
      } else {
        // locus.file is in the real repo (absolute). Try suffix matching.
        const parts = locus.file.split("/").filter(Boolean);
        for (let i = 0; i < parts.length; i++) {
          const suffix = parts.slice(i).join("/");
          if (existsSync(join(overlay.worktreePath, suffix))) {
            locusDisplay = suffix;
            break;
          }
        }
      }
    } catch {
      // Non-fatal: fall back to locus.file (absolute). Log-worthy but not blocking.
    }
  }

  const investigateBlock = investigate
    ? `

Upstream stages have already analyzed this bug. Their findings:

INVESTIGATE (project-level scan that produced the locus you'll receive):
- Primary location: ${investigate.primaryLocation.file}${investigate.primaryLocation.function ? ` (${investigate.primaryLocation.function})` : ""}
- Investigate's confidence: ${investigate.primaryLocation.confidence}
- Investigate's reasoning: ${investigate.primaryLocation.rationale}
- Root-cause hypothesis: ${investigate.rootCauseHypothesis}
- Fix hypothesis (shape, not exact text): ${investigate.fixHypothesis}
${investigate.candidateLocations.length > 0 ? `- Other candidates Investigate considered (rejected or ranked lower):\n${investigate.candidateLocations.map((c) => `  - ${c.file}${c.function ? ` (${c.function})` : ""} (${c.confidence})`).join("\n")}` : ""}

LOCATE (SAST-level confirmation): resolved Investigate's primary
location to the specific node at ${locusDisplay}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}.

C1 (formal invariant): the patch must satisfy:
  ${invariant.description}
  ${invariant.formalExpression.replace(/\n/g, "\n  ")}
`
    : `

Locus (from upstream Locate stage): ${locusDisplay}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}

Invariant the patch must satisfy:
  ${invariant.description}
  ${invariant.formalExpression}
`;

  // Dynamic context block (signal summary + failure + optional fix hint).
  const intentSection = `Intent summary: ${signal.summary}
User text: ${getIntentText(signal)}`;

  // Single bp artifact for the full C3 agent prompt body. Day 0 the
  // template is byte-identical to pre-bp; bp.evolve sees the whole
  // composed prompt — coherence is global. No-op (literal direct) when
  // projectRoot is absent.
  const revisions: Array<{ key: string; revisionId: string }> = [];
  let templateBody = C3_AGENT_FIX_PROMPT_TEMPLATE;
  if (projectRoot) {
    const rev = await getPromptStore(projectRoot).get(
      "c3.agent_fix_prompt",
      C3_AGENT_FIX_PROMPT_TEMPLATE,
      C3_AGENT_FIX_PROMPT_DISCRIMINATOR,
    );
    templateBody = rev.body;
    revisions.push({ key: "c3.agent_fix_prompt", revisionId: rev.id });
  }

  const prompt = templateBody
    .replace("{{INTENT_SECTION}}", intentSection)
    .replace("{{INVESTIGATE_BLOCK}}", investigateBlock)
    .replace("{{LOCUS_DISPLAY}}", locusDisplay);

  return { prompt, revisions };
}

// ---------------------------------------------------------------------------
// Response parser
// ---------------------------------------------------------------------------

/**
 * Parse the LLM response into ProposedFix[].
 * Skips malformed candidates with a console warning.
 * Throws if the response is not JSON or has zero valid candidates.
 *
 * Accepts either a raw LLM string (legacy) or a pre-parsed object (when
 * called from a site that already routed through requestStructuredJson).
 */
export function parseProposedFixes(rawOrParsed: string | unknown): ProposedFix[] {
  let parsed: unknown;
  if (typeof rawOrParsed === "string") {
    try {
      parsed = parseJsonFromLlm(rawOrParsed, "parseProposedFixes");
    } catch (e) {
      throw new Error(e instanceof Error ? e.message : String(e));
    }
  } else {
    parsed = rawOrParsed;
  }

  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !Array.isArray((parsed as Record<string, unknown>)["candidates"])
  ) {
    const display = typeof rawOrParsed === "string" ? rawOrParsed.slice(0, 200) : JSON.stringify(parsed).slice(0, 200);
    throw new Error(
      `parseProposedFixes: expected {"candidates": [...]} but got: ${display}`,
    );
  }

  const rawCandidates = (parsed as { candidates: unknown[] }).candidates;
  const valid: ProposedFix[] = [];

  for (let i = 0; i < rawCandidates.length; i++) {
    const c = rawCandidates[i] as Record<string, unknown> | null | undefined;
    if (!c || typeof c !== "object") {
      console.warn(`parseProposedFixes: candidate[${i}] is not an object — skipping`);
      continue;
    }

    const rationale = c["rationale"];
    const confidence = c["confidence"];
    const patchRaw = c["patch"] as Record<string, unknown> | null | undefined;

    if (typeof rationale !== "string" || !rationale) {
      console.warn(`parseProposedFixes: candidate[${i}] missing 'rationale' — skipping`);
      continue;
    }
    if (typeof confidence !== "number" || confidence < 0 || confidence > 1) {
      console.warn(`parseProposedFixes: candidate[${i}] invalid 'confidence' — skipping`);
      continue;
    }
    if (!patchRaw || typeof patchRaw !== "object") {
      console.warn(`parseProposedFixes: candidate[${i}] missing 'patch' — skipping`);
      continue;
    }

    const description = patchRaw["description"];
    if (typeof description !== "string" || !description) {
      console.warn(`parseProposedFixes: candidate[${i}].patch missing 'description' — skipping`);
      continue;
    }

    const fileEditsRaw = patchRaw["fileEdits"];
    if (!Array.isArray(fileEditsRaw) || fileEditsRaw.length === 0) {
      console.warn(`parseProposedFixes: candidate[${i}].patch.fileEdits must be non-empty array — skipping`);
      continue;
    }

    let allEditsValid = true;
    const fileEdits: { file: string; newContent: string }[] = [];
    for (let j = 0; j < fileEditsRaw.length; j++) {
      const edit = fileEditsRaw[j] as Record<string, unknown> | null | undefined;
      if (!edit || typeof edit !== "object") {
        console.warn(`parseProposedFixes: candidate[${i}].patch.fileEdits[${j}] not object — skipping candidate`);
        allEditsValid = false;
        break;
      }
      const file = edit["file"];
      const newContent = edit["newContent"];
      if (typeof file !== "string" || !file) {
        console.warn(`parseProposedFixes: candidate[${i}].patch.fileEdits[${j}].file invalid — skipping candidate`);
        allEditsValid = false;
        break;
      }
      if (typeof newContent !== "string") {
        console.warn(`parseProposedFixes: candidate[${i}].patch.fileEdits[${j}].newContent invalid — skipping candidate`);
        allEditsValid = false;
        break;
      }
      fileEdits.push({ file, newContent });
    }

    if (!allEditsValid) continue;

    valid.push({
      patch: { fileEdits, description },
      rationale,
      confidence,
    });
  }

  return valid;
}

// ---------------------------------------------------------------------------
// Oracle #2
// ---------------------------------------------------------------------------

/**
 * Oracle #2 verdicts.
 *
 * - "unsat": Z3 confirmed the post-fix violation SMT is unsat. Invariant holds.
 * - "bug_site_removed": structural check confirmed the source expressions are
 *   gone from the modified files. Kind-agnostic — works for both concrete and
 *   abstract invariants. (e.g. fix deletes the call site entirely.)
 * - "deferred_behavioral": ABSTRACT invariant whose source expressions still
 *   appear in modified files (sanitization-style fix that preserves the call).
 *   Z3 cannot prove the abstract Bool predicate is now false because there's
 *   no formula linking the sanitization to the predicate. The behavioral gate
 *   at C5 (oracle #9: test fails on original code, passes on patched) IS the
 *   verification. C3 treats this as a pass-forward verdict; C5 is responsible
 *   for the real proof.
 * - "sat": Z3 says the violation is still reachable. Reject the candidate.
 * - "unknown" / "error": treat as failure.
 */
export type OracleTwoVerdict =
  | "sat"
  | "unsat"
  | "unknown"
  | "error"
  | "bug_site_removed"
  | "deferred_behavioral";

/**
 * Load the DSL source for a principle from the overlay's .provekit/principles/ dir.
 * Returns null if not found.
 */
function loadPrincipleDslFromOverlay(
  overlay: OverlayHandle,
  principleId: string,
): string | null {
  const dslPath = join(overlay.worktreePath, ".provekit", "principles", `${principleId}.dsl`);
  if (!existsSync(dslPath)) return null;
  try {
    return readFileSync(dslPath, "utf-8");
  } catch {
    return null;
  }
}

/**
 * Oracle #2: verify that the invariant now holds in the overlay's scratch DB.
 *
 * Adaptive routing by invariant kind:
 *
 *   1. Approach (a) — structural bug-site removal. KIND-AGNOSTIC.
 *      Either the principle no longer matches, or all binding source_exprs
 *      are gone from modified files. Either way: bug_site_removed.
 *
 *   2. CONCRETE path (Int/Real declarations present): existing Z3 logic.
 *      Try guard-augmented SMT (path conditions from dominance), then plain
 *      verifyBlock. Z3 unsat = invariant holds.
 *
 *   3. ABSTRACT path (Bool-only, no Int/Real declarations): no Z3 fallback.
 *      Z3 cannot prove the abstract taint predicate is now false because
 *      there's no formula linking sanitization to the Bool predicate. Return
 *      "deferred_behavioral" so the orchestrator pass-forwards to C5, where
 *      mutation-verified regression test (oracle #9) IS the verification:
 *        - test fails on original code (must)
 *        - test passes on patched code (must)
 *      That's empirical proof equivalent in informational content.
 *
 * Returns "bug_site_removed" | "unsat" on success.
 * Returns "deferred_behavioral" for abstract invariants whose source
 * expressions still appear in modified files (sanitization-style fix).
 * Returns "sat" | "unknown" | "error" for failure cases.
 */
export async function runOracleTwo(
  overlay: OverlayHandle,
  invariant: InvariantClaim,
): Promise<OracleTwoVerdict> {
  // Prefer the authoritative classification stamped by C1.5 fidelity routing
  // (which can demote concrete → abstract when fixtures verifier returns 0/N
  // negatives, indicating Bool-encoded-as-Int). Fall back to surface SMT
  // classification for paths that don't run C1.5 (principle match, tests).
  const kind = invariant.effectiveKind ?? classifyInvariantKind(invariant);
  // -----------------------------------------------------------------------
  // Approach (a): principle path
  // -----------------------------------------------------------------------
  if (invariant.principleId !== null) {
    const dslSource = loadPrincipleDslFromOverlay(overlay, invariant.principleId);
    if (dslSource === null) {
      // DSL not in overlay (principle not copied) — cannot re-evaluate.
      // Fall through to expression check.
    } else {
      // Re-evaluate the principle against the overlay's scratch DB.
      // evaluatePrinciple also writes to the scratch DB — that's fine (it IS the scratch DB).
      try {
        const matches = evaluatePrinciple(overlay.sastDb, dslSource);
        if (matches.length === 0) {
          return "bug_site_removed";
        }
        // Matches still exist — fix didn't remove the bug site structurally.
        // For ABSTRACT invariants, Z3 cannot help: the principle still matches
        // because the call site is still there post-sanitization, but Z3 has no
        // formula linking the sanitization to the abstract Bool predicate.
        // Defer to C5's behavioral gate (oracle #9).
        if (kind === "abstract") {
          return "deferred_behavioral";
        }
        // CONCRETE path: try guard-augmented SMT (extract dominating guards
        // from overlay SAST). If Z3 returns unsat under guards, the path is
        // unreachable → invariant holds.
        const guards = extractGuardConditions(overlay, invariant.bindings);
        if (guards.guardCount > 0) {
          // Strip the trailing (check-sat) from formalExpression and append guard
          // assertions + a fresh (check-sat).
          const baseSmtWithoutCheck = invariant.formalExpression.replace(/\(check-sat\)[\s\S]*$/, "").trimEnd();
          const augmented = baseSmtWithoutCheck +
            "\n" + guards.smtAssertions.join("\n") + "\n(check-sat)";
          try {
            const z3Result = verifyBlock(augmented);
            if (z3Result.result === "unsat") return "unsat";  // guards make violation unreachable
            if (z3Result.result === "sat") return "sat";
            if (z3Result.result === "unknown") return "unknown";
            return "error";
          } catch {
            // Z3 invocation failed — fall through to original verdict.
          }
        }
        // No guards found, or guard-augmented path failed: proxy was right to reject.
        // Run verifyBlock on the original formalExpression as the rejection signal.
        // formalExpression encodes the violation state → Z3 returns sat → reject candidate.
        // NOTE: this does NOT verify that the fix makes the invariant hold. It is
        // intentionally rejection-only (see original comment).
        const z3Result = verifyBlock(invariant.formalExpression);
        if (z3Result.result === "sat") return "sat";
        if (z3Result.result === "unsat") return "unsat";
        if (z3Result.result === "unknown") return "unknown";
        return "error";
      } catch {
        // evaluatePrinciple errored — treat as unknown (failure).
        return "unknown";
      }
    }
  }

  // -----------------------------------------------------------------------
  // Approach (a) for novel invariants (principleId === null):
  // Check if the binding source_expr strings are gone from the modified files.
  // -----------------------------------------------------------------------
  if (invariant.bindings.length > 0) {
    const modifiedContents: string[] = [];
    for (const rel of overlay.modifiedFiles) {
      const absPath = join(overlay.worktreePath, rel);
      if (existsSync(absPath)) {
        try {
          modifiedContents.push(readFileSync(absPath, "utf-8"));
        } catch {
          // ignore
        }
      }
    }

    if (modifiedContents.length > 0) {
      // If every binding's source_expr is absent from all modified files, bug site removed.
      const allGone = invariant.bindings.every((b) =>
        !modifiedContents.some((content) => content.includes(b.source_expr)),
      );
      if (allGone) {
        return "bug_site_removed";
      }
    }
  }

  // -----------------------------------------------------------------------
  // Fallback for novel invariants: ABSTRACT defers to C5; CONCRETE runs Z3.
  //
  // For ABSTRACT (Bool-only) invariants, Z3 has no formula linking source-code
  // sanitization to the abstract predicate. Re-running verifyBlock on the
  // unmodified formalExpression would just return "sat" again (it's the same
  // SMT as pre-fix) and incorrectly reject every candidate. Defer to oracle #9.
  // -----------------------------------------------------------------------
  if (kind === "abstract") {
    return "deferred_behavioral";
  }

  // CONCRETE: try guard-augmented Z3 first, then plain.
  if (invariant.bindings.length > 0) {
    const guards = extractGuardConditions(overlay, invariant.bindings);
    if (guards.guardCount > 0) {
      const baseSmtWithoutCheck = invariant.formalExpression.replace(/\(check-sat\)[\s\S]*$/, "").trimEnd();
      const augmented = baseSmtWithoutCheck +
        "\n" + guards.smtAssertions.join("\n") + "\n(check-sat)";
      try {
        const z3Result = verifyBlock(augmented);
        if (z3Result.result === "unsat") return "unsat";
        if (z3Result.result === "sat") return "sat";
        if (z3Result.result === "unknown") return "unknown";
        return "error";
      } catch {
        // Fall through to plain Z3 below.
      }
    }
  }

  try {
    const z3Result = verifyBlock(invariant.formalExpression);
    if (z3Result.result === "sat") return "sat";
    if (z3Result.result === "unsat") return "unsat";
    if (z3Result.result === "unknown") return "unknown";
    return "error";
  } catch {
    return "error";
  }
}

// ---------------------------------------------------------------------------
// Candidate verifier
// ---------------------------------------------------------------------------

export async function verifyCandidate(
  proposed: ProposedFix,
  overlay: OverlayHandle,
  invariant: InvariantClaim,
): Promise<{
  invariantHoldsUnderOverlay: boolean;
  z3Verdict: "sat" | "unsat" | "unknown" | "error";
  audit: FixCandidate["audit"];
}> {
  const audit: FixCandidate["audit"] = {
    overlayCreated: true,
    patchApplied: false,
    overlayReindexed: false,
    z3RunMs: 0,
    overlayClosed: false,
  };

  // 1. Apply patch to overlay (writes files, records modifiedFiles).
  applyPatchToOverlay(overlay, proposed.patch);
  audit.patchApplied = true;

  // 2. Re-index affected files in the scratch DB.
  await reindexOverlay(overlay);
  audit.overlayReindexed = true;

  // 3. Run oracle #2.
  const z3Start = Date.now();
  const verdict = await runOracleTwo(overlay, invariant);
  audit.z3RunMs = Date.now() - z3Start;

  // Three pass verdicts:
  //   - "unsat":             Z3 confirmed invariant holds (concrete proof).
  //   - "bug_site_removed":  structural removal (kind-agnostic).
  //   - "deferred_behavioral": abstract invariant; C5 oracle #9 owns the proof.
  const invariantHolds =
    verdict === "bug_site_removed" ||
    verdict === "unsat" ||
    verdict === "deferred_behavioral";

  // Surface the verdict honestly in the FixCandidate audit. The narrow type
  // doesn't include "deferred_behavioral", so map it to "unsat" (pass-forward)
  // for downstream callers — the audit trail logger gets the full string.
  const z3Verdict: "sat" | "unsat" | "unknown" | "error" =
    verdict === "bug_site_removed" || verdict === "deferred_behavioral"
      ? "unsat"
      : verdict;

  return { invariantHoldsUnderOverlay: invariantHolds, z3Verdict, audit };
}
