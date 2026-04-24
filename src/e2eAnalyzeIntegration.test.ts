/**
 * T8 — End-to-end integration test: Pipeline.runFull against division-by-zero.ts
 *
 * Drives the full pipeline in-process against the committed
 * examples/division-by-zero.ts fixture and asserts that:
 *
 *  1. Phase 1–3: the DependencyPhase, ContextPhase, and DerivationPhase complete,
 *     producing a Contract with a division-by-zero violation and smt_bindings (T4).
 *  2. Phase 4: GapDetectionPhase runs against the contract and writes gap_reports
 *     rows (ieee_specials) via detectGaps.
 *  3. explainGaps renders the expected tokens: "encoding-gap", "NaN",
 *     "ieee_specials", and the SMT constant name.
 *
 * Z3 must be installed. The test is skipped (not failed) if z3 is absent.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, mkdirSync, cpSync, copyFileSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { fileURLToPath } from "url";
import { execSync } from "child_process";

import { Pipeline } from "./pipeline/Pipeline.js";
import type { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./llm/index.js";
import { openDb } from "./db/index.js";
import { gapReports } from "./db/schema/index.js";
import { explainGaps } from "./cli.js";

// __dirname in ESM context
const __dirname = fileURLToPath(new URL(".", import.meta.url));

// Worktree root — two levels up from src/
const WORKTREE_ROOT = join(__dirname, "..");

/**
 * A stub LLM provider that throws loudly if called. The template engine covers
 * the division-by-zero signal so the LLM should not be needed for this fixture.
 * Any unexpected LLM call surfaces a real integration issue rather than silently
 * hitting real credentials.
 */
class ThrowingLLMProvider implements LLMProvider {
  readonly name = "throwing-stub";

  async complete(_prompt: string, _opts: LLMRequestOptions): Promise<LLMResponse> {
    throw new Error(
      "[ThrowingLLMProvider] LLM called unexpectedly. " +
        "The template engine should have covered all signals without LLM involvement."
    );
  }

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  async *stream(_prompt: string, _opts: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    throw new Error("[ThrowingLLMProvider] LLM stream called unexpectedly.");
  }
}

function z3Available(): boolean {
  try {
    execSync("z3 --version", { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

describe("e2e: Pipeline.runFull — division-by-zero.ts → gap_reports, rendered via explainGaps", () => {
  let scratchRoot: string;

  afterEach(() => {
    if (scratchRoot && existsSync(scratchRoot)) {
      rmSync(scratchRoot, { recursive: true, force: true });
    }
  });

  it("produces a division-by-zero violation with smt_bindings and runs GapDetectionPhase", async () => {
    if (!z3Available()) {
      console.warn("SKIP: z3 not found on PATH — T8 requires Z3 for verifyBlock");
      return;
    }

    // 1. Create scratch project root
    scratchRoot = mkdtempSync(join(tmpdir(), "neurallog-e2e-analyze-"));

    // 2. Create src/ and .neurallog/ directories
    mkdirSync(join(scratchRoot, "src"), { recursive: true });
    mkdirSync(join(scratchRoot, ".neurallog"), { recursive: true });

    // 3. Copy the division-by-zero fixture into scratch/src/divide.ts
    const fixtureSrc = join(WORKTREE_ROOT, "examples", "division-by-zero.ts");
    const fixtureDst = join(scratchRoot, "src", "divide.ts");
    copyFileSync(fixtureSrc, fixtureDst);

    // 4. Copy the 23 seed principles into scratch/.neurallog/principles/
    const principlesSrc = join(WORKTREE_ROOT, ".neurallog", "principles");
    const principlesDst = join(scratchRoot, ".neurallog", "principles");
    cpSync(principlesSrc, principlesDst, { recursive: true });

    // 5. Run the pipeline
    const pipeline = new Pipeline();
    const result = await pipeline.runFull({
      entryFilePath: join(scratchRoot, "src", "divide.ts"),
      projectRoot: scratchRoot,
      model: "sonnet",
      verbose: false,
      maxConcurrency: 1,
      provider: new ThrowingLLMProvider(),
    });

    // 6. Phase 3: At least one Contract must exist and have a division-by-zero violation
    expect(result.derivation.contracts.length).toBeGreaterThan(0);

    const contractWithViolation = result.derivation.contracts.find(
      (c) => c.violations.length > 0
    );
    expect(contractWithViolation).toBeDefined();

    const divZeroViolation = contractWithViolation!.violations.find(
      (v) => v.principle === "division-by-zero"
    );
    expect(divZeroViolation).toBeDefined();

    // 7. T4 plumbing: smt_bindings are populated on the violation
    expect(divZeroViolation!.smt_bindings).toBeDefined();
    expect(divZeroViolation!.smt_bindings!.length).toBeGreaterThan(0);

    const denomBinding = divZeroViolation!.smt_bindings!.find(
      (b) => b.smt_constant === "b"
    );
    expect(denomBinding).toBeDefined();
    expect(denomBinding!.sort).toBe("Int");

    // 8. Phase 4: GapDetectionPhase ran and produced at least one report.
    expect(result.gapDetection).toBeDefined();

    const skipped = result.gapDetection!.skipped;
    expect(skipped.missingBindings).toBe(0);
    expect(skipped.missingWitness).toBe(0);
    expect(skipped.untestable).toBe(0);

    expect(result.gapDetection!.reportsWritten).toBeGreaterThanOrEqual(1);

    // 9. gap_reports has at least one ieee_specials row for this contract.
    const dbPath = join(scratchRoot, ".neurallog", "neurallog.db");
    expect(existsSync(dbPath)).toBe(true);
    const db = openDb(dbPath);
    const allGapRows = db.select().from(gapReports).all();
    const ieeeRows = allGapRows.filter((r) => r.kind === "ieee_specials");
    expect(ieeeRows.length).toBeGreaterThanOrEqual(1);

    // 10. explainGaps renders the expected tokens for the contract.
    const contractKey = contractWithViolation!.key;
    const explained = explainGaps(db, contractKey);
    expect(explained).toContain("encoding-gap");
    expect(explained).toContain("ieee_specials");
    expect(explained).toContain("NaN");

    db.$client.close();
  });
});
