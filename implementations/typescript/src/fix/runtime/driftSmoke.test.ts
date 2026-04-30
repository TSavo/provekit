/**
 * Standing-runtime drift detection: empirical smoke test.
 *
 * Validates that resolveBindings + verifyAll fire correctly when an
 * observation's bound source span is mutated. This is the "step 4
 * empirical proof" that gates dogfooding — without it, we'd populate
 * .provekit/invariants/ with observations the verifier might not actually
 * decay.
 *
 * No LLM cost. No corpus pollution. Pure mechanical proof.
 *
 * Test plan:
 *   1. Create a tmp project with a known source file.
 *   2. Hand-craft a StoredInvariant binding to that file's content (with
 *      the correct nodeHash recorded at write time).
 *   3. Persist via writeInvariant.
 *   4. Run verifyAll → expect status "holds" (path-check skipped because
 *      no substrate, but bindings resolve cleanly).
 *   5. Mutate the source span.
 *   6. Run verifyAll → expect status "decayed" with decayKind: "changed"
 *      and a "node hash mismatch" reason.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { createHash } from "crypto";
import { writeInvariant, type StoredInvariant } from "./invariantStore.js";
import { verifyAll } from "./verify.js";

let tmpRoot: string;

beforeEach(() => {
  tmpRoot = mkdtempSync(join(tmpdir(), "drift-smoke-"));
  mkdirSync(join(tmpRoot, "src"), { recursive: true });
});

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function makeInvariantBoundTo(
  filePath: string,
  startLine: number,
  endLine: number,
  bytesAtMintTime: string,
): StoredInvariant {
  return {
    id: "test-inv-1",
    createdAt: new Date().toISOString(),
    originatingBug: "synthetic drift smoke test",
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const k Int)"],
      assertion: "(assert (> k 0))",
    },
    bindings: [
      {
        smt_constant: "k",
        source_expr: "k",
        sort: "Int",
        node: {
          filePath,
          nodeHash: hash16(bytesAtMintTime),
          startLine,
          endLine,
        },
      },
    ],
    callsite: {
      filePath,
      function: null,
      startLine,
      endLine,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

describe("standing-runtime drift detection (step 4 empirical)", () => {
  it("reports holds when bound source is unchanged", async () => {
    const file = "src/example.ts";
    const content = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(tmpRoot, file), content);

    const lines = content.split("\n");
    const span = lines.slice(0, 3).join("\n"); // lines 1-3 inclusive
    const inv = makeInvariantBoundTo(file, 1, 3, span);
    writeInvariant(tmpRoot, inv);

    const report = await verifyAll(tmpRoot);

    expect(report.verdicts).toHaveLength(1);
    const v = report.verdicts[0];
    expect(v.status).toBe("holds");
    expect(v.bindings.every((b) => b.status === "resolved")).toBe(true);
    // No substrate built → path-check is skipped, status stays "holds".
    expect(v.pathCheck).toBe("skipped");
  });

  it("reports decayed:changed when bound source bytes mutate", async () => {
    const file = "src/example.ts";
    const original = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(tmpRoot, file), original);

    const lines = original.split("\n");
    const span = lines.slice(0, 3).join("\n");
    const inv = makeInvariantBoundTo(file, 1, 3, span);
    writeInvariant(tmpRoot, inv);

    // Mutate the bound span — change the comparison.
    const mutated = "function f(k: number) {\n  return k >= 0;\n}\n";
    writeFileSync(join(tmpRoot, file), mutated);

    const report = await verifyAll(tmpRoot);

    expect(report.verdicts).toHaveLength(1);
    const v = report.verdicts[0];
    expect(v.status).toBe("decayed");
    expect(v.bindings).toHaveLength(1);
    const b = v.bindings[0];
    expect(b.status).toBe("decayed");
    expect(b.decayKind).toBe("changed");
    expect(b.reason).toMatch(/node hash mismatch/);
  });

  it("reports decayed:deleted when the bound file is removed", async () => {
    const file = "src/example.ts";
    const content = "const x = 1;\n";
    writeFileSync(join(tmpRoot, file), content);

    const inv = makeInvariantBoundTo(file, 1, 1, content.split("\n").slice(0, 1).join("\n"));
    writeInvariant(tmpRoot, inv);

    // Holds first.
    const before = await verifyAll(tmpRoot);
    expect(before.verdicts[0].status).toBe("holds");

    // Delete the file.
    const { unlinkSync } = await import("fs");
    unlinkSync(join(tmpRoot, file));

    const after = await verifyAll(tmpRoot);
    expect(after.verdicts[0].status).toBe("decayed");
    expect(after.verdicts[0].bindings[0].decayKind).toBe("deleted");
    expect(after.verdicts[0].bindings[0].reason).toMatch(/file not found/);
  });

  it("reports decayed:changed when line range exceeds file length", async () => {
    const file = "src/example.ts";
    const content = "const x = 1;\nconst y = 2;\n";
    writeFileSync(join(tmpRoot, file), content);

    // Mint with a line range BEYOND what the file has.
    const inv = makeInvariantBoundTo(file, 1, 10, "irrelevant");
    writeInvariant(tmpRoot, inv);

    const report = await verifyAll(tmpRoot);
    expect(report.verdicts[0].status).toBe("decayed");
    expect(report.verdicts[0].bindings[0].decayKind).toBe("changed");
    expect(report.verdicts[0].bindings[0].reason).toMatch(/exceeds file length/);
  });
});
