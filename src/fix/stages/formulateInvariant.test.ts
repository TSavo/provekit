/**
 * C1: formulateInvariant tests.
 *
 * Tests the full path: principle-match → oracle #1 → InvariantClaim.
 * Also tests LLM fallback path and oracle #1 rejection of unsat/error.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { evaluatePrinciple } from "../../dsl/evaluator.js";
import { principleMatches, principleMatchCaptures } from "../../db/schema/principleMatches.js";
import { formulateInvariant } from "./formulateInvariant.js";
import { InvariantFormulationFailed } from "../types.js";
import type { BugSignal, BugLocus, LLMProvider } from "../types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-c1-test-"));
  const dbPath = join(tmpDir, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, tmpDir };
}

function writeFixture(dir: string, filename: string, source: string): string {
  mkdirSync(dir, { recursive: true });
  const filePath = join(dir, filename);
  writeFileSync(filePath, source, "utf8");
  return filePath;
}

function makeDivSignal(filePath: string): BugSignal {
  return {
    source: "test",
    rawText: "division by zero",
    summary: "divide(a, b) crashes when b is zero",
    failureDescription: "ZeroDivisionError: division by zero",
    codeReferences: [{ file: filePath, line: 1 }],
    bugClassHint: "division-by-zero",
  };
}

function makeLocus(primaryNode: string, filePath: string): BugLocus {
  return {
    file: filePath,
    line: 1,
    confidence: 1.0,
    primaryNode,
    containingFunction: primaryNode,
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

/** Division-by-zero DSL source. */
const DIVISION_BY_ZERO_DSL = `
principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
`;

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("formulateInvariant (C1)", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  afterEach(() => {
    try {
      db.$client.close();
    } catch {
      // ignore
    }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // Test 1: Principle-match path, division-by-zero — end-to-end
  // -------------------------------------------------------------------------
  it("principle-match path: returns InvariantClaim with principleId='division-by-zero' and SAT witness", async () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div.ts", source);
    buildSASTForFile(db, filePath);

    // Evaluate DSL to populate principle_matches + principle_match_captures.
    const matches = evaluatePrinciple(db, DIVISION_BY_ZERO_DSL);
    expect(matches.length).toBeGreaterThan(0);

    // Build locus pointing at the division expression node.
    const match = matches[0]!;
    const divNodeId = match.captures["division"] ?? match.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(divNodeId, filePath);

    const noLlm: LLMProvider = {
      complete: async () => {
        throw new Error("LLM should not be called when principle match exists");
      },
    };

    const claim = await formulateInvariant({ signal, locus, db, llm: noLlm });

    expect(claim.principleId).toBe("division-by-zero");
    expect(claim.description).toBeTruthy();
    expect(claim.formalExpression).toBeTruthy();
    expect(claim.formalExpression).toContain("(check-sat)");
    expect(claim.bindings.length).toBeGreaterThan(0);
    expect(claim.witness).not.toBeNull(); // oracle #1 confirmed SAT
    expect(typeof claim.complexity).toBe("number");
  });

  // -------------------------------------------------------------------------
  // Test 2: Template substitution completeness
  // -------------------------------------------------------------------------
  it("formalExpression has no unreplaced {{...}} placeholders and is complete SMT", async () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div2.ts", source);
    buildSASTForFile(db, filePath);

    const matches = evaluatePrinciple(db, DIVISION_BY_ZERO_DSL);
    const match = matches[0]!;
    const divNodeId = match.captures["division"] ?? match.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(divNodeId, filePath);

    const noLlm: LLMProvider = {
      complete: async () => {
        throw new Error("LLM should not be called");
      },
    };

    const claim = await formulateInvariant({ signal, locus, db, llm: noLlm });

    // No unreplaced placeholders.
    expect(claim.formalExpression).not.toMatch(/\{\{[^}]+\}\}/);

    // Must contain (declare-const, (assert, and (check-sat).
    expect(claim.formalExpression).toContain("(declare-const");
    expect(claim.formalExpression).toContain("(assert");
    expect(claim.formalExpression).toContain("(check-sat)");
  });

  // -------------------------------------------------------------------------
  // Test 3: Oracle #1 failure — unsat (vacuous invariant)
  // -------------------------------------------------------------------------
  it("throws InvariantFormulationFailed when principle-match SMT template is vacuous (always unsat)", async () => {
    ({ db, tmpDir } = openTestDb());
    // Use a simple file with no arithmetic — so no division-by-zero matches will be inserted.
    const source = "function safe(x: number) { return x + 1; }\n";
    const filePath = writeFixture(tmpDir, "safe3.ts", source);
    const result = buildSASTForFile(db, filePath);

    // Use the root node id as the locus primary node.
    const primaryNodeId = result.rootNodeId;

    // Write a fake principle JSON with a always-unsat template.
    const principlesDir = join(process.cwd(), ".provekit", "principles");
    const fakePrincipleId = "test-vacuous-principle";
    const fakePrinciplePath = join(principlesDir, `${fakePrincipleId}.json`);
    let createdFake = false;
    try {
      writeFileSync(fakePrinciplePath, JSON.stringify({
        id: fakePrincipleId,
        description: "vacuous test principle",
        // Template with a placeholder but always-unsat assertion (contradiction).
        smt2Template: "(declare-const {{vx}} Int)\n(assert (and (= {{vx}} 0) (not (= {{vx}} 0))))\n(check-sat)",
      }), "utf8");
      createdFake = true;

      // Insert a principle_match pointing at the root node for the fake principle.
      const fileRow = db
        .select({ id: (await import("../../sast/schema/index.js")).files.id })
        .from((await import("../../sast/schema/index.js")).files)
        .where((await import("drizzle-orm")).eq(
          (await import("../../sast/schema/index.js")).files.path, filePath,
        ))
        .get();
      const fileId = fileRow?.id ?? 1;

      const inserted = db.insert(principleMatches).values({
        principleName: fakePrincipleId,
        fileId,
        rootMatchNodeId: primaryNodeId,
        severity: "violation",
        message: "test vacuous",
      }).returning({ id: principleMatches.id }).get();

      const matchId = inserted?.id ?? 0;
      db.insert(principleMatchCaptures).values({
        matchId,
        captureName: "vx",  // matches placeholder {{vx}}
        capturedNodeId: primaryNodeId,
      }).run();

      const signal: BugSignal = {
        source: "test",
        rawText: "test",
        summary: "test",
        failureDescription: "test",
        codeReferences: [{ file: filePath, line: 1 }],
      };
      const locus = makeLocus(primaryNodeId, filePath);
      const noLlm: LLMProvider = { complete: async () => { throw new Error("no llm"); } };

      await expect(
        formulateInvariant({ signal, locus, db, llm: noLlm }),
      ).rejects.toThrow(InvariantFormulationFailed);

      await expect(
        formulateInvariant({ signal, locus, db, llm: noLlm }),
      ).rejects.toThrow(/unsat|error/i);
    } finally {
      if (createdFake) {
        try { rmSync(fakePrinciplePath); } catch { /* ignore */ }
      }
    }
  });

  // -------------------------------------------------------------------------
  // Test 4: Novel LLM path — no principle_matches, stub returns valid SMT
  // -------------------------------------------------------------------------
  it("novel LLM path: principleId=null, formalExpression from stub, oracle #1 confirms SAT", async () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function risky(x: number) { return 1 / x; }\n";
    const filePath = writeFixture(tmpDir, "risky.ts", source);
    buildSASTForFile(db, filePath);

    // principle_matches table is empty — no matches for this locus.
    // Make up a node ID that won't be in principle_matches.
    const fakeNodeId = "fake-node-id-not-in-db";

    const signal: BugSignal = {
      source: "test",
      rawText: "risky division",
      summary: "division by zero risk in risky()",
      failureDescription: "x could be zero",
      codeReferences: [{ file: filePath, line: 1 }],
    };
    const locus = makeLocus(fakeNodeId, filePath);

    // Stub returns valid SMT that Z3 will find SAT.
    const validSmtResponse = JSON.stringify({
      description: "x must not be zero before division",
      smt_declarations: ["(declare-const x Int)"],
      smt_violation_assertion: "(assert (= x 0))",
      bindings: [{ smt_constant: "x", source_expr: "x", sort: "Int" }],
    });

    const stubLlm: LLMProvider = {
      complete: async () => validSmtResponse,
    };

    const claim = await formulateInvariant({ signal, locus, db, llm: stubLlm });

    expect(claim.principleId).toBeNull();
    expect(claim.description).toContain("zero");
    expect(claim.formalExpression).toContain("(declare-const x Int)");
    expect(claim.formalExpression).toContain("(assert (= x 0))");
    expect(claim.formalExpression).toContain("(check-sat)");
    expect(claim.bindings).toHaveLength(1);
    expect(claim.bindings[0]!.smt_constant).toBe("x");
    expect(claim.witness).not.toBeNull(); // Z3 found SAT
  });

  // -------------------------------------------------------------------------
  // Test 5: Novel LLM path — stub returns malformed (non-JSON) response
  // -------------------------------------------------------------------------
  it("novel LLM path: throws InvariantFormulationFailed when LLM returns non-JSON", async () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function f(x: number) { return x; }\n";
    const filePath = writeFixture(tmpDir, "safe.ts", source);
    buildSASTForFile(db, filePath);

    const fakeNodeId = "fake-node-id-for-llm-error";
    const signal: BugSignal = {
      source: "test",
      rawText: "some bug",
      summary: "some bug",
      failureDescription: "something broke",
      codeReferences: [{ file: filePath, line: 1 }],
    };
    const locus = makeLocus(fakeNodeId, filePath);

    const badLlm: LLMProvider = {
      complete: async () => "This is not JSON at all, just prose describing the bug.",
    };

    await expect(
      formulateInvariant({ signal, locus, db, llm: badLlm }),
    ).rejects.toThrow(InvariantFormulationFailed);
  });

  // Note: "unknown" verdict path is covered by runOracleOne's switch statement;
  // no separate test — Z3-backed timeout injection is fragile in CI.

  // -------------------------------------------------------------------------
  // Test 6: Novel LLM path — LLM returns unsat SMT → InvariantFormulationFailed
  // -------------------------------------------------------------------------
  it("novel LLM path: throws InvariantFormulationFailed when LLM SMT is always unsat", async () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function g(x: number) { return x * 2; }\n";
    const filePath = writeFixture(tmpDir, "safe2.ts", source);
    buildSASTForFile(db, filePath);

    const fakeNodeId = "fake-node-id-for-unsat";
    const signal: BugSignal = {
      source: "test",
      rawText: "some bug",
      summary: "some bug",
      failureDescription: "something broke",
      codeReferences: [{ file: filePath, line: 1 }],
    };
    const locus = makeLocus(fakeNodeId, filePath);

    // Return a contradictory SMT that's always UNSAT.
    const unsatSmtResponse = JSON.stringify({
      description: "contradictory invariant",
      smt_declarations: ["(declare-const y Int)"],
      smt_violation_assertion: "(assert (and (= y 0) (not (= y 0))))",
      bindings: [{ smt_constant: "y", source_expr: "y", sort: "Int" }],
    });

    const stubLlm: LLMProvider = {
      complete: async () => unsatSmtResponse,
    };

    await expect(
      formulateInvariant({ signal, locus, db, llm: stubLlm }),
    ).rejects.toThrow(InvariantFormulationFailed);

    await expect(
      formulateInvariant({ signal, locus, db, llm: stubLlm }),
    ).rejects.toThrow(/unsat|oracle/i);
  });
});
