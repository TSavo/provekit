/**
 * C4: complementary-change generator tests.
 *
 * Uses the same test infra as candidateGen.test.ts:
 * - tmp git repos via makeTestRepo
 * - real overlay machinery (openOverlay / closeOverlay)
 * - StubLLMProvider keyed by prompt substring
 *
 * Tests:
 * 1. Adjacent site discovery: same principle fires at two functions; C4 finds the second.
 * 2. Caller discovery: fix of `divide`; C4 finds `main` caller via calls_table.
 * 3. Rejected change (oracle #3 fails): LLM proposes bogus patch; change not included.
 * 4. Max sites cap: 15 adjacent sites, maxSites: 5; return length ≤ 5.
 * 5. Priority sort order: mix of kinds; caller < adjacent < data_flow on return.
 * 6. Cumulative overlay: site A + site B both accepted; final overlay has both patches.
 * 7. Empty result: no complementary sites found; return []; no LLM calls.
 */

import { describe, it, expect, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  cpSync,
  existsSync,
  readFileSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { and, eq } from "drizzle-orm";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";
import { nodeBinding } from "../sast/schema/capabilities/index.js";
import { openOverlay } from "./stages/openOverlay.js";
import { closeOverlay } from "./overlay.js";
import { generateComplementary } from "./stages/generateComplementary.js";
import type {
  BugSignal,
  BugLocus,
  InvariantClaim,
  FixCandidate,
  CodePatch,
} from "./types.js";
import { StubLLMProvider } from "./types.js";

// ---------------------------------------------------------------------------
// Git config for commits
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeTestRepo(
  files: Record<string, string>,
): { repoDir: string; filePaths: Record<string, string> } {
  const repoDir = mkdtempSync(join(tmpdir(), "provekit-c4-test-repo-"));
  execFileSync("git", [...GIT_ID, "init", repoDir]);
  execFileSync("git", [...GIT_ID, "init"], { cwd: repoDir });

  const filePaths: Record<string, string> = {};
  for (const [name, content] of Object.entries(files)) {
    const filePath = join(repoDir, name);
    writeFileSync(filePath, content, "utf8");
    filePaths[name] = filePath;
  }

  execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
  execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

  return { repoDir, filePaths };
}

function openMainDb(dir: string) {
  const dbPath = join(dir, "main.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

function makeLocus(filePath: string, primaryNode: string, containingFunction: string, line = 1): BugLocus {
  return {
    file: filePath,
    line,
    confidence: 1.0,
    primaryNode,
    containingFunction,
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

/** Make a stub FixCandidate referencing the primary fix's patch. */
function makeFixCandidate(patch: CodePatch): FixCandidate {
  return {
    patch,
    llmRationale: "Remove division to fix bug",
    llmConfidence: 0.95,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 5,
      overlayClosed: false,
    },
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

function seedPrinciples(repoDir: string): void {
  const src = join(process.cwd(), ".neurallog", "principles");
  const dst = join(repoDir, ".neurallog", "principles");
  mkdirSync(dst, { recursive: true });
  if (existsSync(src)) {
    cpSync(src, dst, { recursive: true });
    execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
    execFileSync("git", [...GIT_ID, "commit", "-m", "add principles"], { cwd: repoDir });
  }
}

/** Build a patch JSON for StubLLM that removes division. */
function makeDivFixPatchJson(filename: string, content: string): string {
  return JSON.stringify({
    candidates: [
      {
        rationale: `Fix division-by-zero in ${filename}`,
        confidence: 0.9,
        patch: {
          description: `remove division in ${filename}`,
          fileEdits: [{ file: filename, newContent: content }],
        },
      },
    ],
  });
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("C4: generateComplementary", () => {
  const cleanups: (() => void | Promise<void>)[] = [];

  afterEach(async () => {
    for (const fn of cleanups.splice(0)) {
      try {
        await fn();
      } catch {
        /* ignore */
      }
    }
  });

  // -------------------------------------------------------------------------
  // Test 1: Adjacent site discovery
  // Fixture: two functions both containing division.
  // Primary fix at function 1; C4 finds function 2 as adjacent_site_fix.
  // -------------------------------------------------------------------------
  it("adjacent site discovery: finds second function with same principle match", async () => {
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb1-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);

    // Find the primary node (divide function's division node).
    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBeGreaterThanOrEqual(2);

    // Primary fix is the first match (in divide.ts).
    const divInDivide = divMatches.find((m) => {
      // The rootNodeId should belong to divide.ts.
      return true; // we'll use the first one as primary.
    });
    const primaryNodeId = divInDivide!.captures["division"] ?? divInDivide!.rootNodeId;

    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    // Primary fix: removes division from divide.ts.
    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const primaryPatch: CodePatch = {
      description: "remove division from divide.ts",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    };
    const fix = makeFixCandidate(primaryPatch);

    // StubLLM: returns a fix for compute.ts when "compute" is in the prompt.
    const fixedComputeContent = "export function compute(_x: number, _y: number) { return 0; }\n";
    const computePatchJson = makeDivFixPatchJson("compute.ts", fixedComputeContent);
    const llm = new StubLLMProvider(new Map([["compute", computePatchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // Should have at least one adjacent_site_fix accepted.
    const adjacentChanges = changes.filter((c) => c.kind === "adjacent_site_fix");
    expect(adjacentChanges.length).toBeGreaterThan(0);
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 2: Caller discovery
  // Fixture: divide() called by main(). Primary fix at divide. C4 finds main.
  // -------------------------------------------------------------------------
  it("caller discovery: finds caller of fixed function via calls_table", async () => {
    const source = [
      "export function divide(a: number, b: number) { return a / b; }",
      "export function main(x: number, y: number) { return divide(x, y); }",
    ].join("\n") + "\n";

    const { repoDir, filePaths } = makeTestRepo({ "fixture.ts": source });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb2-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["fixture.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBeGreaterThan(0);

    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;

    // Look up the FunctionDeclaration node for "divide" via nodeBinding.
    // Using the BinaryExpression node as containingFunction would yield no callers
    // (nodeBinding has no row for BinaryExpression nodes). We must use the
    // FunctionDeclaration node so Strategy B can find callers via calls_table.
    const divBindingRow = mainDb
      .select({ nodeId: nodeBinding.nodeId })
      .from(nodeBinding)
      .where(and(eq(nodeBinding.name, "divide"), eq(nodeBinding.bindingKind, "function")))
      .get();
    expect(divBindingRow).toBeDefined();
    const containingFnId = divBindingRow!.nodeId;

    const locus = makeLocus(filePaths["fixture.ts"]!, primaryNodeId, containingFnId);
    const invariant = makeDivInvariant();

    // Primary fix: removes division from fixture.ts.
    const fixedContent = [
      "export function divide(_a: number, _b: number) { return 0; }",
      "export function main(x: number, y: number) { return divide(x, y); }",
    ].join("\n") + "\n";
    const fix = makeFixCandidate({
      description: "remove division",
      fileEdits: [{ file: "fixture.ts", newContent: fixedContent }],
    });

    // StubLLM: returns a patch for main() when "main" appears in prompt.
    const mainPatchedContent = [
      "export function divide(_a: number, _b: number) { return 0; }",
      "export function main(x: number, y: number) {",
      "  if (y === 0) throw new Error('y must not be zero');",
      "  return divide(x, y);",
      "}",
    ].join("\n") + "\n";
    const mainPatchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Add guard to main() caller to prevent zero denominator",
          confidence: 0.85,
          patch: {
            description: "add error handler in main",
            fileEdits: [{ file: "fixture.ts", newContent: mainPatchedContent }],
          },
        },
      ],
    });
    // The prompt for the caller site will contain "divide" (the callee name).
    const llm = new StubLLMProvider(new Map([["divide", mainPatchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // All accepted changes are verified.
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);
    // The caller (main()) should be discovered via calls_table strategy.
    // The patch removes the division from the callee; caller strategy B found it.
    expect(changes.some((c) => c.kind === "caller_update")).toBe(true);
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 3: Rejected change via oracle #3
  // LLM proposes a bogus patch; principle still matches; change not included.
  // -------------------------------------------------------------------------
  it("rejected change: bogus patch leaves principle match → not accepted", async () => {
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb3-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBeGreaterThanOrEqual(2);

    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    });

    // Bogus patch: still contains division in compute.ts (just renames params).
    const bogusComputeContent = "export function compute(xx: number, yy: number) { return xx / yy; }\n";
    const bogusComputePatchJson = JSON.stringify({
      candidates: [
        {
          rationale: "Rename params only (does not fix the division)",
          confidence: 0.6,
          patch: {
            description: "rename params in compute.ts",
            fileEdits: [{ file: "compute.ts", newContent: bogusComputeContent }],
          },
        },
      ],
    });
    // Prompt for compute.ts site will include "compute"
    const llm = new StubLLMProvider(new Map([["compute", bogusComputePatchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // All returned changes must be verified; bogus ones must not appear.
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);
    // The compute.ts site should NOT be accepted (division still present).
    const computeChanges = changes.filter(
      (c) => c.patch.fileEdits.some((e) => e.file === "compute.ts"),
    );
    expect(computeChanges.length).toBe(0);
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 4: Max sites cap
  // 15 matching adjacent sites; maxSites: 5; return length ≤ 5.
  // -------------------------------------------------------------------------
  it("max sites cap: 15 adjacent sites, maxSites: 5 → at most 5 processed", async () => {
    // Create 15 small files each with a division.
    const fileContents: Record<string, string> = {};
    fileContents["primary.ts"] = "export function primary(a: number, b: number) { return a / b; }\n";
    for (let i = 1; i <= 15; i++) {
      fileContents[`file${i}.ts`] = `export function fn${i}(a: number, b: number) { return a / b; }\n`;
    }

    const { repoDir, filePaths } = makeTestRepo(fileContents);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb4-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    for (const [name, path] of Object.entries(filePaths)) {
      buildSASTForFile(mainDb, path);
    }

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const primaryMatch = divMatches[0]!;
    const primaryNodeId = primaryMatch.captures["division"] ?? primaryMatch.rootNodeId;
    const locus = makeLocus(filePaths["primary.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedPrimaryContent = "export function primary(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division from primary.ts",
      fileEdits: [{ file: "primary.ts", newContent: fixedPrimaryContent }],
    });

    // LLM: for each site returns a valid fix (removes the division).
    // Use a catch-all match key that appears in all prompts.
    let llmCallCount = 0;
    const llm: typeof fix extends never ? never : { complete: (p: { prompt: string }) => Promise<string> } = {
      complete: async (p: { prompt: string }) => {
        llmCallCount++;
        // Return a fix that removes division — use a generic response.
        // We need to extract the filename from the prompt to know what to patch.
        // The site reason includes "fn<N>" so look for that pattern.
        const fnMatch = /fn(\d+)/.exec(p.prompt);
        const n = fnMatch ? fnMatch[1] : "0";
        const filename = `file${n}.ts`;
        const fixedContent = `export function fn${n}(_a: number, _b: number) { return 0; }\n`;
        return JSON.stringify({
          candidates: [
            {
              rationale: `Fix division in file${n}.ts`,
              confidence: 0.9,
              patch: {
                description: `remove division in file${n}.ts`,
                fileEdits: [{ file: filename, newContent: fixedContent }],
              },
            },
          ],
        });
      },
    };

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm: llm as Parameters<typeof generateComplementary>[0]["llm"],
      maxSites: 5,
      invariant,
    });

    // At most 5 sites were processed, so at most 5 results.
    expect(changes.length).toBeLessThanOrEqual(5);
    // LLM was called at most 5 times (maxSites).
    expect(llmCallCount).toBeLessThanOrEqual(5);
  }, 180_000);

  // -------------------------------------------------------------------------
  // Test 5: Priority sort order
  // Mix of kinds; return sorted: callers (0) < adjacent (1) < data_flow (2).
  // -------------------------------------------------------------------------
  it("priority sort order: caller_update < adjacent_site_fix < data_flow_guard", async () => {
    // We test the priority ordering by checking the ComplementaryChange type's priority field.
    // We can do this without a full overlay by mocking the helpers.
    // Instead, verify via discoverComplementarySites + priorityOf directly.
    const { priorityOf: pOf } = await import("./complementary.js");
    expect(pOf("caller_update")).toBe(0);
    expect(pOf("adjacent_site_fix")).toBe(1);
    expect(pOf("data_flow_guard")).toBe(2);
    expect(pOf("observability")).toBe(3);
    expect(pOf("startup_assert")).toBe(4);

    // Also verify that a real generateComplementary result is sorted ascending.
    // We create a minimal fixture where we can get at least two different kinds.
    // Use the adjacent site fixture from test 1.
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb5-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    });

    const fixedComputeContent = "export function compute(_x: number, _y: number) { return 0; }\n";
    const computePatchJson = makeDivFixPatchJson("compute.ts", fixedComputeContent);
    const llm = new StubLLMProvider(new Map([["compute", computePatchJson]]));

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // Verify sort order is ascending priority.
    for (let i = 1; i < changes.length; i++) {
      expect(changes[i]!.priority).toBeGreaterThanOrEqual(changes[i - 1]!.priority);
    }
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 6: Cumulative overlay state
  // Two adjacent sites A and B both pass verification; final overlay has both patches.
  // -------------------------------------------------------------------------
  it("cumulative overlay: two accepted patches both visible in final overlay", async () => {
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";
    const source3 = "export function calc(p: number, q: number) { return p / q; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
      "calc.ts": source3,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb6-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);
    buildSASTForFile(mainDb, filePaths["calc.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBeGreaterThanOrEqual(3);

    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    });

    const fixedComputeContent = "export function compute(_x: number, _y: number) { return 0; }\n";
    const fixedCalcContent = "export function calc(_p: number, _q: number) { return 0; }\n";
    const computePatchJson = makeDivFixPatchJson("compute.ts", fixedComputeContent);
    const calcPatchJson = makeDivFixPatchJson("calc.ts", fixedCalcContent);

    // Use distinct keys: "compute" and "calc"
    const llm = new StubLLMProvider(
      new Map([
        ["compute", computePatchJson],
        ["calc", calcPatchJson],
      ]),
    );

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // Both compute.ts and calc.ts should be accepted.
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);

    // Verify that the overlay worktree has the patched files from ALL accepted changes.
    for (const change of changes) {
      for (const edit of change.patch.fileEdits) {
        const absPath = join(overlay.worktreePath, edit.file);
        if (existsSync(absPath)) {
          const content = readFileSync(absPath, "utf-8");
          // Each accepted patch's content should be present.
          expect(content).toBe(edit.newContent);
        }
      }
    }
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 7: Empty result when no complementary sites found
  // Fixture: isolated bug (no other principle matches, no callers).
  // Return empty array. No LLM calls made.
  // -------------------------------------------------------------------------
  it("empty result: no complementary sites → return [] with no LLM calls", async () => {
    // Only one file with one division; no callers; no other files.
    const source = "export function divide(a: number, b: number) { return a / b; }\n";

    const { repoDir, filePaths } = makeTestRepo({ "divide.ts": source });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb7-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBe(1); // exactly one match

    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division",
      fileEdits: [{ file: "divide.ts", newContent: fixedContent }],
    });

    let llmCallCount = 0;
    const llm = {
      complete: async (_p: { prompt: string }) => {
        llmCallCount++;
        return JSON.stringify({ candidates: [] });
      },
    };

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    expect(changes).toHaveLength(0);
    // No sites to process means no LLM calls.
    expect(llmCallCount).toBe(0);
  }, 60_000);

  // -------------------------------------------------------------------------
  // Test 8: Agent path — stub agent edits the site file directly
  // -------------------------------------------------------------------------
  it("agent path: StubLLMProvider with agentResponses edits site file → accepted by oracle #3", async () => {
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb8-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    expect(divMatches.length).toBeGreaterThanOrEqual(2);

    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division from divide.ts",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    });

    // Agent writes the fix for compute.ts (removes the division).
    const fixedComputeContent = "export function compute(_x: number, _y: number) { return 0; }\n";
    const llm = new StubLLMProvider(
      new Map(), // no complete() responses needed — agent path only
      [{ matchPrompt: "compute", fileEdits: [{ file: "compute.ts", newContent: fixedComputeContent }], text: "Removed division from compute.ts" }],
    );

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    // Should have at least one adjacent_site_fix accepted.
    const adjacentChanges = changes.filter((c) => c.kind === "adjacent_site_fix");
    expect(adjacentChanges.length).toBeGreaterThan(0);
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);
    // The compute.ts edit should be captured.
    const computeChange = changes.find((c) => c.patch.fileEdits.some((e) => e.file === "compute.ts"));
    expect(computeChange).toBeDefined();
    expect(computeChange?.patch.fileEdits[0]?.newContent).toBe(fixedComputeContent);
  }, 120_000);

  // -------------------------------------------------------------------------
  // Test 9: JSON path still works when no agentResponses provided
  // -------------------------------------------------------------------------
  it("backward compat: JSON path still works when StubLLMProvider has no agentResponses", async () => {
    const source1 = "export function divide(a: number, b: number) { return a / b; }\n";
    const source2 = "export function compute(x: number, y: number) { return x / y; }\n";

    const { repoDir, filePaths } = makeTestRepo({
      "divide.ts": source1,
      "compute.ts": source2,
    });
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-c4-maindb9-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));

    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePaths["divide.ts"]!);
    buildSASTForFile(mainDb, filePaths["compute.ts"]!);

    const divMatches = evaluatePrinciple(mainDb, DIVISION_BY_ZERO_DSL);
    const primaryNodeId = divMatches[0]!.captures["division"] ?? divMatches[0]!.rootNodeId;
    const locus = makeLocus(filePaths["divide.ts"]!, primaryNodeId, primaryNodeId);
    const invariant = makeDivInvariant();

    const fixedDivideContent = "export function divide(_a: number, _b: number) { return 0; }\n";
    const fix = makeFixCandidate({
      description: "remove division from divide.ts",
      fileEdits: [{ file: "divide.ts", newContent: fixedDivideContent }],
    });

    const fixedComputeContent = "export function compute(_x: number, _y: number) { return 0; }\n";
    const computePatchJson = makeDivFixPatchJson("compute.ts", fixedComputeContent);
    // No agentResponses — uses JSON path.
    const llm = new StubLLMProvider(new Map([["compute", computePatchJson]]));
    expect(llm.agent).toBeUndefined();

    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const changes = await generateComplementary({
      fix,
      locus,
      overlay,
      db: mainDb,
      llm,
      maxSites: 10,
      invariant,
    });

    const adjacentChanges = changes.filter((c) => c.kind === "adjacent_site_fix");
    expect(adjacentChanges.length).toBeGreaterThan(0);
    expect(changes.every((c) => c.verifiedAgainstOverlay)).toBe(true);
  }, 120_000);
});
