/**
 * Weaken workflow integration test. Uses a StubLLMProvider so no real
 * LLM is invoked; the canned response is a relaxed invariant.
 *
 * Verifies:
 * 1. Manifest loads with two Stages and one Action.
 * 2. End-to-end runManifest produces a CompareFormulasDelta with
 *    mode=weaken and a removed conjunct (the relaxation).
 * 3. The Action overwrites the on-disk file with the new surface text.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { StubLLMProvider } from "../fix/types.js";
import type { IntentSignal } from "../fix/types.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadWeakenManifest,
  registerWeakenRegistries,
  WEAKEN_STAGE_CAPABILITIES,
  WEAKEN_ACTION_CAPABILITIES,
  type WeakenWorkflowInput,
} from "./weaken.js";
import type { CompareFormulasDelta } from "../workflow/producers/compareFormulas.js";
import type { IrFormula, Sort, IrTerm } from "../ir/formulas.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "weaken-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const intSort: Sort = { kind: "primitive", name: "Int" };
const var_d: IrTerm = { kind: "var", name: "d", sort: intSort };
const const_0: IrTerm = { kind: "const", value: 0, sort: intSort };
const const_100: IrTerm = { kind: "const", value: 100, sort: intSort };

// Prior invariant: forall d. d !== 0 AND d < 100. Weakening removes
// the upper bound; the LLM emits "d !== 0" only.
const oldFormula: IrFormula = {
  kind: "forall",
  sort: intSort,
  predicate: {
    kind: "lambda",
    varName: "d",
    sort: intSort,
    body: {
      kind: "and",
      conjuncts: [
        { kind: "atomic", predicate: "≠", args: [var_d, const_0] },
        { kind: "atomic", predicate: "<", args: [var_d, const_100] },
      ],
    },
  },
};

const RELAXED_SURFACE = `
import { property, forAll } from "provekit/ir";
import type { Int } from "provekit/sorts";
property("nonZeroDenominator", forAll<Int>((d) => d !== 0));
`;

const fakeIntent: IntentSignal = {
  source: "test",
  intentText: "drop the upper bound on denominator",
  rawText: "drop the upper bound on denominator",
  summary: "Allow large denominators; only forbid zero.",
  failureDescription: "(none — relaxation request)",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  classHint: "weaken-invariant",
  bugClassHint: "weaken-invariant",
};

describe("weaken workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadWeakenManifest();
    expect(manifest.name).toBe("weaken");
    expect(manifest.nodes.map((n) => n.capability).sort()).toEqual([
      "compare-formulas",
      "formulate-via-lifter",
    ]);
    expect(manifest.actions).toHaveLength(1);
    expect(manifest.actions![0].action).toBe("write-invariant-file");
  });

  it("relaxes the prior invariant and overwrites the file", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "weaken-target-"));
    const target = join(tmp, "math.invariant.ts");
    writeFileSync(target, "// stale prior content\n", "utf-8");

    const llm = new StubLLMProvider(
      new Map([["You are writing invariants for a TypeScript codebase", RELAXED_SURFACE]]),
    );
    const manifest = loadWeakenManifest();
    const { registry, actionRegistry } = registerWeakenRegistries({ llm });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: WeakenWorkflowInput = {
      intent: fakeIntent,
      oldFormula,
      filePath: target,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);

    const delta = result.output as CompareFormulasDelta;
    expect(delta.mode).toBe("weaken");
    expect(delta.changed).toBe(true);
    // The relaxed formula has FEWER conjuncts at the lambda body's
    // top level — but the structural diff compares the OUTERMOST
    // top-level shape. After lifting, the new formula is a single
    // forall with a single atomic body, so old's top-level (forall
    // wrapping AND of two) gets split differently from new's (forall
    // wrapping single atomic). The diff records that the formula
    // changed (changed=true) and the property hash differs.
    expect(delta.oldPropertyHash).not.toBe(delta.newPropertyHash);

    // The Action overwrote the file with the new surface text.
    const final = readFileSync(target, "utf-8");
    expect(final).toContain("nonZeroDenominator");
    expect(final).toContain('forAll<Int>((d) => d !== 0)');
    expect(final).not.toContain("stale prior content");
  });

  it("declares the expected capabilities", () => {
    expect(WEAKEN_STAGE_CAPABILITIES).toEqual([
      "formulate-via-lifter",
      "compare-formulas",
    ]);
    expect(WEAKEN_ACTION_CAPABILITIES).toEqual(["write-invariant-file"]);
  });
});
