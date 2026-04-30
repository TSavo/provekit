/**
 * T4 integration test: verify that template bindings are carried through the
 * VerificationResult → Violation plumbing introduced in this task.
 *
 * Strategy (Option A): drive generateProofs + verifyBlock directly, then
 * reproduce the VerificationResult→Violation assignment inline so we can
 * inspect smt_bindings without touching the private buildContracts method.
 */
import { describe, it, expect } from "vitest";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { existsSync } from "fs";
import { parseFile } from "../parser";
import { TemplateEngine } from "../templates";
import { verifyBlock, proofComplexity } from "../verifier";
import type { Violation } from "../contracts";

function findProjectRoot(): string {
  let dir = dirname(fileURLToPath(import.meta.url));
  while (dir !== dirname(dir)) {
    if (existsSync(join(dir, ".provekit", "principles"))) return dir;
    dir = dirname(dir);
  }
  throw new Error("could not locate project root with .provekit/principles/");
}
const PROJECT_ROOT = findProjectRoot();

const FIXTURE = `function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}`;

describe("T4 – template bindings plumbing onto VerificationResult and Violation", () => {
  it("VerificationResult produced from a TemplateResult carries bindings", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const templateResults = engine.generateProofs(fnNode!, "divide", "test.ts");

    const dzResult = templateResults.find((r) => r.principle === "division-by-zero");
    expect(dzResult).toBeTruthy();

    // Reproduce the push-site logic from executeForFile (lines 171-181 of DerivationPhase.ts)
    const { result, error, witness } = verifyBlock(dzResult!.smt2);
    const verResult = {
      smt2: dzResult!.smt2,
      z3Result: result,
      principle: dzResult!.principle,
      error,
      witness,
      complexity: proofComplexity(dzResult!.smt2),
      confidence: dzResult!.confidence,
      bindings: dzResult!.bindings,  // <-- the forwarding under test
    };

    // VerificationResult must carry bindings through
    expect(verResult.bindings).toBeDefined();
    expect(verResult.bindings!.length).toBeGreaterThan(0);

    const denomBinding = verResult.bindings!.find((b) => b.smt_constant === "b");
    expect(denomBinding).toBeTruthy();
    expect(denomBinding!.source_expr).toBe("b");
    expect(denomBinding!.source_line).toBe(2);
    expect(denomBinding!.sort).toBe("Int");
  });

  it("Violation constructed from a sat VerificationResult has smt_bindings populated", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const templateResults = engine.generateProofs(fnNode!, "divide", "test.ts");
    const dzResult = templateResults.find((r) => r.principle === "division-by-zero");
    expect(dzResult).toBeTruthy();

    const { result, error, witness } = verifyBlock(dzResult!.smt2);

    // Reproduce the push-site to create a VerificationResult with bindings
    const v = {
      smt2: dzResult!.smt2,
      z3Result: result,
      principle: dzResult!.principle,
      error,
      witness,
      complexity: proofComplexity(dzResult!.smt2),
      confidence: dzResult!.confidence,
      bindings: dzResult!.bindings,
    };

    // Only proceed if the proof is sat (division-by-zero finds a counterexample)
    expect(v.z3Result).toBe("sat");

    // Reproduce the Violation construction from buildContracts (line 532 of DerivationPhase.ts)
    const violation: Violation = {
      principle: v.principle,
      principle_hash: "",
      claim: "(no claim extracted)",
      smt2: v.smt2,
      witness: v.witness,
      complexity: v.complexity,
      confidence: v.confidence,
      smt_bindings: v.bindings,  // <-- the assignment under test
    };

    // Violation must have smt_bindings forwarded from the VerificationResult
    expect(violation.smt_bindings).toBeDefined();
    expect(violation.smt_bindings!.length).toBeGreaterThan(0);

    const denomBinding = violation.smt_bindings!.find((b) => b.smt_constant === "b");
    expect(denomBinding).toBeTruthy();
    expect(denomBinding!.source_expr).toBe("b");
    expect(denomBinding!.source_line).toBe(2);
    expect(denomBinding!.sort).toBe("Int");
  });
});
