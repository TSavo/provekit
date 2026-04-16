import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, unlinkSync } from "fs";
import { join } from "path";
import { LLMProvider } from "./llm";
import { Principle, PrincipleStore, ASTPattern } from "./principles";

export interface ASTContext {
  nodeType: string;
  operator?: string;
  method?: string;
  referencesParam: boolean;
  pathConditions: string[];
  parentType?: string;
  insideTryCatch: boolean;
}

export interface Observation {
  id: string;
  signalKey: string;
  claim: string;
  smt2: string;
  astContext?: ASTContext;
  rejectedPrincipleName: string;
  rejectedPrincipleDescription: string;
  adversaryFeedback: string;
  observedAt: string;
}

export class ObservationStore {
  private observations: Observation[] = [];
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.loadFromDisk();
  }

  private get observationsDir(): string {
    return join(this.projectRoot, ".neurallog", "observations");
  }

  private loadFromDisk(): void {
    if (!existsSync(this.observationsDir)) return;

    for (const entry of readdirSync(this.observationsDir)) {
      if (!entry.endsWith(".json")) continue;
      try {
        const data: Observation = JSON.parse(
          readFileSync(join(this.observationsDir, entry), "utf-8")
        );
        this.observations.push(data);
      } catch (e: any) { console.log(`[observations] Failed to load ${entry}: ${e?.message?.slice(0, 40)}`); }
    }
  }

  add(observation: Observation): void {
    this.observations.push(observation);
    mkdirSync(this.observationsDir, { recursive: true });
    writeFileSync(
      join(this.observationsDir, `${observation.id}.json`),
      JSON.stringify(observation, null, 2)
    );
    console.log(`[observations] Saved: ${observation.id} — ${observation.claim.slice(0, 60)}`);
  }

  getAll(): Observation[] {
    return [...this.observations];
  }

  nextId(): string {
    return `obs-${this.observations.length + 1}`;
  }

  findSimilarByAST(obs: Observation): Observation[] {
    if (!obs.astContext) return [];
    return this.observations.filter((o) => {
      if (!o.astContext || o.id === obs.id) return false;
      return this.astContextSimilarity(obs.astContext!, o.astContext!) >= 0.7;
    });
  }

  private astContextSimilarity(a: ASTContext, b: ASTContext): number {
    let matches = 0;
    let total = 0;

    total++; if (a.nodeType === b.nodeType) matches++;
    total++; if (a.operator === b.operator) matches++;
    total++; if (a.method === b.method) matches++;
    total++; if (a.referencesParam === b.referencesParam) matches++;
    total++; if (a.insideTryCatch === b.insideTryCatch) matches++;

    return matches / total;
  }

  findClusters(minSize: number = 3): Observation[][] {
    const used = new Set<string>();
    const clusters: Observation[][] = [];

    for (const obs of this.observations) {
      if (used.has(obs.id)) continue;
      if (!obs.astContext) continue;

      const similar = this.findSimilarByAST(obs).filter((o) => !used.has(o.id));
      const cluster = [obs, ...similar];

      if (cluster.length >= minSize) {
        clusters.push(cluster);
        for (const o of cluster) used.add(o.id);
      }
    }

    for (const obs of this.observations) {
      if (used.has(obs.id) || !obs.astContext) continue;

      const key = `${obs.astContext.nodeType}:${obs.astContext.operator || ""}:${obs.astContext.method || ""}`;
      const group = this.observations.filter((o) =>
        !used.has(o.id) && o.astContext &&
        `${o.astContext.nodeType}:${o.astContext.operator || ""}:${o.astContext.method || ""}` === key
      );
      if (group.length >= minSize) {
        clusters.push(group);
        for (const o of group) used.add(o.id);
      }
    }

    return clusters;
  }

  extractCommonASTPattern(cluster: Observation[]): ASTPattern | null {
    const withContext = cluster.filter((o) => o.astContext);
    if (withContext.length < 2) return null;

    const first = withContext[0]!.astContext!;
    const allSameNodeType = withContext.every((o) => o.astContext!.nodeType === first.nodeType);
    if (!allSameNodeType) return null;

    const pattern: ASTPattern = { nodeType: first.nodeType };

    const allSameOp = withContext.every((o) => o.astContext!.operator === first.operator);
    if (allSameOp && first.operator) pattern.operator = first.operator;

    const allSameMethod = withContext.every((o) => o.astContext!.method === first.method);
    if (allSameMethod && first.method) pattern.method = first.method;

    const allRefParam = withContext.every((o) => o.astContext!.referencesParam);
    if (allRefParam) pattern.requiresParamRef = true;

    return pattern;
  }

  async tryCollapse(
    cluster: Observation[],
    principleStore: PrincipleStore,
    provider: LLMProvider,
    model: string
  ): Promise<Principle | null> {
    if (cluster.length < 3) return null;

    const commonPattern = this.extractCommonASTPattern(cluster);

    const examples = cluster.map((o, i) => {
      const parts = [
        `### Observation ${i + 1}: ${o.signalKey}`,
        `**Bug:** ${o.claim}`,
        `**Z3 proof:**`,
        "```smt2",
        o.smt2,
        "```",
      ];
      if (o.astContext) {
        parts.push(`**AST context:** node=${o.astContext.nodeType}, operator=${o.astContext.operator || "none"}, method=${o.astContext.method || "none"}, referencesParam=${o.astContext.referencesParam}, insideTryCatch=${o.astContext.insideTryCatch}`);
      }
      if (o.rejectedPrincipleName) {
        parts.push(`**Prior generalization attempt:** "${o.rejectedPrincipleName}"`);
        parts.push(`**Why the adversary rejected it:** ${o.adversaryFeedback}`);
      }
      return parts.join("\n");
    }).join("\n\n---\n\n");

    const allPrinciples = principleStore.getAll();
    const exemplars = allPrinciples.filter((p) => p.astPatterns && p.smt2Template).slice(0, 3);
    const exemplarText = exemplars.map((p) => `### ${p.id}: ${p.name}
Description: ${p.description}
AST pattern: ${JSON.stringify(p.astPatterns)}
smt2Template: ${p.smt2Template}
Teaching SMT-LIB: ${p.teachingExample.smt2}`).join("\n\n");

    const existingPrinciples = allPrinciples.map((p) =>
      `- ${p.id}: ${p.name} — ${p.description.slice(0, 100)}`
    ).join("\n");

    const patternHint = commonPattern
      ? `\n## Common AST pattern detected\n\nAll ${cluster.length} observations share this AST structure:\n- Node type: \`${commonPattern.nodeType}\`\n${commonPattern.operator ? `- Operator: \`${commonPattern.operator}\`\n` : ""}${commonPattern.method ? `- Method: \`${commonPattern.method}\`\n` : ""}${commonPattern.requiresParamRef ? "- References a function parameter\n" : ""}\nYour principle should match this exact AST pattern. The smt2Template you produce will be instantiated mechanically for every matching AST node — no LLM needed at runtime.\n`
      : "";

    const result = await provider.complete(`You are looking at ${cluster.length} concrete bugs found in real code by a formal verification engine. Each bug was independently confirmed by Z3 (sat = reachable). Each was tagged [NEW] because it didn't fit any existing principle.

Your job: find the COMMON PATTERN and produce an ATOMIC principle — one AST shape, one Z3 template, one bug pattern.

## The observations (with Z3 proofs, AST contexts, and adversary feedback)

${examples}
${patternHint}
## Existing principles (your new principle must be DIFFERENT from all of these)
${existingPrinciples || "(none yet)"}

## Examples of existing atomic principles (showing the FULL structure you must produce)

${exemplarText || "(no principles with AST patterns yet — you are creating the first one)"}

## What makes a good atomic principle

An atomic principle has exactly ONE bug pattern. It is:
- **One AST shape:** "binary_expression with operator /" or "call_expression with method split" — not "arithmetic bugs" or "boundary issues"
- **One Z3 template:** SMT-LIB with {{variable}} holes that get filled from the AST. The template must be self-contained — Z3 can run it after hole-filling with no extra context.
- **One claim:** a single sentence describing what goes wrong

Here is an example of a good atomic principle:

\`\`\`json
{
  "name": "Division by Zero",
  "description": "Division where the denominator derives from a parameter and no guard ensures it is non-zero.",
  "smt2Template": "(declare-const {{numerator}} Int)\\n(declare-const {{denominator}} Int)\\n(assert (= {{denominator}} 0))\\n(check-sat)",
  "teachingExample": {
    "domain": "analytics",
    "explanation": "compute_average divides total by count with no zero guard",
    "smt2": "(declare-const total Int)\\n(declare-const count Int)\\n(assert (= count 0))\\n(check-sat)"
  }
}
\`\`\`

Notice: the smt2Template uses {{variable}} holes. At runtime, the engine replaces {{numerator}} with the actual variable name from the code's AST. The template is instantiated mechanically — no LLM involved.

## What the adversary will try

Two short realistic TypeScript functions:
1. A function with the bug that your principle would MISS (false negative)
2. A function without the bug that your principle would FLAG (false positive)

## Respond with JSON

\`\`\`json
{
  "name": "Short precise name (2-4 words, like a variable name)",
  "description": "One paragraph. State the exact AST pattern, the violation condition, and when it's NOT a violation.",
  "smt2Template": "SMT-LIB 2 with {{hole}} variables. Must include (check-sat). Must be self-contained after hole-filling.",
  "teachingExample": {
    "domain": "A domain DIFFERENT from the observations",
    "explanation": "One sentence.",
    "smt2": "Concrete SMT-LIB 2 block (no holes). Must include (check-sat)."
  }
}
\`\`\``, {
      model,
      systemPrompt: "You produce atomic verification principles — one AST pattern, one Z3 template, one bug. The template will be applied mechanically by a generic engine. Respond with JSON only.",
    });

    const jsonMatch = result.text.match(/```json\s*([\s\S]*?)```/);
    if (!jsonMatch) return null;

    try {
      const parsed = JSON.parse(jsonMatch[1]!.trim());
      return {
        id: "",
        name: parsed.name,
        description: parsed.description,
        astPatterns: commonPattern ? [commonPattern] : undefined,
        smt2Template: parsed.smt2Template || undefined,
        teachingExample: parsed.teachingExample,
        provenance: {
          discoveredIn: cluster.map((o) => o.signalKey).join(", "),
          violation: `Collapsed from ${cluster.length} observations`,
          generalizedAt: new Date().toISOString(),
        },
        validated: false,
      };
    } catch {
      return null;
    }
  }

  promoteObservations(cluster: Observation[], principleId: string): void {
    for (const obs of cluster) {
      const filePath = join(this.observationsDir, `${obs.id}.json`);
      try { unlinkSync(filePath); } catch {}
      this.observations = this.observations.filter((o) => o.id !== obs.id);
    }
    console.log(`[observations] ${cluster.length} observations promoted to ${principleId}`);
  }

  formatForPrompt(): string {
    if (this.observations.length === 0) return "";

    const sections = this.observations.slice(-10).map((o) =>
      `- ${o.signalKey}: ${o.claim} [rejected principle: ${o.rejectedPrincipleName}]`
    );

    return `\n#### Recent observations (bugs found but not yet generalized into principles)\n${sections.join("\n")}`;
  }
}
