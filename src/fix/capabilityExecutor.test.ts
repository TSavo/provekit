/**
 * Tests for capabilityExecutor.ts (oracle #16 full execution).
 *
 * Each test uses a synthetic CapabilitySpec with a trivial extractor and
 * simple TypeScript fixtures. The extractor is transpiled + run against a
 * scratch SQLite DB.
 */

import { describe, it, expect } from "vitest";
import { executeExtractorSpec } from "./capabilityExecutor.js";
import { existsSync, readdirSync, mkdirSync } from "fs";
import { join } from "path";
import { fileURLToPath } from "url";
import { dirname } from "path";
import type { CapabilitySpec } from "./types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const PROJECT_ROOT = join(__dirname, "..", "..");
const CACHE_DIR = join(PROJECT_ROOT, "node_modules", ".cache");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Create a minimal CapabilitySpec for testing. */
function makeSpec(overrides: Partial<CapabilitySpec> = {}): CapabilitySpec {
  return {
    capabilityName: "testBinaryExpr",
    schemaTs: ``,
    migrationSql: "CREATE TABLE node_test_binary_expr (node_id TEXT NOT NULL)",
    extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
import { SyntaxKind } from "ts-morph";
const nodeTestBinaryExpr = sqliteTable("node_test_binary_expr", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryExpr(tx: any, sourceFile: any, nodeIdByNode: any): void {
  sourceFile.forEachDescendant((node: any) => {
    if (node.getKind() === SyntaxKind.BinaryExpression) {
      const nid = nodeIdByNode.get(node);
      if (nid) tx.insert(nodeTestBinaryExpr).values({ nodeId: nid }).run();
    }
  });
}`,
    extractorTestsTs: "",
    registryRegistration: "",
    positiveFixtures: [
      { source: "const z = 1 + 2;", expectedRowCount: 1 },
    ],
    negativeFixtures: [
      { source: "const z = 1;", expectedRowCount: 0 },
    ],
    rationale: "Test",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Test 1: valid extractor, positive fixture emits rows, negative emits 0
// ---------------------------------------------------------------------------

describe("capabilityExecutor — valid extractor + fixtures", () => {
  it("positive fixture emits >=1 row, negative emits 0 → passed: true", async () => {
    const spec = makeSpec();
    const result = await executeExtractorSpec(spec);
    expect(result.passed).toBe(true);
    expect(result.detail).toContain("positive fixtures: 1/1");
    expect(result.detail).toContain("negative fixtures: 1/1");
  }, 30000);
});

// ---------------------------------------------------------------------------
// Test 2: extractor that throws at runtime
// ---------------------------------------------------------------------------

describe("capabilityExecutor — extractor that throws", () => {
  it("runtime error → passed: false with error message", async () => {
    const spec = makeSpec({
      extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
const nodeTestBinaryExpr = sqliteTable("node_test_binary_expr", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryExpr(tx: any, sourceFile: any, nodeIdByNode: any): void {
  throw new Error("deliberate runtime failure");
}`,
    });
    const result = await executeExtractorSpec(spec);
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("deliberate runtime failure");
  }, 30000);
});

// ---------------------------------------------------------------------------
// Test 3: positive fixture emits zero rows (extractor doesn't match)
// ---------------------------------------------------------------------------

describe("capabilityExecutor — positive fixture emits zero rows", () => {
  it("extractor too narrow → passed: false", async () => {
    // Extractor looks for CallExpression, but fixture has only a BinaryExpression.
    // Positive fixture: "const z = 1 + 2;" — no call expressions.
    const spec = makeSpec({
      extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
import { SyntaxKind } from "ts-morph";
const nodeTestBinaryExpr = sqliteTable("node_test_binary_expr", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryExpr(tx: any, sourceFile: any, nodeIdByNode: any): void {
  sourceFile.forEachDescendant((node: any) => {
    if (node.getKind() === SyntaxKind.CallExpression) {
      const nid = nodeIdByNode.get(node);
      if (nid) tx.insert(nodeTestBinaryExpr).values({ nodeId: nid }).run();
    }
  });
}`,
      // Positive fixture has no call expressions
      positiveFixtures: [{ source: "const z = 1 + 2;", expectedRowCount: 1 }],
    });
    const result = await executeExtractorSpec(spec);
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("positive fixtures: 0/1");
  }, 30000);
});

// ---------------------------------------------------------------------------
// Test 4: negative fixture emits rows (extractor too permissive)
// ---------------------------------------------------------------------------

describe("capabilityExecutor — negative fixture emits rows", () => {
  it("extractor matches negative fixture → passed: false", async () => {
    // Extractor emits a row for EVERY node (wildcard) — will match negative fixture too.
    const spec = makeSpec({
      extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
const nodeTestBinaryExpr = sqliteTable("node_test_binary_expr", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryExpr(tx: any, sourceFile: any, nodeIdByNode: any): void {
  // Emit row for the source file node itself — always present
  const nid = nodeIdByNode.get(sourceFile);
  if (nid) tx.insert(nodeTestBinaryExpr).values({ nodeId: nid }).run();
}`,
      // Negative fixture: should emit 0 rows, but wildcard extractor emits 1
      negativeFixtures: [{ source: "const z = 1;", expectedRowCount: 0 }],
    });
    const result = await executeExtractorSpec(spec);
    expect(result.passed).toBe(false);
    expect(result.detail).toContain("negative fixtures: 0/1");
  }, 30000);
});

// ---------------------------------------------------------------------------
// Test 5: transpile failure (TypeScript syntax error)
// ---------------------------------------------------------------------------

describe("capabilityExecutor — transpile failure", () => {
  it("syntax error in extractorTs → passed: false with transpile error detail", async () => {
    // ts.transpileModule is very permissive — it doesn't type-check.
    // A genuine syntax error (unclosed brace) will cause transpile to fail OR
    // produce broken JS that fails at import time.
    const spec = makeSpec({
      extractorTs: `
export function extractTestBinaryExpr(tx: any) {
  // Missing closing brace — broken JS after transpile
  tx.insert(nodeTestBinaryExpr).values({ nodeId: "x"
`,
      // We need to keep migrationSql for a real table
      migrationSql: "CREATE TABLE node_test_binary_expr (node_id TEXT NOT NULL)",
    });

    // ts.transpileModule is lenient — it may produce broken JS.
    // Either transpile errors OR dynamic import fails.
    const result = await executeExtractorSpec(spec);
    expect(result.passed).toBe(false);
  }, 30000);
});

// ---------------------------------------------------------------------------
// Test 6: structural pre-check still rejects missing tx.insert
// (This test goes via executeExtractorSpec which delegates to runOracle16 in capabilityGen.
//  Here we test the executor directly with a structurally-valid but insert-missing extractor.
//  The executor does NOT do the structural pre-check — that's capabilityGen's job.)
// Test 7: tmpfiles cleaned up after execution
// ---------------------------------------------------------------------------

describe("capabilityExecutor — tmpfile cleanup", () => {
  it("tmpdir is removed after successful execution", async () => {
    // Count provekit-extractor-* dirs before and after; none should remain.
    mkdirSync(CACHE_DIR, { recursive: true });
    const before = readdirSync(CACHE_DIR).filter((d) => d.startsWith("provekit-extractor-")).length;

    await executeExtractorSpec(makeSpec());

    const after = readdirSync(CACHE_DIR).filter((d) => d.startsWith("provekit-extractor-")).length;

    // No new orphaned tmpdir should remain
    expect(after).toBe(before);
  }, 30000);

  it("tmpdir is removed after failed execution", async () => {
    mkdirSync(CACHE_DIR, { recursive: true });
    const before = readdirSync(CACHE_DIR).filter((d) => d.startsWith("provekit-extractor-")).length;

    // Run a spec that will fail at execution time
    await executeExtractorSpec(
      makeSpec({
        extractorTs: `
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
const nodeTestBinaryExpr = sqliteTable("node_test_binary_expr", {
  nodeId: text("node_id").notNull(),
});
export function extractTestBinaryExpr(tx: any, sourceFile: any, nodeIdByNode: any): void {
  throw new Error("cleanup test failure");
}`,
      }),
    );

    const after = readdirSync(CACHE_DIR).filter((d) => d.startsWith("provekit-extractor-")).length;

    expect(after).toBe(before);
  }, 30000);
});
