/**
 * Unified verifier tests.
 *
 * The verifier is a thin wrapper around verifyAllCached; the heavy
 * lifting (path enumeration, Z3, binding resolution) is tested in
 * src/fix/runtime/. These tests verify the wrapper's structured
 * report shape, file/line filtering, and the LSP-friendly helpers.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { verifyProject, rowsForFile, rowsAtLine, type ValidityReport } from "./index.js";

function makeProjectWithoutInvariants(): string {
  const root = mkdtempSync(join(tmpdir(), "verifier-test-"));
  mkdirSync(join(root, ".provekit", "invariants"), { recursive: true });
  return root;
}

function makeMockReport(): ValidityReport {
  return {
    projectRoot: "/fake",
    rows: [
      {
        invariantId: "abc",
        status: "holds",
        locus: { filePath: "/fake/src/auth.ts", function: "validate", startLine: 10, endLine: 25 },
        intent: "validate never returns null on valid input",
        fromCache: false,
      },
      {
        invariantId: "def",
        status: "violated",
        locus: { filePath: "/fake/src/auth.ts", function: "signup", startLine: 30, endLine: 50 },
        intent: "signup preserves balance",
        witness: { token: "0", expiry: -1 },
        reason: "Z3 found a counterexample",
        fromCache: false,
      },
      {
        invariantId: "ghi",
        status: "decayed",
        locus: { filePath: "/fake/src/checkout.ts", function: "compute", startLine: 5, endLine: 12 },
        intent: "compute returns non-negative",
        reason: "callsite no longer resolves in substrate",
        fromCache: true,
      },
    ],
    summary: {
      total: 3,
      holds: 1,
      decayed: 1,
      violated: 1,
      unresolved: 0,
      undecidable: 0,
      cacheHits: 1,
      cacheMisses: 2,
    },
    registry: { extensionCount: 0, bridgeCount: 13 },
    verifiedAt: new Date().toISOString(),
  };
}

describe("verifyProject", () => {
  it("returns an empty report when no invariants exist in the project", async () => {
    const root = makeProjectWithoutInvariants();
    try {
      const report = await verifyProject(root, { timeoutMs: 1000 });
      expect(report.rows).toEqual([]);
      expect(report.summary.total).toBe(0);
      expect(report.projectRoot).toBe(root);
      expect(report.registry.bridgeCount).toBeGreaterThanOrEqual(0);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("includes a verifiedAt ISO timestamp", async () => {
    const root = makeProjectWithoutInvariants();
    try {
      const report = await verifyProject(root, { timeoutMs: 1000 });
      expect(report.verifiedAt).toMatch(/\d{4}-\d{2}-\d{2}T/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("rowsForFile", () => {
  it("filters rows down to one specific file", () => {
    const report = makeMockReport();
    const authRows = rowsForFile(report, "/fake/src/auth.ts");
    expect(authRows).toHaveLength(2);
    expect(authRows.map((r) => r.invariantId).sort()).toEqual(["abc", "def"]);
  });

  it("returns an empty array when no rows match", () => {
    const report = makeMockReport();
    expect(rowsForFile(report, "/fake/src/missing.ts")).toEqual([]);
  });
});

describe("rowsAtLine", () => {
  it("returns rows whose locus contains the line", () => {
    const report = makeMockReport();
    expect(rowsAtLine(report, "/fake/src/auth.ts", 15).map((r) => r.invariantId)).toEqual(["abc"]);
    expect(rowsAtLine(report, "/fake/src/auth.ts", 35).map((r) => r.invariantId)).toEqual(["def"]);
  });

  it("returns empty when the line is outside any invariant's locus", () => {
    const report = makeMockReport();
    expect(rowsAtLine(report, "/fake/src/auth.ts", 100)).toEqual([]);
  });

  it("returns multiple rows when overlapping invariants cover the same line", () => {
    const report = makeMockReport();
    // Add an overlapping row
    report.rows.push({
      invariantId: "overlap",
      status: "holds",
      locus: { filePath: "/fake/src/auth.ts", function: null, startLine: 1, endLine: 100 },
      intent: "module-wide invariant",
      fromCache: false,
    });
    const at15 = rowsAtLine(report, "/fake/src/auth.ts", 15);
    expect(at15.map((r) => r.invariantId).sort()).toEqual(["abc", "overlap"]);
  });
});
