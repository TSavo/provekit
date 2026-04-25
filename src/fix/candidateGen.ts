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
import { principleMatches } from "../db/schema/principleMatches.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";
import { verifyBlock } from "../verifier.js";
import { applyPatchToOverlay, reindexOverlay } from "./overlay.js";
import type {
  BugSignal,
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

export function buildFixPrompt(
  signal: BugSignal,
  locus: BugLocus,
  invariant: InvariantClaim,
  maxCandidates: number,
): string {
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

  return `You are a code-repair expert. Given a bug report and a formal invariant violation, propose up to ${maxCandidates} candidate patches.

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}${signal.fixHint ? `\nFix hint: ${signal.fixHint}` : ""}

Location: ${locus.file}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}

Source context:
\`\`\`
${sourceContext}
\`\`\`

Invariant violated: ${invariant.description}
Formal expression (SMT, violation state — must become unsat after fix):
${invariant.formalExpression}

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
export function buildAgentFixPrompt(
  signal: BugSignal,
  locus: BugLocus,
  invariant: InvariantClaim,
  overlay?: { worktreePath: string },
): string {
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

  return `Your CWD is the project root. All paths in this prompt are relative to your CWD. Do not use absolute paths — use only the relative paths shown here.

You are a code-repair expert. A bug has been identified in the codebase in your current working directory.

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}${signal.fixHint ? `\nFix hint: ${signal.fixHint}` : ""}

Location: ${locusDisplay}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}

Invariant violated: ${invariant.description}
Formal expression (SMT, violation state — must become unsat after fix):
${invariant.formalExpression}

Read the relevant files (using relative paths), understand the bug, and edit the file(s) to fix it. Do NOT run tests — just make the minimal change to fix the invariant violation. After making your changes, briefly explain what you changed and why.`;
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
  const kind = classifyInvariantKind(invariant);
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
