import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";

export interface JudgeCacheEntry {
  valid: boolean;
  note: string;
  cachedAt: string;
}

export class JudgeCache {
  private cacheDir: string;

  constructor(projectRoot: string) {
    this.cacheDir = join(projectRoot, ".provekit", "judge-cache");
  }

  private key(...parts: string[]): string {
    const h = createHash("sha256");
    for (const p of parts) {
      h.update(p);
      h.update("\n---\n");
    }
    return h.digest("hex").slice(0, 16);
  }

  get(...parts: string[]): JudgeCacheEntry | null {
    const k = this.key(...parts);
    const path = join(this.cacheDir, `${k}.json`);
    if (!existsSync(path)) return null;
    try {
      return JSON.parse(readFileSync(path, "utf-8"));
    } catch {
      return null;
    }
  }

  put(entry: { valid: boolean; note: string }, ...parts: string[]): void {
    const k = this.key(...parts);
    mkdirSync(this.cacheDir, { recursive: true });
    const path = join(this.cacheDir, `${k}.json`);
    try {
      writeFileSync(
        path,
        JSON.stringify({ ...entry, cachedAt: new Date().toISOString() }, null, 2)
      );
    } catch (e: any) {
      console.log(`[judge-cache] put failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }
}
