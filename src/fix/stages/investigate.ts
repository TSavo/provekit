/**
 * B1.5 — Investigate.
 *
 * Bridges symptom-only bug reports to the rest of the fix-loop pipeline.
 * Intake produces a `BugSignal` with prose summary + failure description;
 * for user-facing bug reports those `codeReferences` are typically empty
 * (real users don't know which file the bug lives in). Locate then has
 * nothing to resolve and the loop aborts.
 *
 * Investigate runs an LLM call that reads the bug summary + a tour of the
 * project (file tree, sizes, recent git activity) and proposes 3-5
 * candidate code sites where the bug most likely lives. Output is the
 * same `CodeReference` shape Intake produces, plus a structured JSON
 * report written to `.provekit/contexts/investigate-<ts>.json` so
 * downstream stages (and humans) can see the reasoning chain.
 *
 * Design notes:
 *   - Uses the same `requestStructuredJson` pattern as the C-stages —
 *     LLM writes JSON to disk via the Write tool; the helper reads back.
 *   - Project tour is files-only (paths + line counts) for v1. No source
 *     contents are sent. The LLM names files it wants, Locate then
 *     refines via SAST.
 *   - The `rootCauseHypothesis` and `fixHypothesis` fields are persisted
 *     for downstream stages (formulateInvariant, generateFixCandidate)
 *     to optionally consume — they're part of the structured artifact,
 *     not currently passed through as separate types. Future iterations
 *     can pipe them into invariant prompts.
 */

import { mkdirSync, writeFileSync, readdirSync, statSync, existsSync } from "fs";
import { join, relative } from "path";
import { execSync } from "child_process";
import { requestStructuredJson } from "../llm/structuredOutput.js";
import { getModelTier } from "../modelTiers.js";
import type { BugSignal, CodeReference, LLMProvider } from "../types.js";
import type { FixLoopLogger } from "../logger.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export type ConfidenceTier = "high" | "medium" | "low";

export interface CandidateLocation {
  file: string;
  /** Function or class member name if known; LLM-best-guess. */
  function?: string;
  /** Inclusive 1-based line range if the LLM can narrow it; otherwise omit. */
  lineRange?: [number, number];
  /** One-sentence rationale: why does the symptom point here? */
  rationale: string;
  confidence: ConfidenceTier;
}

export interface InvestigateReport {
  /** Restate the symptom in our own words. Forces the LLM to grok the report. */
  symptomSummary: string;
  /** What the LLM thinks the root cause is — a brief mechanical hypothesis. */
  rootCauseHypothesis: string;
  /** What change the LLM would propose — high-level shape, NOT a patch. */
  fixHypothesis: string;
  /**
   * The single most-likely site. Locate runs against this first; the
   * candidates list is the fallback if primary doesn't resolve.
   */
  primaryLocation: CandidateLocation;
  /** Up to 4 additional candidates ordered by confidence descending. */
  candidateLocations: CandidateLocation[];
}

export interface InvestigateOptions {
  signal: BugSignal;
  projectRoot: string;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Where to write the structured JSON. Defaults to `<projectRoot>/.provekit/contexts/`. */
  reportDir?: string;
  /** Cap project-tour entries to this many (avoids prompt blowup on monorepos). Default 200. */
  maxTreeEntries?: number;
}

export interface InvestigateResult {
  report: InvestigateReport;
  /** Where the report was persisted. */
  reportPath: string;
  /** All candidate locations flattened to CodeReference[] for Locate consumption. */
  codeReferences: CodeReference[];
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function investigate(opts: InvestigateOptions): Promise<InvestigateResult> {
  const { signal, projectRoot, llm, logger } = opts;
  const reportDir = opts.reportDir ?? join(projectRoot, ".provekit", "contexts");
  const maxEntries = opts.maxTreeEntries ?? 200;

  const tour = buildProjectTour(projectRoot, maxEntries);
  const recentChanges = getRecentChanges(projectRoot);
  const prompt = buildInvestigatePrompt(signal, tour, recentChanges);

  logger?.info(
    `  Investigate prompt: ${tour.length} files in tour, ${recentChanges.length} recent changes`,
  );

  const report = await requestStructuredJson<InvestigateReport>({
    prompt,
    llm,
    stage: "B1.5-Investigate",
    model: getModelTier("intake-report"),
    schemaCheck: validateInvestigateReport,
    logger,
  });

  // Persist the structured report.
  mkdirSync(reportDir, { recursive: true });
  const ts = new Date().toISOString().replace(/[:.]/g, "-");
  const reportPath = join(reportDir, `investigate-${ts}.json`);
  writeFileSync(reportPath, JSON.stringify(report, null, 2), "utf-8");

  const codeReferences = reportToCodeReferences(report, projectRoot);

  return { report, reportPath, codeReferences };
}

// ---------------------------------------------------------------------------
// Project tour
// ---------------------------------------------------------------------------

interface TourEntry {
  path: string;
  lines: number;
}

const SKIP_DIRS = new Set([
  "node_modules", "dist", ".git", ".provekit", ".next",
  "coverage", "build", "out", ".turbo", ".cache",
]);

const SOURCE_EXTS = new Set([".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"]);

function buildProjectTour(projectRoot: string, maxEntries: number): TourEntry[] {
  const out: TourEntry[] = [];

  function walk(dir: string): void {
    if (out.length >= maxEntries) return;
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }
    for (const name of entries) {
      if (out.length >= maxEntries) return;
      if (SKIP_DIRS.has(name)) continue;
      const full = join(dir, name);
      let st;
      try { st = statSync(full); } catch { continue; }
      if (st.isDirectory()) {
        walk(full);
      } else if (st.isFile()) {
        const ext = name.slice(name.lastIndexOf("."));
        if (!SOURCE_EXTS.has(ext)) continue;
        if (/\.(test|spec|d)\.[a-z]+$/i.test(name)) continue;
        try {
          const content = require("fs").readFileSync(full, "utf-8") as string;
          const lines = content.split("\n").length;
          out.push({ path: relative(projectRoot, full), lines });
        } catch {
          /* skip unreadable */
        }
      }
    }
  }
  walk(projectRoot);
  return out;
}

function getRecentChanges(projectRoot: string): string[] {
  // Files touched in the last 10 commits — high signal for "what's been
  // active lately." If git fails or the repo has no history, return empty.
  try {
    const out = execSync(
      "git log -10 --name-only --pretty=format: 2>/dev/null",
      { cwd: projectRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "ignore"] },
    );
    const seen = new Set<string>();
    for (const line of out.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      seen.add(trimmed);
    }
    return [...seen].slice(0, 30);
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

function buildInvestigatePrompt(
  signal: BugSignal,
  tour: TourEntry[],
  recentChanges: string[],
): string {
  const tourText = tour.map((t) => `  ${t.path}  (${t.lines} lines)`).join("\n");
  const recentText = recentChanges.length > 0
    ? recentChanges.map((p) => `  ${p}`).join("\n")
    : "  (no git history available)";

  return `You are the Investigate stage of a fix loop. A user has reported a bug
in their project. They describe symptoms, not code. Your job: read the
symptom, scan the project tour, propose where the bug most likely lives.

Output a JSON object via the Write tool. Schema:

{
  "symptomSummary": "1-sentence restatement of what the user observes",
  "rootCauseHypothesis": "1-2 sentences on the mechanical cause",
  "fixHypothesis": "1-2 sentences on what kind of change would fix it",
  "primaryLocation": {
    "file": "path/relative/to/project/root.ts",
    "function": "optional function or method name",
    "lineRange": [optional inclusive start, end],
    "rationale": "why does the symptom point here?",
    "confidence": "high" | "medium" | "low"
  },
  "candidateLocations": [
    /* up to 4 additional candidates, ordered by confidence descending */
  ]
}

== Bug report ==

Source: ${signal.source}

Summary:
${signal.summary}

Failure description:
${signal.failureDescription}
${signal.fixHint ? `\nFix hint:\n${signal.fixHint}\n` : ""}
== Project tour (source files only, tests excluded) ==

${tourText}

== Files changed in recent commits ==

${recentText}

== Reasoning hints ==

- The user describes WHAT they see, not WHERE in the code. Your job is the WHERE.
- Threshold-shaped symptoms (works small, breaks at scale) usually involve
  pagination, sort order, limits, or caching. Look for query/repository code.
- "Stops working over time" usually involves accumulated state — a list that
  grows, a cache that doesn't invalidate, a counter that overflows.
- Pick the file by NAME first (does the path describe the function the user
  describes?), then narrow within the file.
- Confidence "high" should mean "I'd bet money on it"; "low" means "worth
  checking but the symptom is consistent with several places."

Now write the JSON to a file via the Write tool. Filename: investigate.json
relative to your working directory.
`.trim();
}

// ---------------------------------------------------------------------------
// Schema check + conversion
// ---------------------------------------------------------------------------

function validateInvestigateReport(parsed: unknown): InvestigateReport {
  if (!parsed || typeof parsed !== "object") {
    throw new Error("Investigate: response is not an object");
  }
  const r = parsed as Record<string, unknown>;
  if (typeof r.symptomSummary !== "string") throw new Error("missing symptomSummary");
  if (typeof r.rootCauseHypothesis !== "string") throw new Error("missing rootCauseHypothesis");
  if (typeof r.fixHypothesis !== "string") throw new Error("missing fixHypothesis");
  if (!r.primaryLocation || typeof r.primaryLocation !== "object") {
    throw new Error("missing primaryLocation");
  }
  const primary = validateCandidate(r.primaryLocation, "primaryLocation");
  const candidates = Array.isArray(r.candidateLocations) ? r.candidateLocations : [];
  const validatedCandidates = candidates.map((c, i) => validateCandidate(c, `candidateLocations[${i}]`));
  return {
    symptomSummary: r.symptomSummary,
    rootCauseHypothesis: r.rootCauseHypothesis,
    fixHypothesis: r.fixHypothesis,
    primaryLocation: primary,
    candidateLocations: validatedCandidates,
  };
}

function validateCandidate(c: unknown, where: string): CandidateLocation {
  if (!c || typeof c !== "object") throw new Error(`${where} is not an object`);
  const r = c as Record<string, unknown>;
  if (typeof r.file !== "string" || r.file.length === 0) {
    throw new Error(`${where}.file missing or empty`);
  }
  if (typeof r.rationale !== "string") {
    throw new Error(`${where}.rationale missing`);
  }
  if (r.confidence !== "high" && r.confidence !== "medium" && r.confidence !== "low") {
    throw new Error(`${where}.confidence must be high|medium|low (got ${JSON.stringify(r.confidence)})`);
  }
  const out: CandidateLocation = {
    file: r.file,
    rationale: r.rationale,
    confidence: r.confidence,
  };
  if (typeof r.function === "string" && r.function.length > 0) out.function = r.function;
  if (Array.isArray(r.lineRange) && r.lineRange.length === 2 && typeof r.lineRange[0] === "number" && typeof r.lineRange[1] === "number") {
    out.lineRange = [r.lineRange[0], r.lineRange[1]];
  }
  return out;
}

function reportToCodeReferences(report: InvestigateReport, _projectRoot: string): CodeReference[] {
  const all = [report.primaryLocation, ...report.candidateLocations];
  return all.map((c) => {
    const ref: CodeReference = { file: c.file };
    if (c.function) ref.function = c.function;
    if (c.lineRange) ref.line = c.lineRange[0];
    return ref;
  });
}

// Make sure existsSync isn't tree-shaken if needed for future fallback.
void existsSync;
