/**
 * B1.5 — Investigate.
 *
 * Bridges symptom-only bug reports to the rest of the fix-loop pipeline.
 * Intake produces a `IntentSignal` with prose summary + failure description;
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
import { getPromptStore } from "../../llm/promptStore.js";

// ---------------------------------------------------------------------------
// Investigate (B1.5) prompt artifact: investigate.prompt
//
// One bp namespace. Five runtime placeholders carry per-call context.
// Same pattern as C1/C3/C5 — coherence is global; bp.evolve operates on
// the whole composed prompt body. Day 0 byte-identical.
//
// Future evolution warning: bp.evolve on investigate.prompt MUST preserve
// {{SOURCE}}, {{SUMMARY}}, {{INTENT_TEXT}}, {{TOUR_TEXT}}, {{RECENT_TEXT}}
// placeholders verbatim.
// ---------------------------------------------------------------------------

const INVESTIGATE_PROMPT_TEMPLATE = `You are the Investigate stage of an intent loop. A user has supplied an intent — a bug report, a change request, or a property assertion. The intent describes a property the code should satisfy. Your job: read the intent, scan the project tour, propose where in the codebase this intent applies and where the change (if any) should land.

Output a JSON object via the Write tool. Schema:

{
  "symptomSummary": "1-sentence restatement of what the user wants",
  "rootCauseHypothesis": "1-2 sentences on the mechanical reason the property doesn't currently hold (for bugs) OR what currently exists at the change site (for change requests)",
  "fixHypothesis": "1-2 sentences on what kind of change would make the property hold",
  "primaryLocation": {
    "file": "path/relative/to/project/root.ts",
    "function": "optional function or method name",
    "lineRange": [optional inclusive start, end],
    "rationale": "why does the intent point here?",
    "confidence": "high" | "medium" | "low"
  },
  "candidateLocations": [
    /* up to 4 additional candidates, ordered by confidence descending */
  ]
}

== Intent ==

Source: {{SOURCE}}

Summary:
{{SUMMARY}}

User text:
{{INTENT_TEXT}}

== Project tour (source files only, tests excluded) ==

{{TOUR_TEXT}}

== Files changed in recent commits ==

{{RECENT_TEXT}}

== Reasoning hints ==

- The user describes WHAT property they want to hold (or which is failing), not WHERE in the code. Your job is the WHERE.
- For bugs: threshold-shaped symptoms (works small, breaks at scale) usually involve
  pagination, sort order, limits, or caching. "Stops working over time" usually
  involves accumulated state. Pick the file by NAME first, then narrow within the file.
- For change requests: the landing site is whatever code already does the closest
  thing, or is the natural anchor for the new behavior. "Add a verify-axioms
  subcommand" lands in the CLI dispatcher; "make X return Y instead of Z" lands
  in the function that produces the value.
- For property assertions: the property's bindings name specific symbols; locate
  by symbol resolution.
- Confidence "high" should mean "I'd bet money on it"; "low" means "worth
  checking but the intent is consistent with several places."

The structuredOutput layer will append the exact path to write to;
follow that instruction precisely. Do NOT invent a path or write to
the project root — the appended instruction is the contract.`;

const INVESTIGATE_PROMPT_DISCRIMINATOR = "2026-04-29";
import { getModelTier } from "../modelTiers.js";
import { type IntentSignal, type CodeReference, type LLMProvider, getIntentText } from "../types.js";
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
  signal: IntentSignal;
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
  const { prompt } = await buildInvestigatePrompt(signal, tour, recentChanges, projectRoot);

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

async function buildInvestigatePrompt(
  signal: IntentSignal,
  tour: TourEntry[],
  recentChanges: string[],
  projectRoot?: string,
): Promise<{ prompt: string; revisions: Array<{ key: string; revisionId: string }> }> {
  const tourText = tour.map((t) => `  ${t.path}  (${t.lines} lines)`).join("\n");
  const recentText = recentChanges.length > 0
    ? recentChanges.map((p) => `  ${p}`).join("\n")
    : "  (no git history available)";

  // Single bp artifact for the full Investigate prompt body. Day 0 byte-
  // identical; bp.evolve sees the whole composed prompt. No-op when
  // projectRoot is absent.
  const revisions: Array<{ key: string; revisionId: string }> = [];
  let templateBody = INVESTIGATE_PROMPT_TEMPLATE;
  if (projectRoot) {
    const rev = await getPromptStore(projectRoot).get(
      "investigate.prompt",
      INVESTIGATE_PROMPT_TEMPLATE,
      INVESTIGATE_PROMPT_DISCRIMINATOR,
    );
    templateBody = rev.body;
    revisions.push({ key: "investigate.prompt", revisionId: rev.id });
  }

  const renderVars: Record<string, string> = {
    SOURCE: signal.source,
    SUMMARY: signal.summary,
    INTENT_TEXT: getIntentText(signal),
    TOUR_TEXT: tourText,
    RECENT_TEXT: recentText,
  };
  let prompt = templateBody;
  for (const [k, v] of Object.entries(renderVars)) {
    prompt = prompt.replaceAll(`{{${k}}}`, v);
  }
  return { prompt, revisions };
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
  // Mark the primary location with isPrimary=true; carry each candidate's
  // confidence tier through as `investigateConfidence` so Locate can rank
  // resolved candidates by upstream conviction (high primary in real source
  // must beat a low candidate in a `reference/`-shaped subtree). See
  // locate.ts pickPrimary() for how these fields fold into the score.
  const all: { c: CandidateLocation; isPrimary: boolean }[] = [
    { c: report.primaryLocation, isPrimary: true },
    ...report.candidateLocations.map((c) => ({ c, isPrimary: false })),
  ];
  return all.map(({ c, isPrimary }) => {
    const ref: CodeReference = {
      file: c.file,
      investigateConfidence: c.confidence,
      isPrimary,
    };
    if (c.function) ref.function = c.function;
    if (c.lineRange) ref.line = c.lineRange[0];
    return ref;
  });
}

// Make sure existsSync isn't tree-shaken if needed for future fallback.
void existsSync;
