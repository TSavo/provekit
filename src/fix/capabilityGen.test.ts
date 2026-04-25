/**
 * C6: capabilityGen.ts tests
 *
 * Tests oracle #14 (migration safety), oracle #16 (extractor coverage),
 * oracle #17 (substrate consistency), and oracle #18 (principle-needs-capability).
 * All tests are synchronous or use stub LLMs — no real SAST builds needed here.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import {
  runOracle14,
  runOracle16,
  runOracle16Structural,
  runOracle17,
  proposeCapabilitySpec,
} from "./capabilityGen.js";
import {
  listCapabilities,
  registerCapability,
  unregisterCapability,
} from "../sast/capabilityRegistry.js";
import { generatePrincipleCandidate } from "./stages/generatePrincipleCandidate.js";
import { StubLLMProvider } from "./types.js";
import type { BugSignal, InvariantClaim, FixCandidate, CapabilitySpec, OverlayHandle } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// Registry snapshot/restore
// ---------------------------------------------------------------------------

let savedCapabilities: ReturnType<typeof listCapabilities>;

function snapshotRegistry(): void {
  savedCapabilities = [...listCapabilities()];
}

function restoreRegistry(): void {
  const current = listCapabilities();
  for (const cap of current) {
    if (!savedCapabilities.find((s) => s.dslName === cap.dslName)) {
      unregisterCapability(cap.dslName);
    }
  }
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

function makeSignal(): BugSignal {
  return {
    source: "test",
    rawText: "divide by zero",
    summary: "Division by zero",
    failureDescription: "denominator can be zero",
    codeReferences: [],
    bugClassHint: "arithmetic",
  };
}

function makeInvariant(): InvariantClaim {
  return {
    principleId: null,
    description: "denominator must not be zero",
    formalExpression: "(declare-const d Int)\n(assert (= d 0))\n(check-sat)",
    bindings: [],
    complexity: 1,
    witness: "d = 0",
  };
}

function makeFixCandidate(): FixCandidate {
  return {
    patch: {
      description: "Add guard for zero denominator",
      fileEdits: [{ file: "src/math.ts", newContent: "if (d !== 0) return a / d;" }],
    },
    llmRationale: "Guards against zero",
    llmConfidence: 0.9,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 1,
      overlayClosed: false,
    },
  };
}

function makeDb(): any {
  return {
    select: () => ({
      from: () => ({
        where: () => ({ get: () => null, all: () => [] }),
        get: () => null,
        all: () => [],
      }),
    }),
    $client: { prepare: () => ({ all: () => [] }) },
  };
}

function makeCapabilitySpec(overrides: Partial<CapabilitySpec> = {}): CapabilitySpec {
  return {
    capabilityName: "myCapability",
    schemaTs: `import { sqliteTable, text } from "drizzle-orm/sqlite-core";
export const nodeMyCapability = sqliteTable("node_my_capability", {
  nodeId: text("node_id").notNull(),
  myColumn: text("my_column").notNull(),
});`,
    migrationSql: "CREATE TABLE node_my_capability (node_id TEXT NOT NULL, my_column TEXT NOT NULL);",
    extractorTs: `export function extractMyCapability(tx: any, fileId: number): void {
  tx.insert(nodeMyCapability).values({ nodeId: "x", myColumn: "y" });
}`,
    extractorTestsTs: "import { describe, it, expect } from 'vitest';\ndescribe('x', () => { it('y', () => { expect(1).toBe(1); }); });",
    registryRegistration: "registerCapability({ dslName: 'myCapability' });",
    positiveFixtures: [{ source: "function bad() { return 1; }", expectedRowCount: 1 }],
    negativeFixtures: [{ source: "function ok() { return 2; }", expectedRowCount: 0 }],
    rationale: "Tracks bad patterns",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Oracle #14: migration safety
// ---------------------------------------------------------------------------

describe("oracle #14 — migration safety", () => {
  it("accepts CREATE TABLE", () => {
    const result = runOracle14("CREATE TABLE foo (id INTEGER PRIMARY KEY)");
    expect(result.passed).toBe(true);
  });

  it("accepts ALTER TABLE ADD COLUMN", () => {
    const result = runOracle14("ALTER TABLE foo ADD COLUMN bar TEXT");
    expect(result.passed).toBe(true);
  });

  it("accepts multiple safe statements", () => {
    const sql = [
      "CREATE TABLE foo (id INTEGER PRIMARY KEY)",
      "ALTER TABLE foo ADD COLUMN bar TEXT",
    ].join(";");
    const result = runOracle14(sql);
    expect(result.passed).toBe(true);
  });

  it("rejects DROP TABLE", () => {
    const result = runOracle14("DROP TABLE foo");
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #14");
    expect(result.reason).toContain("DROP");
  });

  it("rejects ALTER TABLE without ADD COLUMN", () => {
    const result = runOracle14("ALTER TABLE foo DROP COLUMN bar");
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #14");
  });

  it("rejects CREATE INDEX (not allowed)", () => {
    const result = runOracle14("CREATE INDEX idx ON foo (bar)");
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #14");
  });

  it("strips line comments before parsing statements", () => {
    const sql = [
      "-- Capability: taintSource",
      "-- Adds the taint_source table for tracking nodes that carry untrusted data.",
      "",
      "CREATE TABLE taint_source (node_id TEXT PRIMARY KEY)",
    ].join("\n");
    const result = runOracle14(sql);
    expect(result.passed).toBe(true);
  });

  it("strips block comments before parsing statements", () => {
    const sql = "/* multi\nline\nblock */\nCREATE TABLE foo (id INTEGER PRIMARY KEY)";
    const result = runOracle14(sql);
    expect(result.passed).toBe(true);
  });

  it("still rejects DROP even when preceded by comments", () => {
    const sql = "-- pretend this is fine\nDROP TABLE foo";
    const result = runOracle14(sql);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("DROP");
  });
});

// ---------------------------------------------------------------------------
// Oracle #16: extractor coverage
// ---------------------------------------------------------------------------

describe("oracle #16 — extractor coverage (structural pre-check)", () => {
  it("accepts valid extractor with exported function + insert", () => {
    const extractorTs = `
export function extractFoo(tx: any, sourceFile: any, nodeIdByNode: any): void {
  tx.insert(nodeFoo).values({ nodeId: "x", foo: "y" });
}`;
    const result = runOracle16Structural(extractorTs);
    expect(result.passed).toBe(true);
  });

  it("rejects extractor with no exported function", () => {
    const extractorTs = `
function extractFoo(tx: any): void {
  tx.insert(nodeFoo).values({ nodeId: "x" });
}`;
    const result = runOracle16Structural(extractorTs);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #16");
    expect(result.reason).toContain("export");
  });

  it("rejects extractor without tx.insert().values() pattern", () => {
    const extractorTs = `
export function extractFoo(tx: any, sourceFile: any, nodeIdByNode: any): void {
  const rows = tx.select().from(nodeFoo).all();
}`;
    const result = runOracle16Structural(extractorTs);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #16");
    expect(result.reason).toContain("insert");
  });
});

describe("oracle #16 — full execution (via runOracle16)", () => {
  it("accepts spec with realistic extractor + fixtures", async () => {
    // Extractor: detect BinaryExpression nodes, insert into node_test_binary_cap.
    // The TS source gets transpiled to CJS by the executor — imports become requires.
    const spec = makeCapabilitySpec({
      capabilityName: "testBinaryCap",
      migrationSql: "CREATE TABLE node_test_binary_cap (node_id TEXT NOT NULL)",
      extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
import { SyntaxKind } from "ts-morph";
const nodeTestBinaryCap = sqliteTable("node_test_binary_cap", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryCap(tx: any, sourceFile: any, nodeIdByNode: any): void {
  sourceFile.forEachDescendant((node: any) => {
    if (node.getKind() === SyntaxKind.BinaryExpression) {
      const nid = nodeIdByNode.get(node);
      if (nid) tx.insert(nodeTestBinaryCap).values({ nodeId: nid }).run();
    }
  });
}`,
      positiveFixtures: [
        { source: "const z = 1 + 2;", expectedRowCount: 1 },
      ],
      negativeFixtures: [
        { source: "const z = 1;", expectedRowCount: 0 },
      ],
    });

    const result = await runOracle16(spec);
    expect(result.passed).toBe(true);
  }, 30000);
});

// ---------------------------------------------------------------------------
// Oracle #17: substrate consistency
// ---------------------------------------------------------------------------

describe("oracle #17 — substrate consistency", () => {
  it("accepts schema with no FK references", () => {
    const schemaTs = `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
export const nodeMyCapability = sqliteTable("node_my_capability", {
  nodeId: text("node_id").notNull(),
  myColumn: text("my_column").notNull(),
});`;
    const result = runOracle17(schemaTs, []);
    expect(result.passed).toBe(true);
  });

  it("accepts schema with FK to nodes (valid)", () => {
    const schemaTs = `
export const nodeMyCapability = sqliteTable("node_my_capability", {
  nodeId: text("node_id").notNull(),
}, (t) => ({
  fk: foreignKey({ columns: [t.nodeId], foreignColumns: [nodes.id] })
}));`;
    const result = runOracle17(schemaTs, []);
    expect(result.passed).toBe(true);
  });

  it("rejects FK to non-existent table variable", () => {
    const schemaTs = `
export const nodeMyCapability = sqliteTable("node_my_capability", {
  nodeId: text("node_id").notNull(),
}, (t) => ({
  fk: foreignKey({ columns: [t.nodeId], foreignColumns: [nonExistentTable.id] })
}));`;
    const result = runOracle17(schemaTs, []);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #17");
    expect(result.reason).toContain("nonExistentTable");
  });
});

// ---------------------------------------------------------------------------
// Oracle #18 via generatePrincipleCandidate: gratuitous capability detection
// ---------------------------------------------------------------------------

describe("oracle #18 — principle-needs-capability (via generatePrincipleCandidate)", () => {
  beforeEach(snapshotRegistry);
  afterEach(restoreRegistry);

  it("returns null when principle compiles without the proposed capability (gratuitous)", async () => {
    // Register the "arithmetic" capability — the DSL principle below uses it.
    registerCapability({
      dslName: "arithmetic",
      table: { _: { name: "node_arithmetic" } } as any,
      columns: {
        node_id: {
          dslName: "node_id",
          drizzleColumn: { name: "node_id" },
          isNodeRef: true,
          nullable: false,
        },
        op: {
          dslName: "op",
          drizzleColumn: { name: "op" },
          isNodeRef: false,
          nullable: false,
          kindEnum: ["+", "-", "*", "/"],
        },
      },
    });

    // A DSL that uses the ALREADY-registered "arithmetic" capability.
    // The LLM proposes a new "myCapability" but the DSL doesn't use it.
    const dslUsingExistingCap = `principle TestP {
  match $x: node where arithmetic.op == "/"
  report violation {
    at $x
    captures { site: $x }
    message "div"
  }
}`;

    const capSpec = makeCapabilitySpec({ capabilityName: "myCapability" });

    // Build the full JSON response as the LLM would return it.
    const llmResponse = JSON.stringify({
      capabilityName: capSpec.capabilityName,
      schemaTs: capSpec.schemaTs,
      migrationSql: capSpec.migrationSql,
      extractorTs: capSpec.extractorTs,
      extractorTestsTs: capSpec.extractorTestsTs,
      registryRegistration: capSpec.registryRegistration,
      positiveFixtures: capSpec.positiveFixtures,
      negativeFixtures: capSpec.negativeFixtures,
      rationale: capSpec.rationale,
      dslSource: dslUsingExistingCap,
      name: "TestP",
      smtTemplate: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
      teachingExample: { domain: "arithmetic", explanation: "test", smt2: "(check-sat)" },
    });

    // First LLM call (needs_capability) + second call (the capabilitySpec proposal).
    const llm = new StubLLMProvider(
      new Map([
        // tryExistingCapabilities gets a "needs_capability" response.
        ["denominator", JSON.stringify({ kind: "needs_capability", missing_predicate: "myCapability" })],
        // proposeCapabilitySpec gets the full spec.
        ["Missing predicate", llmResponse],
      ]),
    );

    const result = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });

    // Should return null because the principle compiles WITHOUT myCapability (gratuitous).
    expect(result).toBeNull();
  });

  it("live registry not mutated after substrate path returns null", async () => {
    const before = listCapabilities().map((c) => c.dslName).sort();

    // LLM returns needs_capability, then a malformed spec.
    const llm = new StubLLMProvider(
      new Map([
        ["denominator", JSON.stringify({ kind: "needs_capability", missing_predicate: "x" })],
        ["Missing predicate", "NOT VALID JSON"],
      ]),
    );

    const result = await generatePrincipleCandidate({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      db: makeDb(),
      llm,
    });

    expect(result).toBeNull();

    const after = listCapabilities().map((c) => c.dslName).sort();
    expect(after).toEqual(before);
  });
});

// ---------------------------------------------------------------------------
// C6 agent path: proposeCapabilitySpec via StubLLMProvider with agentResponses
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

function makeMinimalOverlay(worktreePath: string): OverlayHandle {
  return {
    worktreePath,
    sastDbPath: join(worktreePath, ".provekit", "scratch.db"),
    sastDb: {} as unknown as Db,
    baseRef: "HEAD",
    modifiedFiles: new Set<string>(),
    closed: false,
  };
}

describe("C6: proposeCapabilitySpec — agent path", () => {
  it("agent path: StubLLMProvider with agentResponses writes meta.json + all files → proposal returned", async () => {
    const repoDir = mkdtempSync(join(tmpdir(), "provekit-c6-agent-"));
    try {
      // Init a git repo (runAgentInOverlay needs a git repo in worktree).
      execFileSync("git", [...GIT_ID, "init", repoDir]);
      writeFileSync(join(repoDir, "README.md"), "hello\n", "utf8");
      execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
      execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

      const overlay = makeMinimalOverlay(repoDir);

      // The agent writes all required files.
      const capabilityName = "divisionCap";
      const capDir = `.provekit/capability-proposal/${capabilityName}`;
      const schemaTs = `import { sqliteTable, text } from "drizzle-orm/sqlite-core";
export const nodeDivisionCap = sqliteTable("node_division_cap", {
  nodeId: text("node_id").notNull(),
});`;
      const migrationSql = "CREATE TABLE node_division_cap (node_id TEXT NOT NULL)";
      const extractorTs = `export function extractDivisionCap(tx: any, fileId: number): void {
  tx.insert(nodeDivisionCap).values({ nodeId: "x" });
}`;
      const extractorTestsTs = "import { it, expect } from 'vitest';\nit('works', () => { expect(true).toBe(true); });";
      const registryTs = "registerCapability({ dslName: 'divisionCap' });";
      const fixtures = JSON.stringify({
        positiveFixtures: [{ source: "function bad() { return 1 / 0; }", expectedRowCount: 1 }],
        negativeFixtures: [{ source: "function ok() { return 1; }", expectedRowCount: 0 }],
      });
      const dslSource = `principle DivisionPrinciple {
  match $x: node where divisionCap.node_id == "x"
  report violation {
    at $x
    captures { site: $x }
    message "division by zero"
  }
}`;
      const meta = JSON.stringify({
        capabilityName,
        rationale: "Tracks division nodes",
        dslSource,
        name: "DivisionPrinciple",
        smtTemplate: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
        teachingExample: { domain: "arithmetic", explanation: "test", smt2: "(check-sat)" },
      });

      const llm = new StubLLMProvider(
        new Map(), // no complete() responses needed
        [{
          matchPrompt: "capability",
          fileEdits: [
            { file: `${capDir}/schema.ts`, newContent: schemaTs },
            { file: `${capDir}/migration.sql`, newContent: migrationSql },
            { file: `${capDir}/extractor.ts`, newContent: extractorTs },
            { file: `${capDir}/extractor.test.ts`, newContent: extractorTestsTs },
            { file: `${capDir}/registry.ts`, newContent: registryTs },
            { file: `${capDir}/fixtures.json`, newContent: fixtures },
            { file: `${capDir}/meta.json`, newContent: meta },
            { file: `.provekit/principles/DivisionPrinciple.dsl`, newContent: dslSource },
          ],
          text: "Wrote capability proposal",
        }],
      );
      expect(llm.agent).toBeDefined();

      const proposal = await proposeCapabilitySpec({
        signal: makeSignal(),
        invariant: makeInvariant(),
        fixCandidate: makeFixCandidate(),
        gap: "missing division detector",
        llm,
        overlay,
      });

      expect(proposal).not.toBeNull();
      expect(proposal!.capabilitySpec.capabilityName).toBe(capabilityName);
      expect(proposal!.capabilitySpec.schemaTs).toBe(schemaTs);
      expect(proposal!.capabilitySpec.migrationSql).toBe(migrationSql);
      expect(proposal!.capabilitySpec.extractorTs).toBe(extractorTs);
      expect(proposal!.dslSource).toBe(dslSource);
      expect(proposal!.name).toBe("DivisionPrinciple");
      expect(proposal!.capabilitySpec.positiveFixtures).toHaveLength(1);
      expect(proposal!.capabilitySpec.negativeFixtures).toHaveLength(1);
    } finally {
      rmSync(repoDir, { recursive: true, force: true });
    }
  }, 30_000);

  it("backward compat: JSON path used when LLM has no agent()", async () => {
    const capabilityName = "myCapability";
    const capSpec = makeCapabilitySpec({ capabilityName });
    const llmResponse = JSON.stringify({
      capabilityName: capSpec.capabilityName,
      schemaTs: capSpec.schemaTs,
      migrationSql: capSpec.migrationSql,
      extractorTs: capSpec.extractorTs,
      extractorTestsTs: capSpec.extractorTestsTs,
      registryRegistration: capSpec.registryRegistration,
      positiveFixtures: capSpec.positiveFixtures,
      negativeFixtures: capSpec.negativeFixtures,
      rationale: capSpec.rationale,
      dslSource: `principle TestP { match $x: node where myCapability.node_id == "x" report violation { at $x captures { site: $x } message "test" } }`,
      name: "TestP",
      smtTemplate: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
      teachingExample: { domain: "test", explanation: "test", smt2: "(check-sat)" },
    });

    // No agentResponses → no agent() → JSON path.
    const llm = new StubLLMProvider(new Map([["Missing predicate", llmResponse]]));
    expect(llm.agent).toBeUndefined();

    const proposal = await proposeCapabilitySpec({
      signal: makeSignal(),
      invariant: makeInvariant(),
      fixCandidate: makeFixCandidate(),
      gap: "Missing predicate",
      llm,
      // No overlay → JSON path even if agent existed.
    });

    expect(proposal).not.toBeNull();
    expect(proposal!.capabilitySpec.capabilityName).toBe(capabilityName);
  });
});
