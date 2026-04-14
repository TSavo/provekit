import { query } from "@anthropic-ai/claude-agent-sdk";
import { VerificationResult } from "./verifier";
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join } from "path";

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

export async function classifyAndGeneralize(
  violation: VerificationResult,
  context: string,
  existingPrinciples: Principle[],
  model: string
): Promise<Principle | null> {
  const existingList = [
    "P1. Precondition Propagation",
    "P2. State Mutation Analysis",
    "P3. Calling Context Analysis",
    "P4. Temporal Analysis",
    "P5. Semantic Correctness",
    "P6. Boundary and Degenerate Inputs",
    "P7. Arithmetic Safety",
    ...existingPrinciples.map((p) => `${p.id}. ${p.name}`),
  ].join("\n");

  const prompt = `You are a verification principle analyst. A formal verification engine found a violation tagged [NEW] — meaning it doesn't fit any existing principle. Your job is to determine if this is genuinely new, and if so, generalize it.

## Existing Principles
${existingList}

## The [NEW] Violation
Found at: ${context}
SMT-LIB block:
\`\`\`smt2
${violation.smt2}
\`\`\`

## Your Task

1. Does this violation actually fit one of the existing principles listed above? If yes, respond with ONLY: "EXISTING: P<number>" and explain why.

2. If genuinely new, respond with a JSON object (and NOTHING else before or after it):
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

  let rawResponse = "";

  for await (const message of query({
    prompt,
    options: {
      maxTurns: 1,
      model,
      systemPrompt:
        "You classify verification violations. Be concise. Respond with either 'EXISTING: P<N>' or a JSON object. Nothing else.",
    },
  })) {
    if (message.type === "result" && message.subtype === "success") {
      rawResponse = message.result;
    }
  }

  // Check if it maps to an existing principle
  if (rawResponse.includes("EXISTING:")) {
    return null;
  }

  // Try to extract JSON
  const jsonMatch = rawResponse.match(/```json\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1]! : rawResponse;

  try {
    const parsed = JSON.parse(jsonStr.trim());
    return {
      id: "", // filled by caller
      name: parsed.name,
      description: parsed.description,
      teachingExample: parsed.teachingExample,
      provenance: {
        discoveredIn: context,
        violation: violation.smt2.slice(0, 200),
        generalizedAt: new Date().toISOString(),
      },
    };
  } catch {
    return null;
  }
}
