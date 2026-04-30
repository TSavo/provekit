import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from "fs";
import { join, isAbsolute, resolve as resolvePath } from "path";
import { createHash } from "crypto";
import type { TestOutcome } from "./testAdapters/Adapter";

/**
 * Cache for test-adapter outcomes. Keyed by (test file path + mtimeMs,
 * test name, source file path + mtimeMs) so that unchanged code +
 * unchanged tests skip re-invocation. Source mtime is included because
 * the outcome depends on the implementation the test is exercising;
 * if the source file is touched, the cached outcome is invalidated.
 *
 * Uses mtimeMs (millisecond resolution) rather than content hashing
 * because mtimes are fast to read and practically identical to content
 * hashes for the "has this file changed since cached" question. If
 * content-addressing becomes required later (e.g. to handle git
 * checkouts that preserve content but reset mtime), switch to sha256
 * of file contents here.
 */

export class TestCache {
  private dir: string;
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.dir = join(projectRoot, ".provekit", "test-cache");
  }

  private resolve(p: string): string {
    return isAbsolute(p) ? p : resolvePath(this.projectRoot, p);
  }

  private key(framework: string, testFile: string, testName: string, sourceFile: string): string {
    const h = createHash("sha256");
    const testAbs = this.resolve(testFile);
    const sourceAbs = this.resolve(sourceFile);
    let testMtime = "0";
    try { testMtime = String(statSync(testAbs).mtimeMs); } catch {}
    let sourceMtime = "0";
    try { sourceMtime = String(statSync(sourceAbs).mtimeMs); } catch {}
    h.update(framework);
    h.update("\n");
    h.update(testAbs);
    h.update("\n");
    h.update(testMtime);
    h.update("\n");
    h.update(testName);
    h.update("\n");
    h.update(sourceAbs);
    h.update("\n");
    h.update(sourceMtime);
    return h.digest("hex").slice(0, 16);
  }

  get(framework: string, testFile: string, testName: string, sourceFile: string): TestOutcome | null {
    const k = this.key(framework, testFile, testName, sourceFile);
    const path = join(this.dir, `${k}.json`);
    if (!existsSync(path)) return null;
    try {
      return JSON.parse(readFileSync(path, "utf-8"));
    } catch {
      return null;
    }
  }

  put(framework: string, testFile: string, testName: string, sourceFile: string, outcome: TestOutcome): void {
    const k = this.key(framework, testFile, testName, sourceFile);
    mkdirSync(this.dir, { recursive: true });
    const path = join(this.dir, `${k}.json`);
    try {
      writeFileSync(path, JSON.stringify(outcome, null, 2));
    } catch (e: any) {
      console.log(`[test-cache] put failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }
}
