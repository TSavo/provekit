/**
 * C6: capabilityGen.ts tests
 *
 * Tests oracle #14 (migration safety), oracle #16 (extractor coverage),
 * oracle #17 (substrate consistency), and oracle #18 (principle-needs-capability).
 * All tests are synchronous or use stub LLMs — no real SAST builds needed here.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  runOracle14,
  runOracle16,
  runOracle17,
} from "./capabilityGen.js";
import {
  listCapabilities,
  registerCapability,
  unregisterCapability,
} from "../sast/capabilityRegistry.js";
import { generatePrincipleCandidate } from "./stages/generatePrincipleCandidate.js";
import { StubLLMProvider } from "./types.js";
import type { BugSignal, InvariantClaim, FixCandidate, CapabilitySpec } from "./types.js";

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
});

// ---------------------------------------------------------------------------
// Oracle #16: extractor coverage
// ---------------------------------------------------------------------------

describe("oracle #16 — extractor coverage (structural)", () => {
  it("accepts valid extractor with exported function + insert", () => {
    const extractorTs = `
export function extractFoo(tx: any, fileId: number): void {
  tx.insert(nodeFoo).values({ nodeId: "x", foo: "y" });
}`;
    const result = runOracle16(extractorTs);
    expect(result.passed).toBe(true);
  });

  it("rejects extractor with no exported function", () => {
    const extractorTs = `
function extractFoo(tx: any): void {
  tx.insert(nodeFoo).values({ nodeId: "x" });
}`;
    const result = runOracle16(extractorTs);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #16");
    expect(result.reason).toContain("export");
  });

  it("rejects extractor without tx.insert().values() pattern", () => {
    const extractorTs = `
export function extractFoo(tx: any, fileId: number): void {
  const rows = tx.select().from(nodeFoo).all();
}`;
    const result = runOracle16(extractorTs);
    expect(result.passed).toBe(false);
    expect(result.reason).toContain("Oracle #16");
    expect(result.reason).toContain("insert");
  });
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
