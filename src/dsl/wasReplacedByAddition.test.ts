/**
 * Integration tests for the diff-aware DSL relation `was_replaced_by_addition`
 * (hard-bug 1, Day 3).
 *
 * Exercises the full path the mining pipeline takes:
 *   1. Build SAST from the buggy file (writes `nodes` rows)
 *   2. recordCandidateDiff (writes `pre_post_diff` rows)
 *   3. setActiveCandidate (writes `diff_context_active`)
 *   4. Compile + evaluate the `or-chain-extended-by-fix` principle
 *   5. Assert: matches the inner buggy OR
 *
 * Plus the dormant-when-no-context contract (relation evaluates to false
 * without an active context — pure SAST runs see no diff-aware false
 * positives) and a negative case where the structural condition isn't
 * met (no enclosing addition).
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "fs";
import { join, dirname } from "path";
import { tmpdir } from "os";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb, type Db } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import {
  recordCandidateDiff,
  setActiveCandidate,
  clearActiveDiffContext,
} from "../fix/harvest/diff.js";
import { parseDSL } from "./parser.js";
import { compileProgram } from "./compiler.js";
import "./relations.js"; // self-registers built-ins
import type { HarvestCandidate } from "../fix/harvest/extractBugs.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "..", "drizzle");
const principleSource = readFileSync(
  join(__dirname, "..", "..", ".provekit", "principles", "or-chain-extended-by-fix.dsl"),
  "utf-8",
);

interface Setup {
  db: Db;
  bugFile: string;
  cleanup: () => void;
}

function setupScratch(buggy: string, fixed: string): Setup {
  const scratch = mkdtempSync(join(tmpdir(), "provekit-relation-test-"));
  const dbPath = join(scratch, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder });

  const bugFile = join(scratch, "src", "check.ts");
  mkdirSync(dirname(bugFile), { recursive: true });
  writeFileSync(bugFile, buggy, "utf-8");
  buildSASTForFile(db, bugFile);

  const candidate: HarvestCandidate = {
    source: {
      project: "synthetic",
      bugId: "1",
      baseSha: "deadbeef",
      fixSha: "f1xc0de",
      testSha: null,
      originalSha: null,
    },
    buggyFiles: { [bugFile]: buggy },
    fixedFiles: { [bugFile]: fixed },
    diff: "",
    upstreamFixMessage: "",
    testFiles: {},
    stats: { filesChanged: 1, insertions: 1, deletions: 0 },
  };
  recordCandidateDiff(db, candidate);

  return { db, bugFile, cleanup: () => rmSync(scratch, { recursive: true, force: true }) };
}

function compilePrinciple() {
  const program = parseDSL(principleSource);
  const compiled = compileProgram(program.nodes);
  const principle = compiled.get("or-chain-extended-by-fix");
  if (!principle) throw new Error("principle did not compile");
  return principle;
}

describe("was_replaced_by_addition (or-chain-extended-by-fix integration)", () => {
  it("fires on the buggy OR when the fix wraps it in an extended OR-chain", () => {
    const buggy = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`;
    const fixed = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`;
    const { db, cleanup } = setupScratch(buggy, fixed);
    try {
      setActiveCandidate(db, "synthetic", "1");
      const principle = compilePrinciple();
      const matches = principle(db);
      expect(matches.length).toBeGreaterThan(0);
      // The bound atNodeId should be a node in the buggy file.
      expect(matches[0]!.atNodeId).toBeTruthy();
    } finally {
      cleanup();
    }
  });

  it("dormant without active diff context", () => {
    const buggy = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`;
    const fixed = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`;
    const { db, cleanup } = setupScratch(buggy, fixed);
    try {
      // Don't setActiveCandidate. The relation joins diff_context_active
      // and that table is empty → relation returns false → no matches.
      const principle = compilePrinciple();
      const matches = principle(db);
      expect(matches.length).toBe(0);
    } finally {
      cleanup();
    }
  });

  it("re-clearing context turns the relation back off", () => {
    const buggy = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`;
    const fixed = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`;
    const { db, cleanup } = setupScratch(buggy, fixed);
    try {
      setActiveCandidate(db, "synthetic", "1");
      const principle = compilePrinciple();
      expect(principle(db).length).toBeGreaterThan(0);
      clearActiveDiffContext(db);
      expect(principle(db).length).toBe(0);
    } finally {
      cleanup();
    }
  });

  it("does NOT fire when the buggy OR is unchanged in the fix (no enclosing addition)", () => {
    // Pre and post are identical for the OR — the fix touches an
    // unrelated part of the function. The OR pairs unchanged on both
    // sides, but no `added` post node encloses it. Relation must
    // return false.
    const buggy = `function check(t: string): boolean {
  console.log("checking");
  return t === "Foo" || t === "Bar";
}`;
    const fixed = `function check(t: string): boolean {
  console.warn("checking");
  return t === "Foo" || t === "Bar";
}`;
    const { db, cleanup } = setupScratch(buggy, fixed);
    try {
      setActiveCandidate(db, "synthetic", "1");
      const principle = compilePrinciple();
      const matches = principle(db);
      expect(matches.length).toBe(0);
    } finally {
      cleanup();
    }
  });

  it("does NOT fire when the buggy OR is replaced (no unchanged pairing)", () => {
    // Pre OR fingerprint has no match in post — the fix completely
    // replaced the expression. Relation must return false.
    const buggy = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`;
    const fixed = `function check(t: string): boolean {
  return ["Foo", "Bar", "Baz"].includes(t);
}`;
    const { db, cleanup } = setupScratch(buggy, fixed);
    try {
      setActiveCandidate(db, "synthetic", "1");
      const principle = compilePrinciple();
      const matches = principle(db);
      expect(matches.length).toBe(0);
    } finally {
      cleanup();
    }
  });
});
