/**
 * Principalize workflow integration test. Drives the YAML manifest
 * end-to-end via runManifest, asserting:
 *  1. The manifest loads cleanly.
 *  2. Stages produce a valid validate result on a real corpus.
 *  3. The publish Action lands a LibraryPrinciple JSON on disk.
 *  4. Empty-corpus case short-circuits cleanly without writing.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, existsSync, readFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import {
  writeInvariant,
  type StoredInvariant,
} from "../fix/runtime/invariantStore.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import type { LibraryPrinciple } from "../fix/types.js";
import {
  loadPrincipalizeManifest,
  registerPrincipalizeRegistries,
  PRINCIPALIZE_STAGE_CAPABILITIES,
  PRINCIPALIZE_ACTION_CAPABILITIES,
  type PrincipalizeWorkflowInput,
} from "./principalize.js";
import type { ValidateAdversarialResult } from "../workflow/producers/validateAdversarial.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "principalize-wf-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

function makeInv(id: string, sorts: string[]): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: id,
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (not (= x 0)))",
    },
    bindings: sorts.map((sort, idx) => ({
      type: "local" as const,
      smt_constant: `x${idx}`,
      source_expr: "expr",
      sort,
      node: {
        filePath: "src/m.ts",
        nodeHash: "h",
        startLine: 1,
        endLine: 1,
      },
    })),
    callsite: {
      filePath: "src/m.ts",
      function: null,
      startLine: 1,
      endLine: 1,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

describe("principalize workflow", () => {
  it("loads the manifest cleanly with three stages and one action", () => {
    const manifest = loadPrincipalizeManifest();
    expect(manifest.name).toBe("principalize");
    expect(manifest.nodes.map((n) => n.capability).sort()).toEqual(
      [...PRINCIPALIZE_STAGE_CAPABILITIES].sort(),
    );
    expect((manifest.actions ?? []).map((a) => a.action)).toEqual(
      [...PRINCIPALIZE_ACTION_CAPABILITIES],
    );
    expect(manifest.output).toBe("$node.validate.output");
  });

  it("produces a clean verdict and publishes a principle on a real corpus", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInv("aa", ["Int", "Int"]));
    writeInvariant(projectRoot, makeInv("bb", ["Int", "Int"]));

    const manifest = loadPrincipalizeManifest();
    const { registry, actionRegistry } = registerPrincipalizeRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: PrincipalizeWorkflowInput = {
      projectRoot,
      proposedPrincipleName: "div-by-zero-pair",
      proposedBugClassId: "div-by-zero",
    };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    const verdict = result.output as ValidateAdversarialResult;
    expect(verdict.verdict).toBe("clean");
    expect(verdict.falsePositives).toEqual([]);

    const principlePath = join(
      projectRoot,
      ".provekit",
      "principles",
      "div-by-zero-pair.json",
    );
    expect(existsSync(principlePath)).toBe(true);
    const written = JSON.parse(readFileSync(principlePath, "utf-8")) as LibraryPrinciple;
    expect(written.id).toBe("div-by-zero-pair");
    expect(written.bug_class_id).toBe("div-by-zero");
  });

  it("short-circuits to clean and skips publish on an empty corpus", async () => {
    const { projectRoot, db } = makeProjectAndDb();

    const manifest = loadPrincipalizeManifest();
    const { registry, actionRegistry } = registerPrincipalizeRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: PrincipalizeWorkflowInput = {
      projectRoot,
      proposedPrincipleName: "p",
      proposedBugClassId: "p",
    };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    const verdict = result.output as ValidateAdversarialResult;
    expect(verdict.verdict).toBe("clean");
    expect(verdict.validator).toBe("empty-corpus-short-circuit");
    expect(
      existsSync(join(projectRoot, ".provekit", "principles", "p.json")),
    ).toBe(false);
  });
});
