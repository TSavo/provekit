import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";

export interface Lesson {
  judgeNote: string;
  contractKey: string;
  principleId: string | null;
  claim: string;
  addedAt: string;
}

export class LessonStore {
  private dir: string;

  constructor(projectRoot: string) {
    this.dir = join(projectRoot, ".neurallog", "lessons");
  }

  add(lesson: Omit<Lesson, "addedAt">): void {
    mkdirSync(this.dir, { recursive: true });
    const keySource = `${lesson.contractKey}|${lesson.judgeNote.slice(0, 200)}`;
    const key = createHash("sha256").update(keySource).digest("hex").slice(0, 16);
    const path = join(this.dir, `${key}.json`);
    if (existsSync(path)) return;
    try {
      writeFileSync(
        path,
        JSON.stringify({ ...lesson, addedAt: new Date().toISOString() }, null, 2)
      );
    } catch (e: any) {
      console.log(`[lessons] add failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }

  getAll(): Lesson[] {
    if (!existsSync(this.dir)) return [];
    const results: Lesson[] = [];
    for (const f of readdirSync(this.dir)) {
      if (!f.endsWith(".json")) continue;
      try {
        results.push(JSON.parse(readFileSync(join(this.dir, f), "utf-8")));
      } catch {}
    }
    results.sort((a, b) => (b.addedAt > a.addedAt ? 1 : -1));
    return results;
  }

  formatForPrompt(limit: number = 10): string {
    const lessons = this.getAll().slice(0, limit);
    if (lessons.length === 0) return "";

    const lines: string[] = [
      "",
      "## Known Encoding Gaps — Avoid These Mistakes",
      "",
      "The property-test harness has caught the following encoder failures in past runs. Each entry is a case where an SMT-LIB encoding was proven by Z3 but contradicted by runtime behaviour. When you encode new principles or teaching examples, specifically check that your encoding does not repeat these mistakes.",
      "",
    ];
    for (const l of lessons) {
      lines.push(`### Gap on \`${l.contractKey}\`${l.principleId ? ` (principle: ${l.principleId})` : ""}`);
      lines.push(`- Claim: ${l.claim.slice(0, 120)}`);
      lines.push(`- Judge note: ${l.judgeNote.slice(0, 300)}`);
      lines.push("");
    }
    return lines.join("\n");
  }
}
