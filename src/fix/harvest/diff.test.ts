/**
 * Tests for the harvest-time diff writer + active-context API
 * (hard-bug 1, Day 2/3).
 *
 * Contract:
 *   - recordCandidateDiff persists one row per node in the entries list,
 *     keyed by harvest:<project>:<bugId>
 *   - Idempotent re-run after clearCandidateDiff produces the same row count
 *   - DSL-style lookup by post coordinates locates the added IfStatement /
 *     OR-chain extension reliably
 *   - setActiveDiffContext / clearActiveDiffContext writes to the singleton
 *     diff_context_active table; only one row at a time
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join, dirname } from "path";
import { tmpdir } from "os";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq, and } from "drizzle-orm";

import { openDb, type Db } from "../../db/index.js";
import { prePostDiff, diffContextActive } from "../../db/schema/preDiff.js";
import {
  recordCandidateDiff,
  clearCandidateDiff,
  setActiveDiffContext,
  clearActiveDiffContext,
  setActiveCandidate,
} from "./diff.js";
import type { HarvestCandidate } from "./extractBugs.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "..", "..", "drizzle");

function makeCandidate(): HarvestCandidate {
  return {
    source: {
      project: "synthetic",
      bugId: "1",
      baseSha: "deadbeef",
      fixSha: "f1xc0de",
      testSha: null,
      originalSha: null,
    },
    buggyFiles: {
      "src/divide.ts": `function divide(a: number, b: number): number {
  return a / b;
}`,
      "src/check.ts": `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`,
    },
    fixedFiles: {
      "src/divide.ts": `function divide(a: number, b: number): number {
  if (b === 0) throw new Error("Division by zero");
  return a / b;
}`,
      "src/check.ts": `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`,
    },
    diff: "",
    upstreamFixMessage: "",
    testFiles: {},
    stats: { filesChanged: 2, insertions: 2, deletions: 0 },
  };
}

function withScratchDb<T>(fn: (db: Db) => T): T {
  const scratch = mkdtempSync(join(tmpdir(), "provekit-diff-test-"));
  const db = openDb(join(scratch, "test.db"));
  try {
    migrate(db, { migrationsFolder });
    return fn(db);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

describe("recordCandidateDiff", () => {
  it("writes one row per node and returns per-file summaries", () => {
    withScratchDb((db) => {
      const result = recordCandidateDiff(db, makeCandidate());
      expect(result.filesProcessed).toBe(2);
      expect(result.rowsInserted).toBeGreaterThan(0);
      expect(result.perFile.map((p) => p.filePath).sort()).toEqual([
        "src/check.ts",
        "src/divide.ts",
      ]);
      for (const p of result.perFile) {
        const total = p.summary.unchanged + p.summary.modified + p.summary.added + p.summary.deleted;
        expect(total).toBeGreaterThan(0);
      }
    });
  });

  it("persists rows queryable by post coordinates", () => {
    withScratchDb((db) => {
      recordCandidateDiff(db, makeCandidate());
      const context = "harvest:synthetic:1";

      const ifAdded = db
        .select()
        .from(prePostDiff)
        .where(
          and(
            eq(prePostDiff.context, context),
            eq(prePostDiff.filePath, "src/divide.ts"),
            eq(prePostDiff.changeKind, "added"),
            eq(prePostDiff.postKind, "IfStatement"),
          ),
        )
        .all();
      expect(ifAdded.length).toBe(1);
      expect(ifAdded[0]!.postTextPreview).toMatch(/throw new Error/);

      const orChainAdded = db
        .select()
        .from(prePostDiff)
        .where(
          and(
            eq(prePostDiff.context, context),
            eq(prePostDiff.filePath, "src/check.ts"),
            eq(prePostDiff.changeKind, "added"),
            eq(prePostDiff.postKind, "BinaryExpression"),
          ),
        )
        .all();
      expect(orChainAdded.some((r) => /Baz/.test(r.postTextPreview ?? ""))).toBe(true);
    });
  });

  it("idempotent: clear + re-run produces the same row count", () => {
    withScratchDb((db) => {
      const c = makeCandidate();
      const first = recordCandidateDiff(db, c);
      const cleared = clearCandidateDiff(db, c.source.project, c.source.bugId);
      expect(cleared).toBe(first.rowsInserted);
      const second = recordCandidateDiff(db, c);
      expect(second.rowsInserted).toBe(first.rowsInserted);
    });
  });

  it("skips files that exist only on one side", () => {
    withScratchDb((db) => {
      const c = makeCandidate();
      // Add a deleted-in-fix file: present in pre, absent in post.
      c.buggyFiles["src/onlypre.ts"] = "function gone() { return 1; }";
      // Don't add to fixedFiles — so this file is skipped by the writer.
      const result = recordCandidateDiff(db, c);
      expect(result.perFile.map((p) => p.filePath).sort()).toEqual([
        "src/check.ts",
        "src/divide.ts",
      ]);
    });
  });
});

describe("active diff context", () => {
  it("setActiveDiffContext writes a single row", () => {
    withScratchDb((db) => {
      setActiveDiffContext(db, "harvest:synthetic:1");
      const rows = db.select().from(diffContextActive).all();
      expect(rows.length).toBe(1);
      expect(rows[0]!.context).toBe("harvest:synthetic:1");
    });
  });

  it("setActiveDiffContext replaces the previous row (singleton)", () => {
    withScratchDb((db) => {
      setActiveDiffContext(db, "harvest:foo:1");
      setActiveDiffContext(db, "harvest:bar:2");
      const rows = db.select().from(diffContextActive).all();
      expect(rows.length).toBe(1);
      expect(rows[0]!.context).toBe("harvest:bar:2");
    });
  });

  it("clearActiveDiffContext removes the row", () => {
    withScratchDb((db) => {
      setActiveDiffContext(db, "harvest:foo:1");
      clearActiveDiffContext(db);
      const rows = db.select().from(diffContextActive).all();
      expect(rows.length).toBe(0);
    });
  });

  it("setActiveCandidate composes the harvest-context key", () => {
    withScratchDb((db) => {
      setActiveCandidate(db, "express", "27");
      const rows = db.select().from(diffContextActive).all();
      expect(rows[0]!.context).toBe("harvest:express:27");
    });
  });
});
