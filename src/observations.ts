import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, unlinkSync } from "fs";
import { join } from "path";
import { LLMProvider } from "./llm";
import { Principle, PrincipleStore } from "./principles";

export interface Observation {
  id: string;
  signalKey: string;
  claim: string;
  smt2: string;
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
      } catch {}
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

  findSimilar(claim: string): Observation[] {
    const words = claim.toLowerCase().split(/\s+/).filter((w) => w.length > 4);
    return this.observations.filter((o) => {
      const oClaim = o.claim.toLowerCase();
      const matches = words.filter((w) => oClaim.includes(w));
      return matches.length >= Math.min(3, words.length);
    });
  }

  findClusters(minSize: number = 3): Observation[][] {
    const used = new Set<string>();
    const clusters: Observation[][] = [];

    for (const obs of this.observations) {
      if (used.has(obs.id)) continue;
      const similar = this.findSimilar(obs.claim).filter((o) => !used.has(o.id));
      if (similar.length >= minSize) {
        clusters.push(similar);
        for (const o of similar) used.add(o.id);
      }
    }

    // Also cluster by rejected principle name
    const byPrinciple = new Map<string, Observation[]>();
    for (const obs of this.observations) {
      if (used.has(obs.id) || !obs.rejectedPrincipleName) continue;
      const key = obs.rejectedPrincipleName.toLowerCase().replace(/\s+/g, " ").trim();
      if (!byPrinciple.has(key)) byPrinciple.set(key, []);
      byPrinciple.get(key)!.push(obs);
    }
    for (const [, group] of byPrinciple) {
      if (group.length >= minSize) {
        clusters.push(group);
        for (const o of group) used.add(o.id);
      }
    }

    return clusters;
  }

  async tryCollapse(
    cluster: Observation[],
    principleStore: PrincipleStore,
    provider: LLMProvider,
    model: string
  ): Promise<Principle | null> {
    if (cluster.length < 3) return null;

    const examples = cluster.map((o, i) => {
      const parts = [
        `### Observation ${i + 1}: ${o.signalKey}`,
        `**Bug:** ${o.claim}`,
        `**Z3 proof:**`,
        "```smt2",
        o.smt2,
        "```",
      ];
      if (o.rejectedPrincipleName) {
        parts.push(`**Prior generalization attempt:** "${o.rejectedPrincipleName}"`);
        parts.push(`**Why the adversary rejected it:** ${o.adversaryFeedback}`);
      }
      return parts.join("\n");
    }).join("\n\n---\n\n");

    const existingPrinciples = principleStore.getAll().map((p) =>
      `- ${p.id}: ${p.name} — ${p.description.slice(0, 100)}`
    ).join("\n");

    const result = await provider.complete(`You are looking at ${cluster.length} concrete bugs found in real code by a formal verification engine. Each bug was independently confirmed by Z3 (sat = reachable). Each was tagged [NEW] because it didn't fit any existing principle. Each had a generalization attempt that an adversary rejected.

Your job: find the COMMON PATTERN across all ${cluster.length} observations. You have what no single-instance generalization had — multiple concrete examples showing the same class of bug in different contexts.

## The observations (with Z3 proofs and adversary feedback)

${examples}

## Existing principles (for reference — your new principle must be DIFFERENT from all of these)
${existingPrinciples || "(no discovered principles yet — only the seed axioms in the prompt template)"}

## What makes a good principle

A good principle is:
- **Abstract enough** to catch the bug in code the adversary writes (not just the original observations)
- **Concrete enough** that a developer reading it knows exactly what to look for
- **Distinct** from existing principles — if the existing principles could catch these bugs, they would have been tagged with them

A good teaching example:
- Uses a **completely different domain** from the observations (aviation, healthcare, finance, networking)
- Shows the bug pattern clearly in under 15 lines of SMT-LIB
- Includes comments explaining the code model and the violation

## What the adversary will try

The adversary will write two short realistic TypeScript functions:
1. A function with the bug that your principle would MISS (false negative)
2. A function without the bug that your principle would FLAG (false positive)

Your principle must be precise enough to avoid both.

## Learn from the rejections

The adversary already told you what was wrong with prior attempts. Read the "Why rejected" sections carefully. Your new principle must address those specific weaknesses.

Respond with JSON:
\`\`\`json
{
  "name": "Short, precise name (3-5 words)",
  "description": "One paragraph in formal verification language. State the invariant, the violation condition, and the detection criteria. Be specific about what constitutes a violation vs. correct code.",
  "teachingExample": {
    "domain": "A domain COMPLETELY DIFFERENT from all observations above",
    "explanation": "One sentence: what the code does and where the bug is.",
    "smt2": "Complete SMT-LIB 2 block modeling the bug pattern. Include (check-sat). Include comments explaining the code model."
  }
}
\`\`\``, {
      model,
      systemPrompt: "You discover verification principles from clusters of concrete bug observations. You have multiple examples AND the adversary's prior criticisms. Use both. Respond with JSON only.",
    });

    const jsonMatch = result.text.match(/```json\s*([\s\S]*?)```/);
    if (!jsonMatch) return null;

    try {
      const parsed = JSON.parse(jsonMatch[1]!.trim());
      return {
        id: "",
        name: parsed.name,
        description: parsed.description,
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
