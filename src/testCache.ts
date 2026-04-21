import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";
import type { TestOutcome } from "./testAdapters/Adapter";

/**
 * Cache for test-adapter outcomes. Keyed by (test file mtime-ns,
 * test name, source file hash) so that unchanged code + unchanged
 * tests skips re-invocation. Source file hash is included because
 * the outcome depends on the implementation the test is exercising.
 */

export class TestCache {
  private dir: string;

  constructor(projectRoot: string) {
    this.dir = join(projectRoot, ".neurallog", "test-cache");
  }

  private key(testFile: string, testName: string, sourceFile: string): string {
    const h = createHash("sha256");
    let testMtime = "0";
    try { testMtime = String(statSync(testFile).mtimeMs); } catch {}
    let sourceMtime = "0";
    try { sourceMtime = String(statSync(sourceFile).mtimeMs); } catch {}
    h.update(testFile);
    h.update("\n");
    h.update(testMtime);
    h.update("\n");
    h.update(testName);
    h.update("\n");
    h.update(sourceFile);
    h.update("\n");
    h.update(sourceMtime);
    return h.digest("hex").slice(0, 16);
  }

  get(testFile: string, testName: string, sourceFile: string): TestOutcome | null {
    const k = this.key(testFile, testName, sourceFile);
    const path = join(this.dir, `${k}.json`);
    if (!existsSync(path)) return null;
    try {
      return JSON.parse(readFileSync(path, "utf-8"));
    } catch {
      return null;
    }
  }

  put(testFile: string, testName: string, sourceFile: string, outcome: TestOutcome): void {
    const k = this.key(testFile, testName, sourceFile);
    mkdirSync(this.dir, { recursive: true });
    const path = join(this.dir, `${k}.json`);
    try {
      writeFileSync(path, JSON.stringify(outcome, null, 2));
    } catch (e: any) {
      console.log(`[test-cache] put failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }
}
