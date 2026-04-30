/**
 * Strengthen workflow integration test. Uses a StubLLMProvider so no
 * real LLM is invoked; the canned response is a tightened invariant.
 *
 * Verifies:
 * 1. Manifest loads with two Stages and one Action.
 * 2. End-to-end runManifest produces a CompareFormulasDelta with
 *    mode=strengthen.
 * 3. The Action overwrites the on-disk file with the new surface.
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
  loadStrengthenManifest,
  registerStrengthenRegistries,
  STRENGTHEN_STAGE_CAPABILITIES,
  STRENGTHEN_ACTION_CAPABILITIES,
  type StrengthenWorkflowInput,
} from "./strengthen.js";
import type { CompareFormulasDelta } from "../workflow/producers/compareFormulas.js";
import type { IrFormula, Sort, IrTerm } from "../ir/formulas.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "strengthen-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const intSort: Sort = { kind: "primitive", name: "Int" };
const var_n: IrTerm = { kind: "var", name: "n", sort: intSort };
const const_0: IrTerm = { kind: "const", value: 0, sort: intSort };

// Prior invariant: forall n. n > 0. Strengthening adds n < 100.
const oldFormula: IrFormula = {
  kind: "forall",
  sort: intSort,
  predicate: {
    kind: "lambda",
    varName: "n",
    sort: intSort,
    body: { kind: "atomic", predicate: ">", args: [var_n, const_0] },
  },
};

const TIGHTENED_SURFACE = `
import { property, forAll } from "provekit/ir";
import type { Int } from "provekit/sorts";
property("boundedPositive", forAll<Int>((n) => n > 0 && n < 100));
`;

const fakeIntent: IntentSignal = {
  source: "test",
  intentText: "add upper bound: n must be less than 100",
  rawText: "add upper bound: n must be less than 100",
  summary: "Tighten the contract: positive AND bounded.",
  failureDescription: "(none — strengthening request)",
  codeReferences: [{ file: "src/bounds.ts", line: 1 }],
  classHint: "strengthen-invariant",
  bugClassHint: "strengthen-invariant",
};

describe("strengthen workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadStrengthenManifest();
    expect(manifest.name).toBe("strengthen");
    expect(manifest.nodes.map((n) => n.capability).sort()).toEqual([
      "compare-formulas",
      "formulate-via-lifter",
    ]);
    expect(manifest.actions).toHaveLength(1);
    expect(manifest.actions![0].action).toBe("write-invariant-file");
  });

  it("tightens the prior invariant and overwrites the file", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "strengthen-target-"));
    const target = join(tmp, "bounds.invariant.ts");
    writeFileSync(target, "// stale prior content\n", "utf-8");

    const llm = new StubLLMProvider(
      new Map([["You are writing invariants for a TypeScript codebase", TIGHTENED_SURFACE]]),
    );
    const manifest = loadStrengthenManifest();
    const { registry, actionRegistry } = registerStrengthenRegistries({ llm });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: StrengthenWorkflowInput = {
      intent: fakeIntent,
      oldFormula,
      filePath: target,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);

    const delta = result.output as CompareFormulasDelta;
    expect(delta.mode).toBe("strengthen");
    expect(delta.changed).toBe(true);
    expect(delta.oldPropertyHash).not.toBe(delta.newPropertyHash);

    const final = readFileSync(target, "utf-8");
    expect(final).toContain("boundedPositive");
    expect(final).toContain("n > 0 && n < 100");
    expect(final).not.toContain("stale prior content");
  });

  it("declares the expected capabilities", () => {
    expect(STRENGTHEN_STAGE_CAPABILITIES).toEqual([
      "formulate-via-lifter",
      "compare-formulas",
    ]);
    expect(STRENGTHEN_ACTION_CAPABILITIES).toEqual(["write-invariant-file"]);
  });
});
