import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join } from "path";

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

  formatForPrompt(): string {
    if (this.observations.length === 0) return "";

    const sections = this.observations.slice(-10).map((o) =>
      `- ${o.signalKey}: ${o.claim} [rejected principle: ${o.rejectedPrincipleName}]`
    );

    return `\n#### Recent observations (bugs found but not yet generalized into principles)\n${sections.join("\n")}`;
  }
}
