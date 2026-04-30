/**
 * Stage 2 of the prove-lift pipeline (STUB).
 *
 * Real Propose calls an LLM with the intake prompt at
 * src/proveLift/prompts/intake.md and parses its JSON response into a
 * `Candidate[]`. v0 (this run) ships a stub that returns the
 * hand-curated parseInt candidates from the design doc, plus reads
 * the intake prompt from disk so the wiring is exercised end-to-end.
 *
 * Wiring the real LLM is run-2 work. The interface below is stable;
 * only the implementation changes.
 */

import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

import type { FunctionShape } from "./detect.js";
import type { LiftDiagnostic } from "./errors.js";

export interface Candidate {
  /**
   * The predicate body as a TS expression source string. The framework
   * substitutes this into the fixed quantifier shape derived from the
   * function signature.
   */
  body: string;
  rationale: string;
}

export interface ProposeResult {
  candidates: Candidate[];
  diagnostics: LiftDiagnostic[];
  /** The verbatim prompt text after substitution. Stored for audit. */
  prompt: string;
}

export interface LiftLLM {
  /** Submit the intake prompt; receive the LLM's JSON-shaped string. */
  complete(prompt: string): Promise<string>;
}

export interface ProposeOptions {
  /**
   * If absent, propose() returns hardcoded candidates and only the
   * substituted prompt is observable. With a provider, propose() will
   * dispatch and parse. Run-2 wires this end-to-end.
   */
  llm?: LiftLLM;
  /**
   * Override the intake prompt path (test-only).
   */
  promptPath?: string;
}

// commonjs: __dirname is available at runtime; resolve relative to it.
// Tests can override via ProposeOptions.promptPath.
const DEFAULT_PROMPT_PATH = resolve(__dirname, "prompts", "intake.md");

export async function propose(
  shape: FunctionShape,
  options: ProposeOptions = {},
): Promise<ProposeResult> {
  const promptPath = options.promptPath ?? DEFAULT_PROMPT_PATH;
  if (!existsSync(promptPath)) {
    throw new Error(`propose: intake prompt missing at ${promptPath}`);
  }
  const template = readFileSync(promptPath, "utf8");
  const prompt = substituteIntakePlaceholders(template, shape);

  if (options.llm) {
    // Run-2 path. Not exercised in v0.
    const raw = await options.llm.complete(prompt);
    const parsed = parseLlmJson(raw);
    return { candidates: parsed, diagnostics: [], prompt };
  }

  // v0 stub path: return the curated parseInt candidates from the
  // design doc. The values are NOT a default behavior; they are a
  // placeholder so downstream stages can be exercised against a
  // realistic candidate shape until the real LLM is wired.
  return {
    candidates: stubCandidatesFor(shape),
    diagnostics: [],
    prompt,
  };
}

function substituteIntakePlaceholders(
  template: string,
  shape: FunctionShape,
): string {
  const paramTable = shape.params
    .map((p) => `${p.name}: ${p.sort.name}`)
    .join(", ");
  const quantifierShape = buildQuantifierShape(shape);
  return template
    .replaceAll("{{function_name}}", shape.name)
    .replaceAll("{{function_source}}", shape.functionSource)
    .replaceAll("{{quantifier_shape}}", quantifierShape)
    .replaceAll("{{param_table}}", paramTable)
    .replaceAll("{{return_sort}}", shape.returnSort.name);
}

/**
 * Compose the fixed forall scaffold.
 *
 * Important design correction (v0): the binder sort is the function's
 * RETURN sort, not a parameter sort. Lift expresses "what must hold of
 * the function's output." For parseInt(s: string): number, the binder
 * is `n: Int` so the property can read `parseInt(String(n)) === n` -
 * that matches the hand-authored fixture (propertyHash 8c38f05152707736).
 *
 * Quantifying over parameter sorts (the obvious-but-wrong rule) makes
 * the parseInt CID-equivalence acceptance gate impossible: the
 * hand-authored fixture quantifies over Int and reaches String via the
 * `String(n)` coercion in the body, not via the binder.
 *
 * The legal-sort universe surfaced to the LLM is `{returnSort} U
 * {paramSorts}`. v0 scaffold picks the return sort as the binder; run-2
 * generalizes to "LLM proposes binder sort, Detect constrains to the
 * legal universe."
 */
function buildQuantifierShape(shape: FunctionShape): string {
  if (shape.params.length === 0 && shape.returnSort === undefined) {
    return `forall: <PREDICATE_BODY>`;
  }
  const sort = shape.returnSort.name;
  const binder = canonicalBinder(sort);
  return `forall ${binder}: ${sort}.\n  <PREDICATE_BODY>`;
}

function canonicalBinder(sort: string): string {
  if (sort === "Int") return "n";
  if (sort === "String") return "s";
  if (sort === "Bool") return "b";
  return "x";
}

function stubCandidatesFor(shape: FunctionShape): Candidate[] {
  // The stub only knows parseInt. Anything else gets a single trivial
  // candidate so Filter has SOMETHING to drop, and the failure mode
  // surfaces as `all-candidates-dropped` rather than silent success.
  if (shape.name === "parseInt") {
    return [
      {
        body: "n >= 0 ? parseInt(String(n)) === n : true",
        rationale:
          "parseInt round-trips on non-negative integers; this matches the hand-authored fixture.",
      },
      {
        body: "n > 0 ? parseInt(String(n)) === n : true",
        rationale: "Stricter: positive only.",
      },
      {
        body: "parseInt(String(n)) === n",
        rationale: "Total: round-trip on every Int.",
      },
    ];
  }
  return [
    {
      body: "true",
      rationale: "stub-only candidate; replace by wiring a real LLM provider.",
    },
  ];
}

function parseLlmJson(raw: string): Candidate[] {
  // Run-2 hardens this. v0 keeps the path callable but unsafe by
  // design; the stub does not exercise it.
  const parsed = JSON.parse(raw) as
    | { candidates: Candidate[] }
    | { refuse: string };
  if ("refuse" in parsed) {
    throw new Error(`propose: LLM refused: ${parsed.refuse}`);
  }
  return parsed.candidates;
}
