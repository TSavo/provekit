/**
 * C3: candidateGen tests.
 *
 * Tests generateFixCandidate, verifyCandidate, parseProposedFixes, and buildFixPrompt.
 * Each test that needs overlay machinery creates its own tempdir with git init + initial commit.
 *
 * Key decisions reflected in tests:
 * - "bug site removed" = zero principle_matches in overlay after patch → oracle #2 passes
 * - Novel path: all binding source_expr strings absent from modified files → oracle #2 passes
 * - "unknown" is failure (spec decision #6)
 * - Overlay is NOT closed by C3 — audit.overlayClosed is always false
 */

import { describe, it, expect, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  cpSync,
  existsSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";
import { generateFixCandidate } from "./stages/generateFixCandidate.js";
import { parseProposedFixes, buildFixPrompt, runOracleTwo } from "./candidateGen.js";
import { openOverlay } from "./stages/openOverlay.js";
import { applyPatchToOverlay, reindexOverlay, closeOverlay } from "./overlay.js";
import type {
  BugSignal,
  BugLocus,
  InvariantClaim,
  LLMProvider,
} from "./types.js";
import { StubLLMProvider } from "./types.js";

// ---------------------------------------------------------------------------
// Git config for test commits
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeTestRepo(content: string, filename = "fixture.ts"): { repoDir: string; filePath: string } {
  const repoDir = mkdtempSync(join(tmpdir(), "provekit-c3-test-repo-"));
  execFileSync("git", [...GIT_ID, "init", repoDir]);
  execFileSync("git", [...GIT_ID, "init"], { cwd: repoDir });

  const filePath = join(repoDir, filename);
  writeFileSync(filePath, content, "utf8");

  execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
  execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

  return { repoDir, filePath };
}

function openMainDb(dir: string) {
  const dbPath = join(dir, "main.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

function makeLocus(filePath: string, primaryNode: string, line = 1): BugLocus {
  return {
    file: filePath,
    line,
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

/**
 * Build a division-by-zero InvariantClaim with principleId set.
 * formalExpression is the violation SMT (always SAT when denominator=0 is assertable).
 */
function makeDivInvariant(): InvariantClaim {
  return {
    principleId: "division-by-zero",
    description: "Division where denominator may be zero",
    formalExpression:
      "(declare-const numerator Int)\n(declare-const denominator Int)\n(assert (= denominator 0))\n(check-sat)",
    bindings: [
      { smt_constant: "numerator", source_line: 1, source_expr: "a", sort: "Int" },
      { smt_constant: "denominator", source_line: 1, source_expr: "b", sort: "Int" },
    ],
    complexity: 1,
    witness: "sat",
  };
}

/**
 * Build a novel InvariantClaim (principleId null) with source_expr bindings.
 * formalExpression is always SAT (violation state).
 */
function makeNovelInvariant(sourceExprs: string[]): InvariantClaim {
  return {
    principleId: null,
    description: "Novel invariant: dangerous expression present",
    formalExpression:
      "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
    bindings: sourceExprs.map((expr, i) => ({
      smt_constant: `v${i}`,
      source_line: 1,
      source_expr: expr,
      sort: "Int",
    })),
    complexity: 1,
    witness: "sat",
  };
}

/**
 * Copy the project's real .provekit/principles to the repo so openOverlay can find them.
 */
function seedPrinciples(repoDir: string): void {
  const src = join(process.cwd(), ".provekit", "principles");
  const dst = join(repoDir, ".provekit", "principles");
  mkdirSync(dst, { recursive: true });
  if (existsSync(src)) {
    cpSync(src, dst, { recursive: true });
    execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
    execFileSync("git", [...GIT_ID, "commit", "-m", "add principles"], { cwd: repoDir });
  }
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("C3: candidateGen", () => {
  const cleanups: (() => void | Promise<void>)[] = [];

  afterEach(async () => {
    for (const fn of cleanups.splice(0)) {
      try { await fn(); } catch { /* ignore */ }
    }
  });

  // -------------------------------------------------------------------------
  // 1. parseProposedFixes — happy path
  // -------------------------------------------------------------------------
  it("parseProposedFixes: parses valid JSON into ProposedFix array", () => {
    const raw = JSON.stringify({
      candidates: [
        {
          rationale: "Add a guard",
          confidence: 0.9,
          patch: {
            description: "add guard",
            fileEdits: [{ file: "src/foo.ts", newContent: "// guarded" }],
          },
        },
        {
          rationale: "Remove division",
          confidence: 0.7,
          patch: {
            description: "remove division",
            fileEdits: [{ file: "src/bar.ts", newContent: "// safe" }],
          },
        },
      ],
    });

    const fixes = parseProposedFixes(raw);
    expect(fixes).toHaveLength(2);
    expect(fixes[0]!.confidence).toBe(0.9);
    expect(fixes[0]!.patch.fileEdits[0]!.file).toBe("src/foo.ts");
    expect(fixes[1]!.rationale).toBe("Remove division");
  });

  // -------------------------------------------------------------------------
  // 2. parseProposedFixes — malformed JSON throws
  // -------------------------------------------------------------------------
  it("parseProposedFixes: throws on non-JSON input", () => {
    expect(() => parseProposedFixes("not json at all")).toThrow(/not valid JSON/);
  });

  // -------------------------------------------------------------------------
  // 3. parseProposedFixes — missing candidates array throws
  // -------------------------------------------------------------------------
  it("parseProposedFixes: throws when 'candidates' key is missing", () => {
    expect(() => parseProposedFixes(JSON.stringify({ foo: "bar" }))).toThrow(/expected.*candidates/i);
  });

  // -------------------------------------------------------------------------
  // 4. parseProposedFixes — skips malformed candidates, keeps valid ones
  // -------------------------------------------------------------------------
  it("parseProposedFixes: skips malformed candidates and keeps valid ones", () => {
    const raw = JSON.stringify({
      candidates: [
        { rationale: "bad — no patch", confidence: 0.8 },
        {
          rationale: "good one",
          confidence: 0.7,
          patch: {
            description: "safe",
            fileEdits: [{ file: "f.ts", newContent: "ok" }],
          },
        },
      ],
    });

    const fixes = parseProposedFixes(raw);
    expect(fixes).toHaveLength(1);
    expect(fixes[0]!.rationale).toBe("good one");
  });

  // -------------------------------------------------------------------------
  // 5. buildFixPrompt — includes required fields
  // -------------------------------------------------------------------------
  it("buildFixPrompt: prompt includes signal, locus, invariant details", () => {
    const signal = makeDivSignal("/tmp/fake.ts");
    const locus = makeLocus("/tmp/fake.ts", "node1");
    const invariant = makeDivInvariant();
    const prompt = buildFixPrompt(signal, locus, invariant, 3);

    expect(prompt).toContain(signal.summary);
    expect(prompt).toContain(signal.failureDescription);
    expect(prompt).toContain(invariant.description);
    expect(prompt).toContain(invariant.formalExpression);
    expect(prompt).toContain("3");  // maxCandidates
  });

  // -------------------------------------------------------------------------
  // 6. generateFixCandidate — happy path: principle match removed by fix
  // -------------------------------------------------------------------------
  it("happy path: fix removes division → principle_matches zero → oracle #2 passes", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(matches.length).toBeGreaterThan(0);

    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    // Patch removes the division entirely.
    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Replace division with constant to remove the bug site",
          confidence: 0.95,
          patch: {
            description: "remove division",
            fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
          },
        },
      ],
    });

    // StubLLMProvider matches prompt substring against key — "divide" appears in the prompt
    // (function name in the source context), making it a reliable match key.
    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    expect(candidate.invariantHoldsUnderOverlay).toBe(true);
    expect(candidate.overlayZ3Verdict).toBe("unsat");  // bug_site_removed → mapped to unsat
    expect(candidate.audit.patchApplied).toBe(true);
    expect(candidate.audit.overlayReindexed).toBe(true);
    expect(candidate.audit.overlayClosed).toBe(false);  // orchestrator owns lifecycle
    expect(candidate.llmConfidence).toBe(0.95);
    expect(candidate.patch.fileEdits).toHaveLength(1);
    expect(candidate.patch.fileEdits[0]!.newContent).toContain("return 0");
  }, 60_000);

  // -------------------------------------------------------------------------
  // 7. generateFixCandidate — bogus fix does NOT remove bug site → oracle #2 fails → throw
  // -------------------------------------------------------------------------
  it("rejected candidate: fix renames variable but leaves division → oracle #2 fails → throw", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb2-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(matches.length).toBeGreaterThan(0);

    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    // Bogus patch: still has division, just renamed params.
    const bogusContent = "export function divide(x: number, y: number) { return x / y; }\n";
    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Rename params (does not fix the bug)",
          confidence: 0.6,
          patch: {
            description: "rename params only",
            fileEdits: [{ file: "fixture.ts", newContent: bogusContent }],
          },
        },
      ],
    });

    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    await expect(
      generateFixCandidate({ signal, locus, invariant, overlay, llm }),
    ).rejects.toThrow(/no candidate survived oracle #2/);
  }, 60_000);

  // -------------------------------------------------------------------------
  // 8. generateFixCandidate — multiple candidates: first rejected, second accepted
  // -------------------------------------------------------------------------
  it("multiple candidates: first fails oracle #2, second passes → returns second", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb3-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    const bogusContent = "export function divide(x: number, y: number) { return x / y; }\n";
    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";

    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Bad fix — still divides",
          confidence: 0.8,
          patch: {
            description: "bogus",
            fileEdits: [{ file: "fixture.ts", newContent: bogusContent }],
          },
        },
        {
          rationale: "Good fix — removes division",
          confidence: 0.7,
          patch: {
            description: "remove division",
            fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
          },
        },
      ],
    });

    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    // Must be the second candidate (the good one).
    expect(candidate.invariantHoldsUnderOverlay).toBe(true);
    expect(candidate.llmRationale).toBe("Good fix — removes division");
    expect(candidate.overlayZ3Verdict).toBe("unsat");
  }, 60_000);

  // -------------------------------------------------------------------------
  // 9. generateFixCandidate — all candidates below minConfidence → throw
  // -------------------------------------------------------------------------
  it("all candidates below minConfidence → throw with clear message", async () => {
    const source = "export function add(a: number, b: number) { return a + b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb4-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);

    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, "aaaa000000000000");
    const invariant = makeDivInvariant();

    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "low confidence fix",
          confidence: 0.3,
          patch: {
            description: "something",
            fileEdits: [{ file: "fixture.ts", newContent: "export const x = 1;" }],
          },
        },
      ],
    });

    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    await expect(
      generateFixCandidate({
        signal,
        locus,
        invariant,
        overlay,
        llm,
        options: { minConfidence: 0.5 },
      }),
    ).rejects.toThrow(/below minConfidence/);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 10. generateFixCandidate — LLM returns malformed JSON → throw
  // -------------------------------------------------------------------------
  it("LLM returns malformed JSON → throw parse error", async () => {
    const source = "export function add(a: number, b: number) { return a + b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb5-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);

    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, "aaaa000000000000");
    const invariant = makeDivInvariant();

    const llm = new StubLLMProvider(new Map([["divide", "this is not json at all"]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    await expect(
      generateFixCandidate({ signal, locus, invariant, overlay, llm }),
    ).rejects.toThrow(/not valid JSON/);
  }, 30_000);

  // -------------------------------------------------------------------------
  // 11. Novel invariant (principleId null): source_expr absent → oracle #2 passes
  // -------------------------------------------------------------------------
  it("novel invariant: patched file removes source_expr → bug_site_removed → oracle #2 passes", async () => {
    // Source contains the dangerous expression "DANGER_EXPR".
    const source = 'export function risky() { return "DANGER_EXPR"; }\n';
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb6-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);

    const signal: BugSignal = {
      source: "test",
      rawText: "dangerous expression",
      summary: "risky() uses DANGER_EXPR",
      failureDescription: "DANGER_EXPR is dangerous",
      codeReferences: [{ file: filePath, line: 1 }],
    };
    const locus = makeLocus(filePath, "aaaa000000000000");

    // Novel invariant with source_expr = "DANGER_EXPR"
    const invariant = makeNovelInvariant(["DANGER_EXPR"]);

    // Fixed content removes DANGER_EXPR entirely.
    const fixedContent = 'export function risky() { return "SAFE"; }\n';
    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Replace DANGER_EXPR with SAFE",
          confidence: 0.9,
          patch: {
            description: "replace dangerous expr",
            fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
          },
        },
      ],
    });

    const llm = new StubLLMProvider(new Map([["dangerous", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    expect(candidate.invariantHoldsUnderOverlay).toBe(true);
    expect(candidate.overlayZ3Verdict).toBe("unsat");
    expect(candidate.llmRationale).toContain("DANGER_EXPR");
  }, 60_000);

  // -------------------------------------------------------------------------
  // 12. Audit trail: all fields populated correctly
  // -------------------------------------------------------------------------
  it("audit trail: all fields populated; overlayClosed is false (orchestrator owns lifecycle)", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb7-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Remove division",
          confidence: 0.95,
          patch: {
            description: "remove division",
            fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
          },
        },
      ],
    });

    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    // All audit fields set.
    expect(candidate.audit.overlayCreated).toBe(true);
    expect(candidate.audit.patchApplied).toBe(true);
    expect(candidate.audit.overlayReindexed).toBe(true);
    expect(candidate.audit.z3RunMs).toBeGreaterThanOrEqual(0);
    // C3 does NOT close the overlay — orchestrator owns the lifecycle.
    expect(candidate.audit.overlayClosed).toBe(false);
  }, 60_000);

  // -------------------------------------------------------------------------
  // 13. runOracleTwo — novel invariant with source_expr still present → fallback Z3 (sat = fail)
  // -------------------------------------------------------------------------
  it("runOracleTwo: novel invariant, source_expr still in file → fallback Z3 returns sat (failure path)", async () => {
    const source = 'export function risky() { return "DANGER_EXPR"; }\n';
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb8-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);

    const locus = makeLocus(filePath, "aaaa000000000000");
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // Apply a patch that does NOT remove DANGER_EXPR.
    const sameContent = 'export function risky() { return "DANGER_EXPR" + ""; }\n';
    applyPatchToOverlay(overlay, {
      fileEdits: [{ file: "fixture.ts", newContent: sameContent }],
      description: "does not remove DANGER_EXPR",
    });
    await reindexOverlay(overlay);

    // Novel invariant with source_expr = "DANGER_EXPR" (still present).
    const invariant = makeNovelInvariant(["DANGER_EXPR"]);

    const verdict = await runOracleTwo(overlay, invariant);
    // DANGER_EXPR still present → fallback Z3 on formalExpression
    // formalExpression is "(assert (= x 0))" which is always sat.
    expect(verdict).toBe("sat");
  }, 30_000);

  // -------------------------------------------------------------------------
  // 14. Agent path: StubLLMProvider with agentResponses → agent path runs
  // -------------------------------------------------------------------------
  it("agent path: StubLLM with agentResponses configured → generateFixCandidate uses agent path", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb9-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(matches.length).toBeGreaterThan(0);

    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";

    // StubLLMProvider with agentResponses → agent field will be defined.
    const llm = new StubLLMProvider(
      new Map(),
      [{ matchPrompt: "divide", fileEdits: [{ file: "fixture.ts", newContent: fixedContent }], text: "Removed division" }],
    );
    expect(typeof llm.agent).toBe("function");

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    expect(candidate.invariantHoldsUnderOverlay).toBe(true);
    expect(candidate.overlayZ3Verdict).toBe("unsat");
    expect(candidate.patch.fileEdits[0]!.newContent).toContain("return 0");
  }, 60_000);

  // -------------------------------------------------------------------------
  // 15. Backward compat: StubLLMProvider without agentResponses → JSON path
  // -------------------------------------------------------------------------
  it("backward compat: StubLLMProvider without agentResponses → falls through to JSON patch path", async () => {
    const source = "export function divide(a: number, b: number) { return a / b; }\n";
    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c3-maindb10-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const matches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const divNodeId = matches[0]!.captures["division"] ?? matches[0]!.rootNodeId;
    const signal = makeDivSignal(filePath);
    const locus = makeLocus(filePath, divNodeId);
    const invariant = makeDivInvariant();

    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const patchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Remove division (JSON path)",
          confidence: 0.95,
          patch: {
            description: "remove division",
            fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
          },
        },
      ],
    });

    // No agentResponses → agent field is undefined → JSON path.
    const llm = new StubLLMProvider(new Map([["divide", patchJson]]));
    expect(llm.agent).toBeUndefined();

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const candidate = await generateFixCandidate({ signal, locus, invariant, overlay, llm });

    expect(candidate.invariantHoldsUnderOverlay).toBe(true);
    expect(candidate.overlayZ3Verdict).toBe("unsat");
    expect(candidate.llmRationale).toContain("JSON path");
  }, 60_000);
});
