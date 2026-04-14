import { query } from "@anthropic-ai/claude-agent-sdk";
import { VerificationResult } from "./verifier";
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";

export interface Principle {
  id: string;
  name: string;
  description: string;
  teachingExample: {
    domain: string;
    explanation: string;
    smt2: string;
  };
  provenance: {
    discoveredIn: string;
    violation: string;
    generalizedAt: string;
  };
  validated: boolean;
  validationFailure?: string;
}

const SEED_AXIOMS = [
  {
    id: "P1",
    name: "Precondition Propagation",
    description:
      "Every function has preconditions. When function A calls function B, A must establish B's preconditions before the call. If it does not, the violation is reachable.",
    teachingExample:
      "transfer(from, to, amount) calls withdraw(account, amount) where withdraw requires amount <= account.balance. If transfer only checks amount > 0 but not amount <= balance, the violation is reachable.",
  },
  {
    id: "P2",
    name: "State Mutation Analysis",
    description:
      "When a function mutates shared state, subsequent calls reading that state see different values. Each mutation changes the precondition landscape for everything that follows. Loop iterations are NOT independent when they can alias the same shared state.",
    teachingExample:
      "A loop processes work items that can reference the same resource by ID. The first iteration reduces a budget; the second iteration on the same resource finds the budget exhausted.",
  },
  {
    id: "P3",
    name: "Calling Context Analysis",
    description:
      "Public functions can receive any input. The set of valid inputs is only what the function itself validates. Unvalidated inputs can violate any assumption the function's body makes.",
    teachingExample:
      "process_payment(invoice) trusts invoice.amount without validation. A negative payment is reachable because the function never checks.",
  },
  {
    id: "P4",
    name: "Temporal Analysis",
    description:
      "If the same function can be invoked multiple times on the same input, analyze whether the second invocation's preconditions still hold given the first invocation's side effects on shared state.",
    teachingExample:
      "ship_order(order) decrements inventory and sets order.shipped = true but doesn't check order.shipped before executing. A second call drives inventory negative.",
  },
  {
    id: "P5",
    name: "Semantic Correctness",
    description:
      "Beyond precondition violations, check whether the computed values are meaningful in the domain. A function might execute without error but produce a result that is semantically wrong — a refund that exceeds the payment, a price that is negative, a date in the past.",
    teachingExample:
      "calculate_discount(original_price, discount_percent) doesn't cap the discount. A 150% discount produces a negative price — semantically invalid even though the code runs without error.",
  },
  {
    id: "P6",
    name: "Boundary and Degenerate Inputs",
    description:
      "Functions that process collections or accumulate values can receive empty inputs, zero-valued inputs, or single-element inputs. The code may execute without error but produce a degenerate result — a zero total, an empty output, a no-op that still mutates state. Also covers boundary exposure: when input crosses a trust or format boundary (e.g., internal tokens passed to external contexts, raw user input reaching privileged operations), the boundary itself is the degenerate case.",
    teachingExample:
      "finalize_invoice(line_items) sums items and marks the invoice finalized. If line_items is empty, a zero-dollar invoice is finalized — masking upstream bugs. Similarly, a Bearer token included in an external-facing log or response crosses a trust boundary.",
  },
  {
    id: "P7",
    name: "Arithmetic Safety",
    description:
      "Division, modular arithmetic, and subtraction can produce undefined or unexpected results at boundary values. Division by zero is undefined. Subtraction can underflow. Integer division truncates.",
    teachingExample:
      "compute_average(total, count) divides total by count. If count comes from len(items) and items is empty, division by zero is reachable.",
  },
];

function getAdversaryModel(model: string): string {
  const lower = model.toLowerCase();
  if (lower.includes("opus")) return "sonnet";
  if (lower.includes("sonnet")) return "haiku";
  // Default: if haiku or unknown, use sonnet as adversary
  return "sonnet";
}

export class PrincipleStore {
  private principles: Principle[] = [];
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.loadFromDisk();
  }

  private get principlesDir(): string {
    return join(this.projectRoot, ".neurallog", "principles");
  }

  private loadFromDisk(): void {
    if (!existsSync(this.principlesDir)) return;

    for (const entry of readdirSync(this.principlesDir)) {
      if (!entry.endsWith(".json")) continue;
      try {
        const data: Principle = JSON.parse(
          readFileSync(join(this.principlesDir, entry), "utf-8")
        );
        this.principles.push(data);
      } catch {
        // skip corrupt files
      }
    }
  }

  getAll(): Principle[] {
    return [...this.principles];
  }

  add(principle: Principle): void {
    this.principles.push(principle);
    mkdirSync(this.principlesDir, { recursive: true });
    const filename = `${principle.id}.json`;
    writeFileSync(
      join(this.principlesDir, filename),
      JSON.stringify(principle, null, 2)
    );
  }

  /**
   * Compute a hash of all principle files on disk. Used as part of the
   * contract cache key so contracts are invalidated when principles change.
   */
  computePrincipleHash(): string {
    if (!existsSync(this.principlesDir)) return "";

    const hash = createHash("sha256");
    const entries = readdirSync(this.principlesDir)
      .filter((e) => e.endsWith(".json"))
      .sort(); // deterministic order

    for (const entry of entries) {
      const content = readFileSync(join(this.principlesDir, entry), "utf-8");
      hash.update(entry);
      hash.update(content);
    }

    return hash.digest("hex");
  }

  nextId(): string {
    const existing = this.principles.length + 8; // P1-P7 are seed, P8+ are discovered
    return `P${existing}`;
  }

  formatForPrompt(): string {
    if (this.principles.length === 0) return "";

    const sections: string[] = [];
    sections.push("\n#### Discovered Principles (system-generated)\n");

    for (const p of this.principles) {
      sections.push(`#### ${p.id}. ${p.name}\n`);
      sections.push(p.description + "\n");
      sections.push(`**Teaching example (${p.teachingExample.domain}):**\n`);
      sections.push(p.teachingExample.explanation + "\n");
      sections.push("```smt2");
      sections.push(p.teachingExample.smt2);
      sections.push("```\n");
    }

    return sections.join("\n");
  }
}

export function findNewViolations(
  verifications: VerificationResult[],
  filePath: string,
  functionName: string,
  line: number
): { violation: VerificationResult; context: string }[] {
  return verifications
    .filter(
      (v) =>
        v.z3Result === "sat" &&
        v.principle !== null &&
        v.principle.toUpperCase().includes("NEW")
    )
    .map((v) => ({
      violation: v,
      context: `${filePath}:${functionName}:${line}`,
    }));
}

/**
 * Step 1: Semantic diff — check if a proposed principle is already covered
 * by an existing seed axiom (P1-P7) or discovered principle.
 * Uses a separate LLM call with strict 80% overlap threshold.
 */
async function semanticDiffCheck(
  proposed: { name: string; description: string },
  existingPrinciples: Principle[],
  model: string
): Promise<{ covered: boolean; coveredBy?: string }> {
  const allAxioms = [
    ...SEED_AXIOMS,
    ...existingPrinciples.map((p) => ({
      id: p.id,
      name: p.name,
      description: p.description,
    })),
  ];

  const axiomList = allAxioms
    .map((a) => `${a.id}. ${a.name}: ${a.description}`)
    .join("\n\n");

  const prompt = `You are a strict deduplication checker for formal verification principles.

## Proposed New Principle
Name: ${proposed.name}
Description: ${proposed.description}

## Existing Principles
${axiomList}

## Your Task
Is the proposed principle already covered by any of the existing principles above?

Be STRICT: if any existing principle covers even 80% of the pattern described by the proposed principle, it is NOT new.

Consider:
- Does an existing principle already address the same class of bugs?
- Would someone applying the existing principle already catch what this new one describes?
- Is the proposed principle just a specific instance of a more general existing principle?

Respond with ONLY one of:
- "EXISTING: P<N> — <brief explanation>" if covered
- "GENUINELY_NEW" if truly not covered by any existing principle`;

  let rawResponse = "";
  for await (const message of query({
    prompt,
    options: {
      maxTurns: 1,
      model,
      systemPrompt:
        "You deduplicate verification principles. Be strict — most proposed principles are already covered. Respond with either 'EXISTING: P<N>' or 'GENUINELY_NEW'. Nothing else.",
    },
  })) {
    if (message.type === "result" && message.subtype === "success") {
      rawResponse = message.result;
    }
  }

  if (rawResponse.includes("GENUINELY_NEW")) {
    return { covered: false };
  }

  const match = rawResponse.match(/EXISTING:\s*(P\d+)/);
  return {
    covered: true,
    coveredBy: match ? match[1] : "unknown existing principle",
  };
}

/**
 * Step 2: Adversarial model test — use a different model to try to find
 * false positives and false negatives for the proposed principle.
 * The principle only passes if the adversary fails to find counterexamples
 * in 5 attempts.
 */
async function adversarialModelTest(
  proposed: { name: string; description: string; teachingExample: { domain: string; explanation: string; smt2: string } },
  derivationModel: string
): Promise<{ passed: boolean; failure?: string }> {
  const adversaryModel = getAdversaryModel(derivationModel);

  const prompt = `You are an adversarial tester for formal verification principles. Your goal is to BREAK the proposed principle by finding counterexamples.

## Proposed Principle
Name: ${proposed.name}
Description: ${proposed.description}

Teaching Example:
Domain: ${proposed.teachingExample.domain}
Explanation: ${proposed.teachingExample.explanation}
SMT2:
\`\`\`smt2
${proposed.teachingExample.smt2}
\`\`\`

## Your Task
Try to find BOTH of these counterexamples. You have 5 attempts for each.

### False Negative Test
Produce a realistic code snippet (in TypeScript/JavaScript) where this principle SHOULD detect a violation but WOULD NOT. This means code that has exactly the kind of bug the principle describes, but structured in a way that the principle's pattern would miss it.

### False Positive Test
Produce a realistic code snippet where this principle WOULD flag a violation but SHOULD NOT. This means code that superficially matches the principle's pattern but is actually correct.

## Response Format
Respond with ONLY a JSON object:
\`\`\`json
{
  "falseNegative": {
    "found": true/false,
    "snippet": "code here or null",
    "explanation": "why this is a false negative, or why you couldn't find one"
  },
  "falsePositive": {
    "found": true/false,
    "snippet": "code here or null",
    "explanation": "why this is a false positive, or why you couldn't find one"
  }
}
\`\`\`

Be aggressive. Try hard to break the principle. If the principle is vague, overly broad, or poorly defined, it should be easy to find counterexamples. Only report found:false if you genuinely cannot construct a counterexample after careful thought.`;

  const failures: string[] = [];
  const MAX_ATTEMPTS = 5;

  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    let rawResponse = "";
    for await (const message of query({
      prompt: attempt === 0
        ? prompt
        : `${prompt}\n\nThis is attempt ${attempt + 1} of ${MAX_ATTEMPTS}. Previous attempts did not find counterexamples. Try harder — consider edge cases, adversarial inputs, concurrency, type coercion, and unusual but valid code patterns.`,
      options: {
        maxTurns: 1,
        model: adversaryModel,
        systemPrompt:
          "You are an adversarial red-teamer for verification principles. Your ONLY goal is to find counterexamples that break the principle. Be creative and thorough. Respond with JSON only.",
      },
    })) {
      if (message.type === "result" && message.subtype === "success") {
        rawResponse = message.result;
      }
    }

    // Parse the adversary's response
    const jsonMatch = rawResponse.match(/```json\s*([\s\S]*?)```/);
    const jsonStr = jsonMatch ? jsonMatch[1]! : rawResponse;

    try {
      const parsed = JSON.parse(jsonStr.trim());

      if (parsed.falseNegative?.found === true) {
        failures.push(
          `False negative (attempt ${attempt + 1}): ${parsed.falseNegative.explanation}`
        );
      }
      if (parsed.falsePositive?.found === true) {
        failures.push(
          `False positive (attempt ${attempt + 1}): ${parsed.falsePositive.explanation}`
        );
      }

      if (failures.length > 0) {
        return {
          passed: false,
          failure: failures.join("; "),
        };
      }
    } catch {
      // If the adversary's response isn't parseable, treat this attempt as
      // inconclusive and continue to next attempt
      continue;
    }
  }

  // Adversary couldn't find counterexamples in any attempt
  return { passed: true };
}

/**
 * Build a detailed description of all principles (seed + discovered) for
 * use in classifier prompts. Includes full descriptions and teaching examples
 * so the LLM can make proper semantic comparisons.
 */
function formatAllPrinciplesDetailed(existingPrinciples: Principle[]): string {
  const sections: string[] = [];

  for (const axiom of SEED_AXIOMS) {
    sections.push(`### ${axiom.id}. ${axiom.name}`);
    sections.push(axiom.description);
    sections.push(`**Teaching example:** ${axiom.teachingExample}`);
    sections.push("");
  }

  for (const p of existingPrinciples) {
    sections.push(`### ${p.id}. ${p.name}`);
    sections.push(p.description);
    sections.push(`**Teaching example (${p.teachingExample.domain}):** ${p.teachingExample.explanation}`);
    sections.push("");
  }

  return sections.join("\n");
}

/**
 * Stage 2 of two-stage classification: given a proposed NEW principle,
 * re-examine with reversed framing — asking whether any existing principle
 * could have caught this violation if applied correctly.
 *
 * This catches cases where Stage 1 said NEW because it wasn't thinking
 * carefully enough about the existing principles' scope.
 */
async function reverseFramingCheck(
  proposedName: string,
  proposedDescription: string,
  violation: VerificationResult,
  context: string,
  existingPrinciples: Principle[],
  model: string
): Promise<{ isNew: boolean; matchedPrinciple?: string }> {
  const detailedPrinciples = formatAllPrinciplesDetailed(existingPrinciples);

  const prompt = `You are a second-opinion reviewer for principle classification. A first-pass classifier looked at a violation and proposed creating a NEW principle. Your job is to challenge that decision.

## The Proposed New Principle
Name: ${proposedName}
Description: ${proposedDescription}

## The Violation That Triggered It
Found at: ${context}
SMT-LIB block:
\`\`\`smt2
${violation.smt2}
\`\`\`

## All Existing Principles (with full descriptions and teaching examples)
${detailedPrinciples}

## Your Task — Challenge the "NEW" Classification

For EACH existing principle above, ask yourself:
1. Could this existing principle, if applied thoroughly, have caught the violation described above?
2. Is the proposed new principle just a specific instance or sub-case of this existing principle?
3. Does the proposed principle's description overlap significantly with this existing principle's scope?

If ANY existing principle could reasonably cover this violation, respond with:
"REDUNDANT: P<number> — <explanation of how the existing principle covers this>"

Only if you are CERTAIN that no existing principle covers this pattern, respond with:
"CONFIRMED_NEW"

Err on the side of REDUNDANT. The cost of a false NEW is higher than the cost of mapping to an existing principle.`;

  let rawResponse = "";
  for await (const message of query({
    prompt,
    options: {
      maxTurns: 1,
      model,
      systemPrompt:
        "You are a strict second-opinion reviewer. Your bias is toward finding an existing principle that covers the violation. Respond with either 'REDUNDANT: P<N>' or 'CONFIRMED_NEW'. Nothing else.",
    },
  })) {
    if (message.type === "result" && message.subtype === "success") {
      rawResponse = message.result;
    }
  }

  if (rawResponse.includes("CONFIRMED_NEW")) {
    return { isNew: true };
  }

  const match = rawResponse.match(/REDUNDANT:\s*(P\d+)/);
  return {
    isNew: false,
    matchedPrinciple: match ? match[1] : "unknown existing principle",
  };
}

export async function classifyAndGeneralize(
  violation: VerificationResult,
  context: string,
  existingPrinciples: Principle[],
  model: string
): Promise<Principle | null> {
  // Build detailed principle list with full descriptions and teaching examples
  const detailedPrinciples = formatAllPrinciplesDetailed(existingPrinciples);

  // --- Stage 1: Classification with full principle descriptions ---
  const stage1Prompt = `You are a verification principle analyst. A formal verification engine found a violation tagged [NEW] — meaning the derivation engine thought it doesn't fit any existing principle. Your job is to determine if this is genuinely new, and if so, generalize it.

## Existing Principles (with full descriptions and teaching examples)
${detailedPrinciples}

## The [NEW] Violation
Found at: ${context}
SMT-LIB block:
\`\`\`smt2
${violation.smt2}
\`\`\`

## Your Task

Read each existing principle's FULL description and teaching example carefully. Many violations that appear new at first glance are actually covered by an existing principle when you consider the principle's full scope.

1. Does this violation fit one of the existing principles listed above? Consider whether any principle, applied broadly, covers this pattern. If yes, respond with ONLY: "EXISTING: P<number>" and a brief explanation.

2. If genuinely new — meaning NO existing principle's description covers this class of bug even when interpreted broadly — respond with a JSON object (and NOTHING else before or after it):
\`\`\`json
{
  "name": "Short Principle Name",
  "description": "One paragraph description in formal verification textbook language.",
  "teachingExample": {
    "domain": "A domain COMPLETELY DIFFERENT from the original code (e.g., aviation, healthcare, networking, physics)",
    "explanation": "One sentence explaining the teaching example.",
    "smt2": "A self-contained SMT-LIB 2 block demonstrating the pattern. Must include (check-sat)."
  }
}
\`\`\`

Be rigorous. Most violations fit existing principles. Only propose a new one if you genuinely cannot capture the pattern with the existing set.`;

  let stage1Response = "";

  for await (const message of query({
    prompt: stage1Prompt,
    options: {
      maxTurns: 1,
      model,
      systemPrompt:
        "You classify verification violations. Be concise. Respond with either 'EXISTING: P<N>' or a JSON object. Nothing else.",
    },
  })) {
    if (message.type === "result" && message.subtype === "success") {
      stage1Response = message.result;
    }
  }

  // If Stage 1 says EXISTING, accept immediately
  if (stage1Response.includes("EXISTING:")) {
    return null;
  }

  // Stage 1 said NEW — extract the proposed principle
  const jsonMatch = stage1Response.match(/```json\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1]! : stage1Response;

  let parsed: any;
  try {
    parsed = JSON.parse(jsonStr.trim());
  } catch {
    return null;
  }

  // --- Stage 2: Reverse framing check ---
  // Re-examine with a different prompt that biases toward finding existing matches
  const stage2Result = await reverseFramingCheck(
    parsed.name,
    parsed.description,
    violation,
    context,
    existingPrinciples,
    model
  );

  // If Stage 2 says it's redundant, reject the new principle
  if (!stage2Result.isNew) {
    return null;
  }

  // Both stages agree it's NEW — proceed through existing validation pipeline

  // Semantic diff check (existing Step 1 — additional safety net)
  const diffResult = await semanticDiffCheck(
    { name: parsed.name, description: parsed.description },
    existingPrinciples,
    model
  );

  if (diffResult.covered) {
    return null;
  }

  // Adversarial model test (existing Step 2)
  const adversarialResult = await adversarialModelTest(
    {
      name: parsed.name,
      description: parsed.description,
      teachingExample: parsed.teachingExample,
    },
    model
  );

  const principle: Principle = {
    id: "", // filled by caller
    name: parsed.name,
    description: parsed.description,
    teachingExample: parsed.teachingExample,
    provenance: {
      discoveredIn: context,
      violation: violation.smt2.slice(0, 200),
      generalizedAt: new Date().toISOString(),
    },
    validated: adversarialResult.passed,
  };

  if (!adversarialResult.passed) {
    principle.validationFailure = adversarialResult.failure;
  }

  return principle;
}
