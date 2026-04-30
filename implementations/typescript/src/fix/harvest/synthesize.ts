/**
 * Phase 2-B input synthesis: turn a HarvestCandidate into the (BugSignal,
 * InvariantClaim, FixCandidate) tuple that C6's principle-distillation
 * primitive expects.
 *
 * The fix loop's production path produces these via Intake/Locate/Classify
 * (BugSignal), formulateInvariant (InvariantClaim), and generateFixCandidate
 * (FixCandidate). Harvest skips most of that: the upstream commit message +
 * diff already give us nearly everything. Only the InvariantClaim still needs
 * an LLM call (asking "what invariant does this fix establish?").
 *
 * synthesizeBugSignal is pure — no LLM, no I/O. It builds a BugSignal from
 * the candidate's upstreamFixMessage and changed-file metadata.
 *
 * synthesizeFixCandidate is pure — converts the diff's per-file content
 * pairs into a CodePatch. The harvest path skips the Z3-overlay verdict
 * (FixCandidate.invariantHoldsUnderOverlay) since we have ground truth: the
 * upstream maintainer merged this fix.
 */

import type {
  BugSignal,
  CodePatch,
  CodePatchFileEdit,
  FixCandidate,
} from "../types.js";
import type { HarvestCandidate } from "./extractBugs.js";

// ---------------------------------------------------------------------------
// BugSignal synthesis (mechanical)
// ---------------------------------------------------------------------------

/**
 * Build a BugSignal from a HarvestCandidate without an LLM call.
 *
 * - `summary`: first non-empty line of the upstream commit message.
 * - `failureDescription`: full commit message body (after the first line).
 * - `rawText`: full commit message verbatim (for fidelity / debugging).
 * - `codeReferences`: one entry per changed production file, with the line
 *   pinned to the first hunk in the diff for that file. Tests are excluded.
 * - `bugClassHint`: undefined; harvest deliberately doesn't pre-classify here.
 *   C6 derives `bug_class_id` from the principle structure, not the hint.
 */
export function synthesizeBugSignal(candidate: HarvestCandidate): BugSignal {
  const message = candidate.upstreamFixMessage.trim();
  const lines = message.split("\n");
  const summary = (lines.find((l) => l.trim().length > 0) ?? "").trim();
  const body = lines.slice(1).join("\n").trim();

  const failureDescription = body.length > 0 ? body : summary;

  // Code references: one per production file changed in the diff, line
  // taken from the first hunk for that file (best-effort).
  const codeReferences: BugSignal["codeReferences"] = [];
  const dirtyByFile = parseFirstHunkLines(candidate.diff);
  for (const path of Object.keys(candidate.fixedFiles)) {
    if (isTestPath(path)) continue;
    const line = dirtyByFile.get(path) ?? 1;
    codeReferences.push({
      file: path,
      line,
      function: "",
    });
  }

  return {
    source: "harvest",
    rawText: message,
    summary,
    failureDescription,
    codeReferences,
  };
}

// ---------------------------------------------------------------------------
// FixCandidate synthesis (mechanical)
// ---------------------------------------------------------------------------

/**
 * Build a FixCandidate from the candidate's fixedFiles map. The patch's
 * fileEdits each carry `newContent` (the post-fix content); buggyFiles
 * provide context but aren't part of CodePatch. invariantHoldsUnderOverlay
 * is set to `true` because the upstream maintainer merged this fix —
 * ground truth, not Z3-derived.
 */
export function synthesizeFixCandidate(candidate: HarvestCandidate): FixCandidate {
  const fileEdits: CodePatchFileEdit[] = [];
  for (const [path, newContent] of Object.entries(candidate.fixedFiles)) {
    if (isTestPath(path)) continue;
    fileEdits.push({ file: path, newContent });
  }

  const patch: CodePatch = {
    description: shortDescription(candidate),
    fileEdits,
  };

  return {
    patch,
    llmRationale: candidate.upstreamFixMessage,
    llmConfidence: 1.0,
    // Ground truth: this IS the upstream-merged fix.
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "ground-truth",
    audit: {
      overlayCreated: false,
      patchApplied: false,
      overlayReindexed: false,
      z3RunMs: 0,
      overlayClosed: false,
    },
  };
}

// ---------------------------------------------------------------------------
// InvariantClaim synthesis prompt (LLM call lives in discover.ts)
// ---------------------------------------------------------------------------

/**
 * Prompt template for the LLM call that produces an InvariantClaim from a
 * HarvestCandidate. Returned as a string so the caller controls the LLM
 * provider; tests can render the prompt without invoking an LLM.
 *
 * The LLM is asked for JSON with:
 *   description, kind, smt_declarations, smt_violation_assertion, bindings
 * mirroring the production C1 stage's output, but derived from the diff +
 * commit message rather than a bug report.
 */
export function buildInvariantSynthesisPrompt(candidate: HarvestCandidate): string {
  const message = candidate.upstreamFixMessage.trim();
  const diff = truncate(candidate.diff, 4000);
  return `[STAGE:harvest-invariant] synthesize InvariantClaim from a real upstream fix
You are a formal-verification expert. A maintainer landed the production fix
below. Distill the invariant the fix establishes — the property the fixed
code maintains that the buggy code violated.

# Upstream commit message
${message || "(no message)"}

# Diff (Bug-N..Bug-N-fix, production code only)
\`\`\`diff
${diff}
\`\`\`

# Output

Return JSON only, no prose, with these fields:

{
  "description": "<1-2 sentence prose: what invariant does the fix establish?>",
  "kind": "arithmetic" | "abstract" | "taint" | "set_uniqueness" | "cardinality" | "order",
  "smt_declarations": ["(declare-const x Int)", ...],
  "smt_violation_assertion": "(assert <negation of invariant>)",
  "bindings": [{"smt_constant": "x", "source_expr": "<JS expr>", "sort": "Int" | "Bool" | "String"}]
}

Pick the kind that best fits:
- arithmetic: the invariant is a numeric inequality / equality (e.g. denominator != 0)
- abstract: the invariant is a structural property over a collection or domain (e.g. no duplicates)
- taint: the invariant tracks a flow constraint (e.g. user input never reaches a sink)
- set_uniqueness, cardinality, order: refinements when the abstract property has a clear shape

Be tight. The invariant should be specific to THIS fix, not a general "code should not crash."`;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Parse the first hunk's start-line per file from a unified diff. */
function parseFirstHunkLines(diff: string): Map<string, number> {
  const out = new Map<string, number>();
  const lines = diff.split("\n");
  let currentPath: string | null = null;
  for (const line of lines) {
    const fileMatch = /^diff --git a\/(.+?) b\/(.+)$/.exec(line);
    if (fileMatch) {
      currentPath = fileMatch[2] ?? null;
      continue;
    }
    if (currentPath !== null && !out.has(currentPath)) {
      const hunkMatch = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
      if (hunkMatch && hunkMatch[1]) {
        out.set(currentPath, parseInt(hunkMatch[1], 10));
      }
    }
  }
  return out;
}

function isTestPath(p: string): boolean {
  const segments = p.split("/");
  for (const seg of segments) {
    if (seg === "test" || seg === "tests" || seg === "__tests__") return true;
  }
  return /\.(test|spec)\.[^/]+$/.test(p);
}

function shortDescription(candidate: HarvestCandidate): string {
  const summary = candidate.upstreamFixMessage.split("\n")[0]!.trim();
  if (summary.length > 0) return summary;
  return `${candidate.source.project} bug-${candidate.source.bugId} fix`;
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + `\n... (truncated, ${s.length - max} chars omitted)`;
}
