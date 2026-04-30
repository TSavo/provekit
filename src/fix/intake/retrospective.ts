/**
 * B0 retrospective intake.
 *
 * Companion to the prospective intake (which takes a user-filed problem
 * statement and feeds it through Investigate / Locate / Classify / C1 / ... /
 * D2). The retrospective direction takes an EXISTING change — a git commit
 * (sha or diff+message) — and uses an LLM to derive the same kind of intent:
 * "what was this change trying to accomplish? what property does it establish?
 * is there a regression test that would lock that property in? what would the
 * SMT-LIB shape of that property be?"
 *
 * Both directions converge on a single canonical artifact: the IntentReport.
 * Downstream gates (Z3 SAT, fidelity check, mutation verification,
 * no-existing-violation) are identical regardless of which intake direction
 * fed the pipeline.
 *
 * Reference: protocol/specs/2026-04-27-standing-invariant-runtime.md
 *            ("Intake unification (v1)" section, B0 stage).
 *            protocol/specs/2026-04-27-constraint-driven-development.md
 *            ("The intent report" section, schema source-of-truth).
 *
 * v1 scope: intent extraction only. This module is independently testable;
 * wiring into the orchestrator is a follow-up step. No commits, no patch
 * generation, no test synthesis here — those are downstream pipeline work.
 */

import { execFileSync } from "child_process";
import type { LLMProvider } from "../types.js";
import { requestStructuredJson } from "../llm/structuredOutput.js";
import { getModelTier } from "../modelTiers.js";
import { validateIntentReport } from "../../contracts/intentReport.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/**
 * Inputs accepted by the retrospective intake.
 *
 * Either:
 *   - commitSha alone (in which case the function shells out to `git show` to
 *     populate diff + commitMessage), OR
 *   - diff + commitMessage directly (callers that already have these in hand,
 *     e.g., a pre-commit hook with the staged diff already buffered, can skip
 *     the git invocation).
 *
 * If commitSha is provided AND diff/commitMessage are also provided, the
 * supplied values are used verbatim — the function does NOT cross-check.
 * That keeps the contract single-purpose: orchestrate inputs, extract intent.
 */
export interface RetrospectiveIntakeInput {
  commitSha?: string;
  diff?: string;
  commitMessage?: string;
}

/**
 * Per-binding citation linking an SMT clause to a span of source the change
 * introduced or modified. Optional — used by downstream traceability gates
 * when the LLM identifies the supporting code span. Mirrors
 * InvariantCitation in src/fix/types.ts but is locally defined here so this
 * module stays decoupled from the post-Investigate pipeline shape.
 */
export interface IntentReportCitation {
  smtClause: string;
  sourceQuote: string;
}

/**
 * One identified intent within a commit. A single commit may produce zero,
 * one, or many intents. Refactors / formatting / dependency bumps that
 * establish no constraint-shape property correctly produce zero intents.
 *
 * Schema mirrors the JSON shape in the constraint-driven-development spec
 * (top of file reference). lineRange is [startLine, endLine] inclusive,
 * 1-indexed against filePath in the post-commit tree.
 */
export interface IntentReportIntent {
  filePath: string;
  lineRange: [number, number];
  intent: string;
  hasRegressionTest: boolean;
  testGenerationOpportunity: boolean;
  /**
   * Constraint candidate when the intent is constraint-shaped (SMT-expressible
   * universal property). null when the intent is real but not constraint-
   * shaped (e.g., a dependency upgrade whose intent is "stay on supported
   * versions" — true, but nothing for Z3 to check).
   */
  constraintCandidate: IntentReportConstraintCandidate | null;
  /**
   * Optional traceability hooks for the downstream C1.5 fidelity check.
   * Empty array is valid; absent is also valid (we coerce to []).
   */
  citations?: IntentReportCitation[];
}

/**
 * Constraint candidate proposed by the LLM. validationStatus is always
 * "candidate" coming out of B0 — downstream gates promote it to z3_sat,
 * passed_oracles, or rejected.
 */
export interface IntentReportConstraintCandidate {
  smtSketch: string;
  /**
   * Constraint kind. Mirrors the "kind" field in the invariant store schema
   * (see standing-invariant-runtime.md §1, Invariant store schema). Open
   * string here to avoid coupling B0 to the closed enum; downstream stages
   * normalize via classifyInvariantKind.
   */
  kind: string;
  validationStatus: "candidate" | "z3_sat" | "passed_oracles" | "rejected";
}

/**
 * Output bundle. v1 of B0 retrospective produces an empty bundle (no patch
 * to ship — the change already landed; no tests yet — C5 generates those;
 * no constraint artifact yet — D2 writes that). Field is present in the
 * canonical schema regardless, so the JSON shape is identical across both
 * intake directions.
 */
export interface IntentReportOutputBundle {
  patch: string | null;
  addedTests: string[];
  constraintArtifact: string | null;
}

/** Trigger metadata: where this report came from. */
export interface IntentReportTrigger {
  kind: "problem_statement" | "commit";
  ref: string;
  diff?: string;
  commitMessage?: string;
}

/**
 * Canonical structured output of the B0 stage. Diffable, queryable, source-
 * controlled (downstream). Both prospective and retrospective intakes return
 * this shape; the rest of the pipeline reads it without knowing which intake
 * produced it.
 */
export interface IntentReport {
  source: "prospective" | "retrospective";
  trigger: IntentReportTrigger;
  intents: IntentReportIntent[];
  outputBundle: IntentReportOutputBundle;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/**
 * Thrown when the LLM's structured output cannot be coerced into an
 * IntentReport. Carries the parsed-but-invalid JSON in `received` so the
 * caller (and logs) can see exactly which field failed.
 */
export class IntentReportSchemaError extends Error {
  constructor(message: string, public readonly received?: unknown) {
    super(message);
    this.name = "IntentReportSchemaError";
  }
}

/**
 * Thrown when neither a usable diff nor a commitSha is supplied.
 */
export class RetrospectiveIntakeInputError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RetrospectiveIntakeInputError";
  }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/**
 * Extract an IntentReport from a git commit (sha, or pre-fetched diff +
 * message). The LLM proposes; downstream gates dispose. No verification
 * happens here — that's all post-B0.
 *
 * @param input        Either a commitSha (we shell out to git) or pre-fetched
 *                     diff + commitMessage. If commitSha is supplied with
 *                     missing diff/message, we run `git show` to populate.
 * @param llm          LLMProvider. If `llm.agent` is defined, structuredOutput
 *                     uses agent mode (preferred). Otherwise text mode.
 * @param projectRoot  Absolute path to the git repo. Used as cwd for git
 *                     invocations. Not used for any LLM tool-permission scope
 *                     (the structured-output helper writes to its own scratch
 *                     dir, not into projectRoot).
 */
export async function extractIntent(
  input: RetrospectiveIntakeInput,
  llm: LLMProvider,
  projectRoot: string,
): Promise<IntentReport> {
  const { diff, commitMessage, commitSha } = resolveDiffAndMessage(input, projectRoot);

  const ref = commitSha ?? "<inline-diff>";
  const trigger: IntentReportTrigger = {
    kind: "commit",
    ref,
    diff,
    commitMessage,
  };

  const prompt = buildIntentExtractionPrompt({ diff, commitMessage });

  const parsed = await requestStructuredJson<unknown>({
    prompt,
    llm,
    stage: "B0-retrospective",
    model: getModelTier("B0-retrospective"),
  });

  const intents = coerceIntents(parsed);

  const report: IntentReport = {
    source: "retrospective",
    trigger,
    intents,
    outputBundle: {
      patch: null,
      addedTests: [],
      constraintArtifact: null,
    },
  };

  return validateIntentReport(report);
}

// ---------------------------------------------------------------------------
// Internals: git plumbing
// ---------------------------------------------------------------------------

/**
 * Populate diff + commitMessage from whatever the caller supplied.
 *
 * Decision tree:
 *   - diff and commitMessage both present → use as-is, ignore commitSha for
 *     fetching (caller is authoritative).
 *   - commitSha present (and at least one of diff/message missing) → shell out
 *     to git for the missing piece(s).
 *   - nothing useful supplied → throw RetrospectiveIntakeInputError.
 *
 * The git invocations are split into two calls so we can populate either
 * field independently:
 *   `git show --no-patch --format=%B <sha>`  → commit message
 *   `git show --format= <sha>`               → unified diff (no message header)
 *
 * `--format=` (empty) is the cleanest way to suppress the header without also
 * suppressing the diff body. `--no-patch` is the inverse.
 */
function resolveDiffAndMessage(
  input: RetrospectiveIntakeInput,
  projectRoot: string,
): { diff: string; commitMessage: string; commitSha?: string } {
  const haveDiff = typeof input.diff === "string" && input.diff.length > 0;
  const haveMessage = typeof input.commitMessage === "string" && input.commitMessage.length > 0;

  if (haveDiff && haveMessage) {
    const out: { diff: string; commitMessage: string; commitSha?: string } = {
      diff: input.diff!,
      commitMessage: input.commitMessage!,
    };
    if (input.commitSha) out.commitSha = input.commitSha;
    return out;
  }

  if (!input.commitSha) {
    throw new RetrospectiveIntakeInputError(
      "extractIntent: must supply either commitSha, or both diff and commitMessage. " +
        `got: ${JSON.stringify({
          commitSha: input.commitSha ?? null,
          diffLen: input.diff?.length ?? 0,
          messageLen: input.commitMessage?.length ?? 0,
        })}`,
    );
  }

  const sha = input.commitSha;
  const diff = haveDiff ? input.diff! : runGit(projectRoot, ["show", "--format=", sha]);
  const commitMessage = haveMessage
    ? input.commitMessage!
    : runGit(projectRoot, ["show", "--no-patch", "--format=%B", sha]);

  return { diff, commitMessage, commitSha: sha };
}

function runGit(cwd: string, args: string[]): string {
  try {
    return execFileSync("git", args, {
      cwd,
      encoding: "utf-8",
      maxBuffer: 64 * 1024 * 1024, // large diffs happen
    });
  } catch (err) {
    throw new RetrospectiveIntakeInputError(
      `git ${args.join(" ")} failed in ${cwd}: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

// ---------------------------------------------------------------------------
// Internals: prompt construction
// ---------------------------------------------------------------------------

/**
 * Build the LLM prompt for retrospective intent extraction.
 *
 * Teaching strategy:
 *   1. State the goal in one sentence (extract intent, NOT describe what changed).
 *   2. Define "intent" precisely: a property the code now establishes.
 *   3. Give one positive worked example (asc/desc fix in repositories.ts) so the
 *      model sees what a constraint-shaped intent looks like end-to-end.
 *   4. Give one negative worked example (dependency bump) so the model knows
 *      "no intent" is a valid output.
 *   5. State the output schema with field-by-field semantics.
 *   6. Final instruction: emit JSON matching the schema.
 *
 * Worked examples are the load-bearing teaching mechanism per Sir's
 * "never speedrun prompt writing" rule. The examples are concrete, contain
 * SMT, and include the WHY for each non-obvious field choice.
 */
function buildIntentExtractionPrompt(args: {
  diff: string;
  commitMessage: string;
}): string {
  return [
    `You are the B0 intent extractor for ProveKit's constraint-driven-development pipeline.`,
    ``,
    `Your job: read a single git commit (diff + message) and identify the INTENTS the change establishes.`,
    ``,
    `What "intent" means here, precisely:`,
    `  An intent is a PROPERTY the code now pledges to satisfy. It is a universal-over-paths`,
    `  statement about what the code under the change must do, not a narration of what the`,
    `  diff syntactically changed. "Renamed foo to bar" is NOT an intent. "Function returns`,
    `  the most-recent K rows" IS an intent.`,
    ``,
    `Each intent you identify will, downstream, be:`,
    `  - Translated into an SMT-LIB invariant by C1`,
    `  - Z3-checked for SAT (the violation must be reachable on the OLD code)`,
    `  - Mutation-verified via a regression test`,
    `  - Stored in .provekit/invariants/ as a permanent obligation the codebase keeps`,
    `So an intent must be (a) shaped like a property, (b) tied to specific lines, (c)`,
    `something a Z3 query could express.`,
    ``,
    `Worked example 1 (positive — constraint-shaped intent):`,
    ``,
    `  Commit message:`,
    `    "fix: forRevision returns most-recent K invocations, not oldest"`,
    ``,
    `  Diff:`,
    `    --- a/src/store/sqlite/repositories.ts`,
    `    +++ b/src/store/sqlite/repositories.ts`,
    `    @@ -118,7 +118,7 @@`,
    `         .from(schema.invocations)`,
    `         .where(eq(schema.invocations.revisionId, revId))`,
    `    -    .orderBy(asc(schema.invocations.date))`,
    `    +    .orderBy(desc(schema.invocations.date))`,
    `         .limit(k);`,
    ``,
    `  Correct intent extraction:`,
    `    {`,
    `      "filePath": "src/store/sqlite/repositories.ts",`,
    `      "lineRange": [115, 124],`,
    `      "intent": "forRevision returns the K most-recent invocations by date, not the K oldest",`,
    `      "hasRegressionTest": false,`,
    `      "testGenerationOpportunity": true,`,
    `      "constraintCandidate": {`,
    `        "smtSketch": "(declare-const k Int) (declare-const result_max_date Int) (declare-const total_max_date Int) (assert (and (> k 0) (< result_max_date total_max_date)))",`,
    `        "kind": "order",`,
    `        "validationStatus": "candidate"`,
    `      },`,
    `      "citations": [`,
    `        { "smtClause": "(< result_max_date total_max_date)", "sourceQuote": ".orderBy(desc(schema.invocations.date))" }`,
    `      ]`,
    `    }`,
    ``,
    `  Why this is an intent (and not "the diff replaced asc with desc"):`,
    `    The change establishes a property: any path through forRevision now returns`,
    `    the K most-recent rows. That property is universal over inputs (any revId, any k > 0)`,
    `    and is Z3-expressible (the max date in the result must equal the max date over the`,
    `    full filtered set). The diff is one example of HOW the property gets satisfied;`,
    `    the property itself is the intent.`,
    ``,
    `Worked example 2 (negative — no constraint-shape intent):`,
    ``,
    `  Commit message:`,
    `    "deps: bump zod from 3.22.4 to 3.23.8"`,
    ``,
    `  Diff:`,
    `    --- a/package.json`,
    `    +++ b/package.json`,
    `    @@ -42,7 +42,7 @@`,
    `       "dependencies": {`,
    `    -    "zod": "^3.22.4"`,
    `    +    "zod": "^3.23.8"`,
    ``,
    `  Correct extraction:`,
    `    intents: []`,
    ``,
    `  Why empty: the change has no SMT-expressible property attached to it. "We're on a`,
    `  newer zod" is real but is not a universal-over-paths property of the codebase. Refactors`,
    `  with no behavior change, formatting-only commits, comment edits, and dependency bumps`,
    `  ALL correctly produce intents: [].`,
    ``,
    `Output schema (return JSON exactly matching this shape):`,
    `{`,
    `  "intents": [`,
    `    {`,
    `      "filePath": "<repo-relative path the intent applies to>",`,
    `      "lineRange": [<startLine>, <endLine>],`,
    `      "intent": "<one sentence describing the property, in plain English>",`,
    `      "hasRegressionTest": <true if the diff itself includes a test that locks the intent in, false otherwise>,`,
    `      "testGenerationOpportunity": <true when hasRegressionTest is false AND the intent is testable>,`,
    `      "constraintCandidate": {`,
    `        "smtSketch": "<SMT-LIB sketch encoding the violation state — assert-the-negation, not assert-the-goal>",`,
    `        "kind": "<arithmetic | set_uniqueness | cardinality | order | taint | other>",`,
    `        "validationStatus": "candidate"`,
    `      } | null,`,
    `      "citations": [ { "smtClause": "...", "sourceQuote": "..." } ]   // optional`,
    `    }`,
    `  ]`,
    `}`,
    ``,
    `Field rules:`,
    `  - intents may be []. Empty is the right answer for refactors / formatting / dependency bumps.`,
    `  - constraintCandidate may be null when the intent is real but not constraint-shaped`,
    `    (e.g., "use a clearer variable name"). If you cannot write an SMT sketch, use null,`,
    `    do not invent one.`,
    `  - validationStatus is ALWAYS "candidate" coming out of B0. Downstream gates promote it.`,
    `  - kind is one of: arithmetic, set_uniqueness, cardinality, order, taint, other.`,
    `    Pick the closest match; "other" is a valid escape hatch.`,
    `  - lineRange refers to lines in the POST-commit tree (the new state).`,
    `  - smtSketch encodes the VIOLATION state — what we'd want Z3 to find UNSAT after the fix.`,
    `    Express the negation of the goal, not the goal itself.`,
    ``,
    `Now extract intent from the following commit:`,
    ``,
    `=== COMMIT MESSAGE ===`,
    args.commitMessage.trim(),
    `=== END COMMIT MESSAGE ===`,
    ``,
    `=== UNIFIED DIFF ===`,
    args.diff,
    `=== END UNIFIED DIFF ===`,
    ``,
    `Return JSON only. The top-level object must have a single "intents" key whose value`,
    `is the (possibly empty) array described above.`,
  ].join("\n");
}

// ---------------------------------------------------------------------------
// Internals: schema validation / coercion
// ---------------------------------------------------------------------------

/**
 * Validate the LLM's parsed JSON and coerce it into IntentReportIntent[].
 * Throws IntentReportSchemaError on any structural mismatch — the message
 * names the offending path, and `received` carries the raw object so a human
 * can inspect.
 *
 * The validation is strict on shape (required fields, correct types) but
 * permissive on string content (we don't enforce that `kind` is in a closed
 * set — downstream stages do that). This matches how the rest of the fix
 * loop validates LLM output: catch structural malformation early, defer
 * semantic validation to the gates.
 */
function coerceIntents(raw: unknown): IntentReportIntent[] {
  if (raw === null || typeof raw !== "object") {
    throw new IntentReportSchemaError(
      `IntentReport: expected object, got ${raw === null ? "null" : typeof raw}`,
      raw,
    );
  }

  const obj = raw as Record<string, unknown>;
  const intentsRaw = obj["intents"];

  if (!Array.isArray(intentsRaw)) {
    throw new IntentReportSchemaError(
      `IntentReport.intents: expected array, got ${intentsRaw === null ? "null" : typeof intentsRaw}`,
      raw,
    );
  }

  return intentsRaw.map((item, idx) => coerceOneIntent(item, idx, raw));
}

function coerceOneIntent(item: unknown, idx: number, full: unknown): IntentReportIntent {
  if (item === null || typeof item !== "object") {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}]: expected object, got ${item === null ? "null" : typeof item}`,
      full,
    );
  }
  const obj = item as Record<string, unknown>;

  const filePath = obj["filePath"];
  if (typeof filePath !== "string" || filePath.length === 0) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].filePath: expected non-empty string, got ${typeof filePath}`,
      full,
    );
  }

  const lineRangeRaw = obj["lineRange"];
  if (
    !Array.isArray(lineRangeRaw) ||
    lineRangeRaw.length !== 2 ||
    typeof lineRangeRaw[0] !== "number" ||
    typeof lineRangeRaw[1] !== "number"
  ) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].lineRange: expected [number, number], got ${JSON.stringify(lineRangeRaw)}`,
      full,
    );
  }
  const lineRange: [number, number] = [lineRangeRaw[0], lineRangeRaw[1]];

  const intent = obj["intent"];
  if (typeof intent !== "string" || intent.length === 0) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].intent: expected non-empty string, got ${typeof intent}`,
      full,
    );
  }

  const hasRegressionTest = obj["hasRegressionTest"];
  if (typeof hasRegressionTest !== "boolean") {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].hasRegressionTest: expected boolean, got ${typeof hasRegressionTest}`,
      full,
    );
  }

  const testGenerationOpportunity = obj["testGenerationOpportunity"];
  if (typeof testGenerationOpportunity !== "boolean") {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].testGenerationOpportunity: expected boolean, got ${typeof testGenerationOpportunity}`,
      full,
    );
  }

  const constraintCandidate = coerceConstraintCandidate(obj["constraintCandidate"], idx, full);
  const citations = coerceCitations(obj["citations"], idx, full);

  const result: IntentReportIntent = {
    filePath,
    lineRange,
    intent,
    hasRegressionTest,
    testGenerationOpportunity,
    constraintCandidate,
  };
  if (citations !== undefined) result.citations = citations;
  return result;
}

function coerceConstraintCandidate(
  raw: unknown,
  idx: number,
  full: unknown,
): IntentReportConstraintCandidate | null {
  if (raw === null || raw === undefined) return null;
  if (typeof raw !== "object") {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].constraintCandidate: expected object or null, got ${typeof raw}`,
      full,
    );
  }

  const obj = raw as Record<string, unknown>;
  const smtSketch = obj["smtSketch"];
  if (typeof smtSketch !== "string" || smtSketch.length === 0) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].constraintCandidate.smtSketch: expected non-empty string, got ${typeof smtSketch}`,
      full,
    );
  }

  const kind = obj["kind"];
  if (typeof kind !== "string" || kind.length === 0) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].constraintCandidate.kind: expected non-empty string, got ${typeof kind}`,
      full,
    );
  }

  const validationStatus = obj["validationStatus"];
  if (
    validationStatus !== "candidate" &&
    validationStatus !== "z3_sat" &&
    validationStatus !== "passed_oracles" &&
    validationStatus !== "rejected"
  ) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].constraintCandidate.validationStatus: expected one of candidate|z3_sat|passed_oracles|rejected, got ${JSON.stringify(validationStatus)}`,
      full,
    );
  }

  return { smtSketch, kind, validationStatus };
}

function coerceCitations(
  raw: unknown,
  idx: number,
  full: unknown,
): IntentReportCitation[] | undefined {
  if (raw === undefined || raw === null) return undefined;
  if (!Array.isArray(raw)) {
    throw new IntentReportSchemaError(
      `IntentReport.intents[${idx}].citations: expected array or absent, got ${typeof raw}`,
      full,
    );
  }
  return raw.map((c, ci) => {
    if (c === null || typeof c !== "object") {
      throw new IntentReportSchemaError(
        `IntentReport.intents[${idx}].citations[${ci}]: expected object, got ${c === null ? "null" : typeof c}`,
        full,
      );
    }
    const co = c as Record<string, unknown>;
    const smtClause = co["smtClause"];
    const sourceQuote = co["sourceQuote"];
    if (typeof smtClause !== "string" || typeof sourceQuote !== "string") {
      throw new IntentReportSchemaError(
        `IntentReport.intents[${idx}].citations[${ci}]: expected {smtClause: string, sourceQuote: string}, got ${JSON.stringify(c)}`,
        full,
      );
    }
    return { smtClause, sourceQuote };
  });
}
