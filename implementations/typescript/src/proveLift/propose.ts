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
 * Compose the fixed forall scaffold from the function's parameter
 * sorts. v0 quantifies over Int parameters and treats String / Bool
 * params as opaque "any value of that sort." Run-2 may refine this
 * (e.g. quantify only over the parameter whose name matches the LLM's
 * proposed body, or all parameters if multivariate).
 */
function buildQuantifierShape(shape: FunctionShape): string {
  // For v0 + parseInt, the binder is `n: Int` so the body can refer to
  // both `n` and the function call `parseInt(String(n))`. We choose `n`
  // as the canonical Int binder name for the parseInt fixture; for
  // String parameters we use `s`, for Bool we use `b`. Multi-arg cases
  // produce numbered binders. None of this is normative; the LLM is
  // told what the binders are and writes a body that uses them.
  if (shape.params.length === 0) {
    return `forall: <PREDICATE_BODY>`;
  }
  if (shape.params.length === 1) {
    const sort = shape.params[0]!.sort.name;
    const binder = sort === "Int" ? "n" : sort === "String" ? "s" : "b";
    return `forall ${binder}: ${sort}.\n  <PREDICATE_BODY>`;
  }
  const binders = shape.params
    .map((p, i) => `${pickBinder(p.sort.name, i)}: ${p.sort.name}`)
    .join(", ");
  return `forall ${binders}.\n  <PREDICATE_BODY>`;
}

function pickBinder(sort: string, idx: number): string {
  const base = sort === "Int" ? "n" : sort === "String" ? "s" : "b";
  return idx === 0 ? base : `${base}${idx}`;
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
