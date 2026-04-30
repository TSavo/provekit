/**
 * reevaluateInvariant Stage — LLM judgment on whether a decayed invariant
 * still holds against an edited function.
 *
 * Architectural placement: case 3 of the four-way binding state machine
 * (see src/fix/runtime/substrate.ts). Case 3 is "the function's content
 * hash present at mint time no longer resolves in the substrate" —
 * i.e., the function was edited, not just shifted. Mechanical recovery
 * cannot tell whether the edit preserved, strengthened, weakened, or
 * refuted the invariant. That's a reasoning question; the LLM is the
 * right tool.
 *
 * The output is structured: a verdict that maps onto the workflows we
 * already have:
 *
 *   verdict      → next workflow
 *   ──────────────────────────
 *   holds        → re-bind invariant with current functionHash + offset
 *   strengthened → strengthen workflow with the LLM's tighter claim
 *   weakened     → weaken workflow with the LLM's looser claim
 *   refuted      → refute workflow (Z3 finds the counterexample)
 *   gone         → retire workflow with the LLM's rationale
 *   uncertain    → surface to human; do NOT auto-route
 *
 * Cache: pure given (invariant CID, current function body bytes, model).
 * Re-running with the same inputs returns the same verdict; the runner
 * caches under those keys.
 *
 * Spec: docs/specs/2026-04-29-the-semantic-envelope.md (case 3 routing
 * is a corollary of "code-invariant claims compose by CID; verdicts on
 * those claims are downstream Stages").
 */

import { z } from "zod";
import type { LLMProvider } from "../../fix/types.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import type { Stage } from "../types.js";
import { requestStructuredJson } from "../../fix/llm/structuredOutput.js";

export const REEVALUATE_INVARIANT_CAPABILITY = "reevaluate-invariant";

export interface ReevaluateInvariantInput {
  invariant: StoredInvariant;
  /** Bytes of the function body at its current location, post-edit. */
  currentFunctionBody: string;
  /** Substrate's current subtreeHash for the function. Used in the cache key. */
  currentFunctionHash: string;
  /** Optional model override. Defaults to "sonnet". */
  model?: "haiku" | "sonnet" | "opus";
}

export type ReevaluateVerdict =
  | "holds"
  | "strengthened"
  | "weakened"
  | "refuted"
  | "gone"
  | "uncertain";

export interface ReevaluateInvariantOutput {
  verdict: ReevaluateVerdict;
  rationale: string;
  /** Present when verdict is strengthened or weakened: the new claim text. */
  newClaimText?: string;
  /** Suggested next workflow; null when verdict is "uncertain" or "holds". */
  recommendedWorkflow:
    | "strengthen"
    | "weaken"
    | "refute"
    | "retire"
    | "rebind"
    | null;
}

const ResponseSchema = z.object({
  verdict: z.enum(["holds", "strengthened", "weakened", "refuted", "gone", "uncertain"]),
  rationale: z.string().min(1),
  newClaimText: z.string().optional(),
  recommendedWorkflow: z.enum(["strengthen", "weaken", "refute", "retire", "rebind"]).nullable(),
});

const PROMPT_TEMPLATE = `You are evaluating whether a code invariant still holds after the function it bound to was edited.

# Context

A code invariant is a universal claim about a function's behavior, expressed as
a first-order-logic formula. Each invariant is content-addressed by its
canonical IR; that hash is the propertyHash. Invariants are bound to specific
code locations via a "callsite" reference.

When we minted this invariant, we recorded the function's content hash. The
function has since been edited, so the recorded hash no longer resolves in the
current code. We need to decide: does the edited function still satisfy the
original claim?

# The original invariant

Author's intent (free text): {{ORIGINATING_BUG}}

Formal claim (SMT-LIB):
\`\`\`
{{SMT_ASSERTION}}
\`\`\`

The function's name is \`{{FUNCTION_NAME}}\` (when known) and it lives in
\`{{FILE_PATH}}\`.

# The current function body (post-edit)

\`\`\`
{{CURRENT_FUNCTION_BODY}}
\`\`\`

# Your job

Read the formal claim and the current function body. Decide which of these is
true, then answer in JSON.

Six possible verdicts:

- "holds": the edit was cosmetic, refactoring, or otherwise behavior-
  preserving. The original claim still describes the function's behavior
  exactly. recommendedWorkflow: "rebind" (we'll re-anchor the invariant to
  the new content hash, no re-derivation needed).

- "strengthened": the new function body satisfies a STRICTER property than
  the original claim. The original claim still holds, but a tighter one is
  now true. Provide \`newClaimText\` describing the stronger property.
  recommendedWorkflow: "strengthen".

- "weakened": the new function body satisfies a WEAKER property than the
  original claim. The original claim no longer holds in full, but a looser
  property does. Provide \`newClaimText\` describing the weaker property.
  recommendedWorkflow: "weaken".

- "refuted": the new function body actively VIOLATES the original claim.
  There is at least one input under which the claim is false. recommendedWorkflow:
  "refute" (Z3 will find the concrete counterexample).

- "gone": the function has been deleted, or renamed beyond plausible match,
  or the body is so unrelated that the original claim is no longer about
  this code at all. recommendedWorkflow: "retire".

- "uncertain": you cannot decide without more context (missing imports,
  dependencies on external state, ambiguity in the claim). Briefly explain
  what would resolve the uncertainty. recommendedWorkflow: null.

# Rules

1. The author's intent text is informational, not authoritative. The
   formal SMT claim is the actual property. If the intent text and the
   SMT formula disagree, the SMT formula wins.

2. "holds" requires that the original claim is fully preserved. If the
   new body is correct in a STRICTER way, that's "strengthened" — not
   "holds." Be precise.

3. "refuted" is a strong claim; only use it when you can name a concrete
   input under which the property fails. If you suspect the property
   fails but cannot exhibit a witness, prefer "uncertain" so the refute
   workflow can run Z3 to confirm.

4. Do not embellish the rationale. State the case in two or three
   sentences. The downstream pipeline reads the verdict and the
   recommendedWorkflow; humans read the rationale only when something
   went wrong.

5. Output ONLY the JSON object — no preamble, no markdown fences, no
   commentary. Keys: verdict, rationale, newClaimText (optional),
   recommendedWorkflow.

# Examples

Original claim: "forall x: int, parseInt(toString(x)) = x"
Edit: function reformatted, body unchanged semantically.
→ {"verdict": "holds", "rationale": "Reformatting only; the parseInt-of-toString round-trip behavior is unchanged.", "recommendedWorkflow": "rebind"}

Original claim: "forall a, b: int, b != 0, divide(a, b) is finite"
Edit: divide function now ALSO checks for overflow and throws on it.
→ {"verdict": "strengthened", "rationale": "The new body adds an overflow check, which strengthens the original guarantee from 'finite given non-zero divisor' to 'finite given non-zero divisor and no overflow'.", "newClaimText": "forall a, b: int, b != 0 and not_overflows(a, b), divide(a, b) is finite", "recommendedWorkflow": "strengthen"}

Original claim: "no_log_in_validate: validate() does not call console.log"
Edit: validate() now logs to console.log when DEBUG=true.
→ {"verdict": "refuted", "rationale": "The new body explicitly calls console.log in the DEBUG=true branch, providing a concrete counterexample.", "recommendedWorkflow": "refute"}

Original claim: "reserveStock preserves available + reserved"
Edit: reserveStock function deleted; replaced with reserveItem which takes different arguments.
→ {"verdict": "gone", "rationale": "The function name and signature changed beyond renamed; the reservation invariant no longer has a binding here.", "recommendedWorkflow": "retire"}

# Output

Return ONLY the JSON object now.`;

export interface MakeReevaluateInvariantStageDeps {
  llm: LLMProvider;
  producerVersion?: string;
}

export function makeReevaluateInvariantStage(
  deps: MakeReevaluateInvariantStageDeps,
): Stage<ReevaluateInvariantInput, ReevaluateInvariantOutput> {
  const producedBy = deps.producerVersion ?? "reevaluateInvariant@v1";

  return {
    name: "reevaluateInvariant",
    producedBy,

    serializeInput(input) {
      // Cache key: invariant CID + current body hash + model. Two runs with
      // the same trio produce the same verdict (the LLM is treated as a
      // deterministic-ish oracle; same prompt + same model returns the
      // same answer modulo sampling, and the cache lets us treat that as
      // contractual).
      return {
        invariantId: input.invariant.id,
        currentFunctionHash: input.currentFunctionHash,
        model: input.model ?? "sonnet",
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ReevaluateInvariantOutput;
    },

    async run(input) {
      const prompt = PROMPT_TEMPLATE
        .replaceAll("{{ORIGINATING_BUG}}", input.invariant.originatingBug)
        .replaceAll("{{SMT_ASSERTION}}", input.invariant.smt.assertion)
        .replaceAll("{{FUNCTION_NAME}}", input.invariant.callsite.function ?? "(unknown)")
        .replaceAll("{{FILE_PATH}}", input.invariant.callsite.filePath)
        .replaceAll("{{CURRENT_FUNCTION_BODY}}", input.currentFunctionBody);

      const result = await requestStructuredJson({
        llm: deps.llm,
        prompt,
        schema: ResponseSchema,
        stage: "reevaluate-invariant",
        model: input.model ?? "sonnet",
      });
      return result.parsed as ReevaluateInvariantOutput;
    },
  };
}
