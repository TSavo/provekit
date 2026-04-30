/**
 * intent-from-diff stage — diff-driven IR formula proposal.
 *
 * Given a unified diff, its commit message, optional linked-ticket
 * summaries, and optional related test sources, ask an LLM what
 * property the developer was asserting and emit a proposed IR formula
 * as a memento. The proposal is content-addressed via (diffHash,
 * commitMessage, ticketContent, testsHashed, hostLanguageHint,
 * llmIdentifier, promptCid). When the prompt is revised, the
 * promptCid changes, the property hash changes, and prior cache
 * entries miss correctly — so a new question to the LLM does not
 * return a stale answer to the old question.
 *
 * Tests are existential evidence ("for THIS input, output is THIS").
 * The producer renders them as a `== TESTS ==` block so the LLM
 * weighs them when generalising to a universal invariant. See spec
 * `protocol/specs/2026-04-29-ts-ir-language.md` §15 ("Three-Step Unit of
 * Work" — tests are the highest-value intent source). NOTE: §2 ("Two
 * LLM Calls") currently names only LLM #2 as the consumer of tests;
 * threading them into intent-extraction (an LLM #1-side producer) is
 * a deliberate strengthening that flows the test signal upstream as
 * well. Reported as a spec gap.
 *
 * The witness column stores the LlmProposalEvidence body shape from the
 * universal claim envelope spec, plus an extension field
 * `__inferredIntent` so deserializeOutput round-trips exactly. The
 * envelope wrapper itself is constructed downstream; this Stage owns
 * only the witness shape.
 *
 * Spec: protocol/specs/2026-04-29-next-wave-prompts.md (Prompt 1).
 * Evidence shape: protocol/specs/2026-04-29-universal-claim-envelope.md
 * (LlmProposalEvidence body).
 */

import { hashCanonical } from "../../fix/runtime/mementoStore.js";
import type { LLMProvider } from "../../fix/types.js";
import type { Stage } from "../types.js";

export const INTENT_FROM_DIFF_CAPABILITY = "intent-from-diff";

export interface TicketRef {
  id: string;
  url?: string;
  summary?: string;
}

/**
 * How a test relates to the diff. Treated as a hint by the LLM, not a
 * strict typing — the relationship shapes the prompt section header
 * so the model knows whether the test was just authored ("added"),
 * carried forward ("preserves"), etc.
 *
 * - "added"     — new test introduced by the diff (strongest signal:
 *                 the developer wrote it as part of this change).
 * - "modified"  — pre-existing test the diff changed.
 * - "preserves" — pre-existing test in the modified file's parallel
 *                 test file. Must still pass after the change.
 * - "calls"     — test in another file that imports the modified
 *                 symbol. Constraint from the caller's perspective.
 */
export type TestRelationship =
  | "added"
  | "modified"
  | "preserves"
  | "calls";

export interface TestSource {
  /** Raw test source code. */
  source: string;
  /** Names of test functions extracted from `source` (best-effort). */
  testNames: string[];
  /** Where the test lives, relative to the project root. */
  filePath: string;
  /** How this test relates to the diff. */
  relationship: TestRelationship;
}

export interface IntentFromDiffInput {
  diff: string;
  commitMessage: string;
  linkedTickets?: TicketRef[];
  hostLanguageHint?: string;
  /**
   * Existential intent evidence: tests authored alongside or
   * surrounding the diff. The runner sorts and hashes test sources
   * for the cache key so order is irrelevant; the rendered prompt
   * also presents tests in a stable order.
   */
  tests?: TestSource[];
}

export interface IntentProposal {
  proposedIrFormula: string;
  confidence: number;
  rationale: string;
  inferredIntent: string;
  llm: string;
  llmVersion: string;
  promptCid: string;
}

export interface MakeIntentFromDiffStageDeps {
  llm: LLMProvider;
  /** Content hash of the prompt template currently in use. Hex32 CID. */
  promptCid: string;
  /** Format: "<llm>@<version>", e.g. "claude-opus@4-7". */
  llmIdentifier: string;
  /**
   * Override producer identity. Default encodes llmIdentifier + first
   * 8 hex chars of promptCid so a prompt revision yields a distinct
   * `producedBy` and a fresh row in the verifications table.
   */
  producerVersion?: string;
}

const PROMPT_TEMPLATE = `You read a diff and extract what property the developer was asserting.

Given:
  - a unified diff (the change)
  - the commit message (what the dev said they were doing)
  - optional linked tickets (the reported symptom)
  - optional related tests (existential evidence: "for THIS input, output is THIS")
  - optional host-language hint

You output JSON describing the underlying invariant the change encodes.

Tests, when supplied, are the strongest signal. Each test is a concrete
point the universal invariant must cover. Use them to anchor the
predicate, then generalise. The "added" relationship is strongest (the
developer wrote it as part of this change); "preserves" tells you the
prior behaviour must still hold; "calls" constrains the symbol from a
caller's perspective.

Output schema (JSON, no prose, no code fences):
{
  "inferredIntent": string,         // natural-language description of what the dev was trying to enforce
  "proposedIrFormula": string,      // serialized IR formula as a single line of pseudo-code
  "confidence": number,             // 0..1 — how confident you are this captures the real invariant
  "rationale": string               // why this formula matches the diff + commit + ticket evidence
}

Good extraction (study this):
  Diff:
    +  if denominator == 0 { return Err("divide by zero") }
       result = numerator / denominator
  Commit: "fix divide-by-zero crash in calculate()"
  Ticket: "INC-2847: calculate() crashes when called with b=0"

  Output:
  {
    "inferredIntent": "function calculate must not be called with a zero denominator",
    "proposedIrFormula": "forAll(call: CalculateCall) => call.b !== 0 OR call.returnsError()",
    "confidence": 0.92,
    "rationale": "The guard clause added in the diff directly encodes the invariant: when denominator is zero the function must return an error rather than dividing. The ticket confirms b=0 was the reported failure mode."
  }

Bad extraction (do NOT do this):
  {
    "inferredIntent": "fixed a bug",
    "proposedIrFormula": "true",
    "confidence": 0.5,
    "rationale": "the diff fixes something"
  }

  Why bad: the formula is vacuous (true is always true and verifies nothing); the
  intent is a tautology that names no specific property; the rationale restates the
  commit message instead of grounding in the diff. A useful proposal names a
  concrete predicate over a concrete subject.

Inputs follow.

`;

export function makeIntentFromDiffStage(
  deps: MakeIntentFromDiffStageDeps,
): Stage<IntentFromDiffInput, IntentProposal> {
  const producedBy =
    deps.producerVersion ??
    `intent-from-diff@v1+${deps.llmIdentifier}+${deps.promptCid.slice(0, 8)}`;

  const [llmName, llmVersion] = splitLlmIdentifier(deps.llmIdentifier);

  return {
    name: "intent-from-diff",
    producedBy,

    serializeInput(input) {
      // The raw diff can be megabytes; hash it before it enters the
      // property hash. promptCid + llmIdentifier are part of the cache
      // key so a prompt revision invalidates prior entries (the runner
      // looks up by (bindingHash, propertyHash) only).
      return {
        diffHash: hashCanonical(input.diff),
        commitMessage: input.commitMessage,
        ticketContent:
          input.linkedTickets && input.linkedTickets.length > 0
            ? input.linkedTickets
                .map((t) => t.summary ?? t.id)
                .sort()
                .join("|")
            : null,
        // Test sources can be arbitrarily large; hash each source and
        // sort by filePath so the cache key is stable under filesystem
        // ordering and shuffled discovery output. `undefined` and `[]`
        // both normalise to `null` so the two have identical hashes.
        tests: hashTests(input.tests),
        hostLanguageHint: input.hostLanguageHint ?? null,
        llmIdentifier: deps.llmIdentifier,
        promptCid: deps.promptCid,
      };
    },

    serializeOutput(output) {
      // Body shape matches LlmProposalEvidence from the universal
      // claim envelope spec. `__inferredIntent` is an extension field
      // so deserializeOutput can reconstruct the full IntentProposal
      // exactly; downstream envelope readers ignore unknown fields.
      const evidenceBody = {
        llm: output.llm,
        llmVersion: output.llmVersion,
        promptCid: output.promptCid,
        proposedIrFormula: output.proposedIrFormula,
        confidence: output.confidence,
        rationale: output.rationale,
        __inferredIntent: output.inferredIntent,
      };
      return JSON.stringify(evidenceBody);
    },

    deserializeOutput(witness) {
      const parsed = JSON.parse(witness) as {
        llm: string;
        llmVersion: string;
        promptCid: string;
        proposedIrFormula: string;
        confidence: number;
        rationale: string;
        __inferredIntent: string;
      };
      return {
        llm: parsed.llm,
        llmVersion: parsed.llmVersion,
        promptCid: parsed.promptCid,
        proposedIrFormula: parsed.proposedIrFormula,
        confidence: parsed.confidence,
        rationale: parsed.rationale,
        inferredIntent: parsed.__inferredIntent,
      };
    },

    async run(input) {
      const prompt = renderPrompt(PROMPT_TEMPLATE, input);
      const raw = await deps.llm.complete({ prompt });

      let parsed: {
        inferredIntent?: unknown;
        proposedIrFormula?: unknown;
        confidence?: unknown;
        rationale?: unknown;
      };
      try {
        parsed = JSON.parse(raw);
      } catch (err) {
        throw new Error(
          `intent-from-diff: LLM response was not valid JSON. ` +
            `Parse error: ${(err as Error).message}. Raw response: ${raw}`,
        );
      }

      const proposedIrFormula = asString(parsed.proposedIrFormula, "proposedIrFormula", raw);
      const inferredIntent = asString(parsed.inferredIntent, "inferredIntent", raw);
      const rationale = asString(parsed.rationale, "rationale", raw);
      const confidence = asConfidence(parsed.confidence, raw);

      return {
        proposedIrFormula,
        inferredIntent,
        rationale,
        confidence,
        llm: llmName,
        llmVersion,
        promptCid: deps.promptCid,
      };
    },
  };
}

function renderPrompt(template: string, input: IntentFromDiffInput): string {
  const ticketBlock =
    input.linkedTickets && input.linkedTickets.length > 0
      ? input.linkedTickets
          .map((t) => `- ${t.id}${t.summary ? `: ${t.summary}` : ""}`)
          .join("\n")
      : "(none)";

  return (
    template +
    `Diff:\n${input.diff}\n\n` +
    `Commit message: ${input.commitMessage}\n\n` +
    `Linked tickets:\n${ticketBlock}\n\n` +
    `== TESTS ==\n${renderTestsBlock(input.tests)}\n\n` +
    `Host-language hint: ${input.hostLanguageHint ?? "(unspecified)"}\n`
  );
}

/**
 * Hash each test's source and file path so the cache key is bounded
 * regardless of how large the test files are. Returns `null` for both
 * undefined and empty arrays so they collide.
 */
function hashTests(tests: TestSource[] | undefined): unknown {
  if (!tests || tests.length === 0) return null;
  return tests
    .map((t) => ({
      filePathHash: hashCanonical(t.filePath),
      sourceHash: hashCanonical(t.source),
      testNames: [...t.testNames].sort(),
      relationship: t.relationship,
    }))
    .sort((a, b) => a.filePathHash.localeCompare(b.filePathHash));
}

/**
 * Render tests as numbered, fenced blocks. Sorted by filePath for
 * deterministic prompt content (the cache key already sorts; this
 * keeps the LLM-visible text in lockstep so cache hits aren't
 * polluted by reordered prompts in the LLM provider's own logs).
 */
function renderTestsBlock(tests: TestSource[] | undefined): string {
  if (!tests || tests.length === 0) return "(none)";
  const sorted = [...tests].sort((a, b) =>
    a.filePath.localeCompare(b.filePath),
  );
  return sorted
    .map((t, i) => {
      const namesPart =
        t.testNames.length > 0
          ? ` (test names: ${[...t.testNames].sort().join(", ")})`
          : "";
      return [
        `[${i + 1}] ${t.relationship} test in ${t.filePath}${namesPart}:`,
        "```ts",
        t.source,
        "```",
      ].join("\n");
    })
    .join("\n\n");
}

function splitLlmIdentifier(id: string): [string, string] {
  const at = id.lastIndexOf("@");
  if (at === -1) return [id, ""];
  return [id.slice(0, at), id.slice(at + 1)];
}

function asString(value: unknown, field: string, raw: string): string {
  if (typeof value !== "string") {
    throw new Error(
      `intent-from-diff: LLM response missing string field "${field}". Raw response: ${raw}`,
    );
  }
  return value;
}

function asConfidence(value: unknown, raw: string): number {
  if (typeof value !== "number" || Number.isNaN(value) || value < 0 || value > 1) {
    throw new Error(
      `intent-from-diff: LLM response field "confidence" must be a number in [0,1]. Raw response: ${raw}`,
    );
  }
  return value;
}
