import { LLMProvider, createProvider } from "./llm";
import { VerificationResult } from "./verifier";
import { judgeTeachingExample } from "./judge";
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";

export interface ASTPattern {
  nodeType: string;
  operator?: string;
  method?: string;
  requiresParamRef?: boolean;
  guardPatterns?: string[];
  pairMethod?: string;
  checkPaths?: string[];
}

export interface PrincipleStats {
  uses: number;
  proven: number;
  violations: number;
  errors: number;
  lastUsedAt?: string;
  retiredAt?: string;
  retiredReason?: string;
}

export interface Principle {
  id: string;
  name: string;
  description: string;
  astPatterns?: ASTPattern[];
  smt2Template?: string;
  smt2ProofTemplate?: string;
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
  confidence?: "high" | "low";
  validated: boolean;
  validationFailure?: string;
  stats?: PrincipleStats;
}


export function hashPrinciple(id: string): string {
  return id;
}

function getAdversaryModel(model: string): string {
  return model;
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
      } catch (e: any) { console.log(`[principles] Failed to load ${entry}: ${e?.message?.slice(0, 40)}`); }
    }
  }

  getAll(): Principle[] {
    return [...this.principles];
  }

  hashForPrinciple(id: string): string {
    const seedHash = hashPrinciple(id);
    if (seedHash) return seedHash;

    const discovered = this.principles.find((p) => p.id === id);
    if (discovered) {
      const hash = createHash("sha256");
      hash.update(PrincipleStore.ENGINE_VERSION);
      hash.update(discovered.id);
      hash.update(discovered.name);
      hash.update(discovered.description);
      hash.update(discovered.teachingExample.smt2);
      if (discovered.smt2Template) hash.update(discovered.smt2Template);
      if (discovered.smt2ProofTemplate) hash.update(discovered.smt2ProofTemplate);
      if (discovered.astPatterns) hash.update(JSON.stringify(discovered.astPatterns));
      return hash.digest("hex");
    }

    return "";
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

  private dirtyStats: Set<string> = new Set();

  recordUse(principleTag: string | null, verdict: "proven" | "violation" | "error"): void {
    if (!principleTag) return;
    const ids = principleTag
      .replace(/\[NEW\]/gi, "")
      .split(/[,+&\s]+/)
      .map((s) => s.trim())
      .filter((s) => /^P\d+$/i.test(s));
    if (ids.length === 0) return;

    const nowIso = new Date().toISOString();
    for (const id of ids) {
      const p = this.principles.find((x) => x.id === id);
      if (!p) continue;
      if (!p.stats) p.stats = { uses: 0, proven: 0, violations: 0, errors: 0 };
      p.stats.uses++;
      p.stats.lastUsedAt = nowIso;
      if (verdict === "proven") p.stats.proven++;
      else if (verdict === "violation") p.stats.violations++;
      else p.stats.errors++;
      this.dirtyStats.add(id);
    }
  }

  persistStats(): void {
    if (this.dirtyStats.size === 0) return;
    mkdirSync(this.principlesDir, { recursive: true });
    for (const id of this.dirtyStats) {
      const p = this.principles.find((x) => x.id === id);
      if (!p) continue;
      const filename = `${id}.json`;
      try {
        writeFileSync(
          join(this.principlesDir, filename),
          JSON.stringify(p, null, 2)
        );
      } catch (e: any) {
        console.log(`[principles] persistStats ${id}: ${e?.message?.slice(0, 40) || "ok"}`);
      }
    }
    this.dirtyStats.clear();
  }

  private shouldRetire(p: Principle): { retire: boolean; reason?: string } {
    const s = p.stats;
    if (!s) return { retire: false };
    if (s.retiredAt) return { retire: false };
    if (s.uses < 20) return { retire: false };
    const errorRate = s.errors / s.uses;
    if (errorRate > 0.5 && s.proven === 0) {
      return { retire: true, reason: `error-rate ${(errorRate * 100).toFixed(0)}% over ${s.uses} uses, 0 proofs` };
    }
    return { retire: false };
  }

  evaluateRetirements(): { id: string; reason: string }[] {
    const retired: { id: string; reason: string }[] = [];
    for (const p of this.principles) {
      const v = this.shouldRetire(p);
      if (v.retire && v.reason) {
        retired.push({ id: p.id, reason: v.reason });
        this.retire(p.id, v.reason);
      }
    }
    return retired;
  }

  retire(id: string, reason: string): void {
    const p = this.principles.find((x) => x.id === id);
    if (!p) return;
    if (!p.stats) p.stats = { uses: 0, proven: 0, violations: 0, errors: 0 };
    p.stats.retiredAt = new Date().toISOString();
    p.stats.retiredReason = reason;

    const retiredDir = join(this.principlesDir, "retired");
    mkdirSync(retiredDir, { recursive: true });
    const filename = `${id}.json`;
    try {
      writeFileSync(join(retiredDir, filename), JSON.stringify(p, null, 2));
      try {
        require("fs").unlinkSync(join(this.principlesDir, filename));
      } catch {}
    } catch (e: any) {
      console.log(`[principles] retire ${id}: ${e?.message?.slice(0, 40) || "ok"}`);
    }

    this.principles = this.principles.filter((x) => x.id !== id);
  }

  /**
   * Compute a hash of all principle files on disk. Used as part of the
   * contract cache key so contracts are invalidated when principles change.
   */
  static readonly ENGINE_VERSION = "15";

  computePrincipleHash(): string {
    const hash = createHash("sha256");
    hash.update(PrincipleStore.ENGINE_VERSION);

    if (!existsSync(this.principlesDir)) return "";

    const entries = readdirSync(this.principlesDir)
      .filter((e) => e.endsWith(".json"))
      .sort();

    for (const entry of entries) {
      const content = readFileSync(join(this.principlesDir, entry), "utf-8");
      hash.update(entry);
      hash.update(content);
    }

    return hash.digest("hex");
  }

  getPrincipleCount(): number {
    return this.principles.length;
  }

  getNewPrinciplesSince(contractPrincipleHash: string): Principle[] {
    if (!contractPrincipleHash) return this.principles;
    const currentHash = this.computePrincipleHash();
    if (currentHash === contractPrincipleHash) return [];
    return this.principles;
  }

  nextId(): string {
    const existing = this.principles.length + 1;
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
 * by an existing principle (seed or discovered).
 * Uses a separate LLM call with strict 80% overlap threshold.
 */
async function semanticDiffCheck(
  proposed: { name: string; description: string },
  existingPrinciples: Principle[],
  model: string,
  provider: LLMProvider
): Promise<{ covered: boolean; coveredBy?: string }> {
  const axiomList = existingPrinciples
    .map((p) => `${p.id}. ${p.name}: ${p.description}`)
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

  const response = await provider.complete(prompt, {
    model,
    systemPrompt: "You deduplicate verification principles. Be strict — most proposed principles are already covered. Respond with either 'EXISTING: P<N>' or 'GENUINELY_NEW'. Nothing else.",
  });

  if (response.text.includes("GENUINELY_NEW")) {
    return { covered: false };
  }

  const rawResponse = response.text;

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
  derivationModel: string,
  provider: LLMProvider
): Promise<{ passed: boolean; failure?: string }> {
  const adversaryModel = getAdversaryModel(derivationModel);

  const prompt = `You are testing whether a verification principle is useful in practice.

## Proposed Principle
Name: ${proposed.name}
Description: ${proposed.description}

Teaching Example (${proposed.teachingExample.domain}):
${proposed.teachingExample.explanation}
\`\`\`smt2
${proposed.teachingExample.smt2}
\`\`\`

## Your Task

Write TWO short, realistic TypeScript functions (under 20 lines each) that a normal developer would actually write:

1. **False negative**: A function with exactly the bug this principle describes, but written in a way that might slip past it. Keep it realistic — no async edge cases, no metaprogramming, no framework magic. Just a normal function with the bug.

2. **False positive**: A function that looks like it has the bug but is actually correct. The principle would wrongly flag it.

If you genuinely cannot write either one after honest effort, say found: false.

\`\`\`json
{
  "falseNegative": {
    "found": true/false,
    "snippet": "short realistic TypeScript function or null",
    "explanation": "one sentence"
  },
  "falsePositive": {
    "found": true/false,
    "snippet": "short realistic TypeScript function or null",
    "explanation": "one sentence"
  }
}
\`\`\`

Be practical. Only report found:true if a normal developer would actually write that code. No async edge cases, no metaprogramming, no theoretical constructs.`;

  const failures: string[] = [];
  const MAX_ATTEMPTS = 2;

  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    const response = await provider.complete(prompt, {
      model: adversaryModel,
      systemPrompt: "Write realistic TypeScript counterexamples. Short, practical, something a real developer would write. Respond with JSON only.",
    });
    const rawResponse = response.text;

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

  for (const p of existingPrinciples) {
    sections.push(`### ${p.id}. ${p.name}`);
    sections.push(p.description);
    if (p.teachingExample.domain) {
      sections.push(`**Teaching example (${p.teachingExample.domain}):** ${p.teachingExample.explanation}`);
    }
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
  model: string,
  provider: LLMProvider
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

  const response = await provider.complete(prompt, {
    model,
    systemPrompt: "You are a strict second-opinion reviewer. Your bias is toward finding an existing principle that covers the violation. Respond with either 'REDUNDANT: P<N>' or 'CONFIRMED_NEW'. Nothing else.",
  });
  const rawResponse = response.text;

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
  model: string,
  provider?: LLMProvider
): Promise<Principle | null> {
  const llm = provider || createProvider();
  // Build detailed principle list with full descriptions and teaching examples
  const detailedPrinciples = formatAllPrinciplesDetailed(existingPrinciples);

  // The derivation engine tagged this [NEW]. Trust that judgment.
  // Generalize it into a principle, then let the adversary try to break it.
  const generalizePrompt = `A formal verification engine found a novel violation pattern. Generalize it into a reusable verification principle.

## The [NEW] Violation
Found at: ${context}
SMT-LIB block:
\`\`\`smt2
${violation.smt2}
\`\`\`

## Existing Principles (for reference — do NOT map to these)
${detailedPrinciples}

## Your Task

This violation was tagged [NEW] because it doesn't fit the existing principles. Your job is to GENERALIZE it — extract the abstract pattern so it can find similar bugs in other codebases.

Respond with a JSON object:
\`\`\`json
{
  "name": "Short Principle Name (e.g., Resource Lifecycle, State Machine Constraint)",
  "description": "One paragraph description in formal verification textbook language. Describe the GENERAL class of bug, not this specific instance.",
  "teachingExample": {
    "domain": "A domain COMPLETELY DIFFERENT from the original code (e.g., aviation, healthcare, networking, physics)",
    "explanation": "One sentence explaining the teaching example.",
    "smt2": "A self-contained SMT-LIB 2 block demonstrating the pattern. Must include (check-sat)."
  }
}
\`\`\``;

  const generalizeResult = await llm.complete(generalizePrompt, {
    model,
    systemPrompt: "You generalize bug patterns into reusable verification principles. Be precise and abstract. Respond with JSON only.",
  });

  const jsonMatch = generalizeResult.text.match(/```json\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1]! : generalizeResult.text;

  let parsed: any;
  try {
    parsed = JSON.parse(jsonStr.trim());
  } catch {
    return null;
  }

  const MAX_REFINEMENTS = 3;
  let current = parsed;

  for (let round = 0; round < MAX_REFINEMENTS; round++) {
    const adversarialResult = await adversarialModelTest(
      { name: current.name, description: current.description, teachingExample: current.teachingExample },
      model, llm
    );

    if (adversarialResult.passed) {
      const judge = await judgeTeachingExample(
        {
          name: current.name,
          description: current.description,
          explanation: current.teachingExample.explanation,
          smt2: current.teachingExample.smt2,
        },
        llm,
        model
      );
      if (!judge.valid) {
        console.log(`      Judge rejected teaching example: ${judge.note}`);
        return {
          id: "",
          name: current.name,
          description: current.description,
          teachingExample: current.teachingExample,
          provenance: { discoveredIn: context, violation: violation.smt2.slice(0, 200), generalizedAt: new Date().toISOString() },
          validated: false,
          validationFailure: `judge-rejected: ${judge.note}`,
        };
      }
      return {
        id: "",
        name: current.name,
        description: current.description,
        teachingExample: current.teachingExample,
        provenance: { discoveredIn: context, violation: violation.smt2.slice(0, 200), generalizedAt: new Date().toISOString() },
        validated: true,
      };
    }

    if (round === MAX_REFINEMENTS - 1) {
      return {
        id: "",
        name: current.name,
        description: current.description,
        teachingExample: current.teachingExample,
        provenance: { discoveredIn: context, violation: violation.smt2.slice(0, 200), generalizedAt: new Date().toISOString() },
        validated: false,
        validationFailure: adversarialResult.failure,
      };
    }

    console.log(`      Refining principle (round ${round + 1}): ${adversarialResult.failure?.slice(0, 100)}`);

    const refineResult = await llm.complete(`A verification principle was proposed but the adversary found a weakness. Refine the principle to address the weakness.

## Current Principle
Name: ${current.name}
Description: ${current.description}

## Adversary's Criticism
${adversarialResult.failure}

## Your Task
Refine the principle to address the adversary's criticism. Make the description more precise so the weakness no longer applies. Do NOT make it so narrow that it only catches the original bug — keep it general.

Respond with a JSON object:
\`\`\`json
{
  "name": "Refined Principle Name",
  "description": "Refined one-paragraph description addressing the adversary's criticism.",
  "teachingExample": {
    "domain": "A different domain from the original",
    "explanation": "One sentence.",
    "smt2": "Self-contained SMT-LIB 2 block. Must include (check-sat)."
  }
}
\`\`\``, { model, systemPrompt: "You refine verification principles based on adversarial feedback. Make them stronger, not narrower. Respond with JSON only." });

    const refineMatch = refineResult.text.match(/```json\s*([\s\S]*?)```/);
    const refineStr = refineMatch ? refineMatch[1]! : refineResult.text;

    try {
      current = JSON.parse(refineStr.trim());
    } catch {
      break;
    }
  }

  return null;
}
