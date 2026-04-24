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
import { join } from "path";
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
// Response parser
// ---------------------------------------------------------------------------

/**
 * Parse the LLM response into ProposedFix[].
 * Skips malformed candidates with a console warning.
 * Throws if the response is not JSON or has zero valid candidates.
 */
export function parseProposedFixes(raw: string): ProposedFix[] {
  // Strip markdown fences if present.
  let cleaned = raw.trim();
  if (cleaned.startsWith("```")) {
    cleaned = cleaned.replace(/^```[a-z]*\n?/, "").replace(/```\s*$/, "").trim();
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(cleaned);
  } catch (err) {
    throw new Error(
      `parseProposedFixes: LLM response is not valid JSON: ${cleaned.slice(0, 200)}`,
    );
  }

  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !Array.isArray((parsed as Record<string, unknown>)["candidates"])
  ) {
    throw new Error(
      `parseProposedFixes: expected {"candidates": [...]} but got: ${cleaned.slice(0, 200)}`,
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

export type OracleTwoVerdict = "sat" | "unsat" | "unknown" | "error" | "bug_site_removed";

/**
 * Load the DSL source for a principle from the overlay's .neurallog/principles/ dir.
 * Returns null if not found.
 */
function loadPrincipleDslFromOverlay(
  overlay: OverlayHandle,
  principleId: string,
): string | null {
  const dslPath = join(overlay.worktreePath, ".neurallog", "principles", `${principleId}.dsl`);
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
 * Returns "bug_site_removed" when approach (a) confirms the fix structurally
 * removed the bug site. Returns "unsat" when Z3 confirms the negated goal is
 * unprovable. Returns "sat" | "unknown" | "error" for failure cases.
 *
 * Defers to approach (a) first:
 *   - Principle path: re-evaluate the principle's DSL against the overlay's
 *     scratch DB. Zero matches → bug_site_removed.
 *   - Novel path: check whether each SMT binding's source_expr appears in
 *     the overlay's modified file contents. All absent → bug_site_removed.
 *
 * Falls back to approach (b) — re-running verifyBlock on the original
 * formalExpression — only as a last resort check for novel invariants
 * where the source_expr strings still appear (treats as unknown/failure
 * per spec decision #6).
 */
export async function runOracleTwo(
  overlay: OverlayHandle,
  invariant: InvariantClaim,
): Promise<OracleTwoVerdict> {
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
        // Matches still exist — fix didn't remove the bug site.
        // We run verifyBlock on the original formalExpression here, but this is
        // intentionally doing REJECTION, not real Z3 verification.
        //
        // The formalExpression encodes the violation state (e.g., "denominator = 0"),
        // so Z3 will return "sat" (= violation is still satisfiable = fix did not help).
        // This correctly causes verifyCandidate to reject the candidate.
        //
        // NOTE: this code path does NOT verify that the fix makes the invariant hold.
        // It only confirms that the bug-site-removed check failed. If you ever need
        // real Z3 verification here, you would need to negate formalExpression first.
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
  // Fallback: run Z3 on the original formalExpression.
  // For novel invariants where the expression is self-contained SMT (no
  // source references), this is the only check available.
  // -----------------------------------------------------------------------
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

  const invariantHolds = verdict === "bug_site_removed" || verdict === "unsat";
  const z3Verdict: "sat" | "unsat" | "unknown" | "error" =
    verdict === "bug_site_removed" ? "unsat" : verdict;

  return { invariantHoldsUnderOverlay: invariantHolds, z3Verdict, audit };
}
