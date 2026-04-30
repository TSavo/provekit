/**
 * prove-with-lean workflow end-to-end smoke.
 *
 * Two variants:
 *   1. Loaded-only test — verify the manifest parses and all capabilities
 *      register. Always runs.
 *   2. Action-stubbed test — replace provideLeanProof with a fake that
 *      returns a canned verdict, and assert the verdict memento lands at
 *      the recovered (bindingHash, propertyHash). Always runs.
 *   3. Real `lean` test — only runs if `lean` is on PATH. Mints a memento
 *      for `forall (Int) x. x = x`, supplies `fun x => rfl` as proof,
 *      asserts verdict "valid".
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { spawnSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../db/index.js";
import { writeMemento, findAll } from "../fix/runtime/mementoStore.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadProveWithLeanManifest,
  registerProveWithLeanRegistries,
  PROVE_WITH_LEAN_STAGE_CAPABILITIES,
  PROVE_WITH_LEAN_ACTION_CAPABILITIES,
  type ProveWithLeanWorkflowInput,
} from "./prove-with-lean.js";
import {
  PROVIDE_LEAN_PROOF_CAPABILITY,
  type ProvideLeanProofActionInput,
  type ProvideLeanProofResource,
} from "../workflow/producers/provideLeanProof.js";
import type { IrFormula } from "../ir/formulas.js";
import type { Action } from "../workflow/types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "prove-with-lean-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const reflexivityFormula: IrFormula = {
  kind: "forall",
  sort: { kind: "primitive", name: "Int" },
  predicate: {
    kind: "lambda",
    varName: "_x0",
    sort: { kind: "primitive", name: "Int" },
    body: {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "var", name: "_x0", sort: { kind: "primitive", name: "Int" } },
        { kind: "var", name: "_x0", sort: { kind: "primitive", name: "Int" } },
      ],
    },
  },
};

function mintFormulateMemento(
  db: Db,
  propertyHash: string,
  bindingHash: string,
  formula: IrFormula = reflexivityFormula,
): string {
  const witnessPayload = {
    surfaceText: "property('reflexive', forAll<Int>(x => x === x));",
    formula,
    propertyHash,
    name: "reflexive",
    inputCidsToCompose: [],
  };
  const row = writeMemento(db, {
    bindingHash,
    propertyHash,
    verdict: "holds",
    witness: JSON.stringify(witnessPayload),
    producedBy: "formulate-via-lifter@v1",
  });
  return row.cid!;
}

function fakeProvideLeanProofAction(
  out: ProvideLeanProofResource,
): Action<ProvideLeanProofActionInput, ProvideLeanProofResource> {
  return {
    name: "provideLeanProof",
    producedBy: "lean-fake@test",
    serializeInput: (i) => ({
      theoremSource: i.theoremSource,
      proofText: i.proofText,
      theoremName: i.theoremName,
    }),
    describeResource: (r) => `fake lean verdict ${r.verdict}`,
    run: async () => out,
  };
}

function leanIsAvailable(): boolean {
  try {
    const r = spawnSync("lean", ["--version"], { encoding: "utf-8" });
    return r.status === 0;
  } catch {
    return false;
  }
}

describe("prove-with-lean workflow (manifest + registry)", () => {
  it("loads the manifest and registers all required capabilities", () => {
    const manifest = loadProveWithLeanManifest();
    expect(manifest.name).toBe("prove-with-lean");

    const db = makeDb();
    const { registry, actionRegistry } = registerProveWithLeanRegistries({ db });
    for (const cap of PROVE_WITH_LEAN_STAGE_CAPABILITIES) {
      expect(registry.resolve(cap)).not.toBeNull();
    }
    for (const cap of PROVE_WITH_LEAN_ACTION_CAPABILITIES) {
      expect(actionRegistry.resolve(cap)).not.toBeNull();
    }
  });

  it("produces verdict 'holds' when provideLeanProof returns valid", async () => {
    const db = makeDb();
    mintFormulateMemento(db, "propAA000000aaaa", "bindAA000000aaaa");

    const manifest = loadProveWithLeanManifest();
    const { registry, actionRegistry } = registerProveWithLeanRegistries({ db });
    actionRegistry.replace(
      PROVIDE_LEAN_PROOF_CAPABILITY,
      fakeProvideLeanProofAction({
        verdict: "valid",
        combinedSource: "theorem t : True := trivial\n",
        stdout: "",
        stderr: "",
        leanRunMs: 5,
        leanVersion: "4.6.0",
      }),
    );

    const workflow = manifestToWorkflow(manifest);
    const runner = new WorkflowRunner(db, workflow, registry);
    const input: ProveWithLeanWorkflowInput = {
      propertyHash: "propAA000000aaaa",
      proofText: "trivial",
      producedBy: "lean-fake@test",
    };
    await runManifest(runner, registry, manifest, input, actionRegistry);

    const all = findAll(db, {
      bindingHash: "bindAA000000aaaa",
      propertyHash: "propAA000000aaaa",
    });
    const verdict = all.find((m) => m.producedBy === "lean-fake@test");
    expect(verdict).toBeDefined();
    expect(verdict!.verdict).toBe("holds");
  });

  it("produces verdict 'undecidable' when provideLeanProof returns invalid", async () => {
    const db = makeDb();
    mintFormulateMemento(db, "propBB000000bbbb", "bindBB000000bbbb");

    const manifest = loadProveWithLeanManifest();
    const { registry, actionRegistry } = registerProveWithLeanRegistries({ db });
    actionRegistry.replace(
      PROVIDE_LEAN_PROOF_CAPABILITY,
      fakeProvideLeanProofAction({
        verdict: "invalid",
        combinedSource: "theorem t : True := bogus\n",
        stdout: "error: unknown identifier 'bogus'\n",
        stderr: "",
        leanRunMs: 12,
      }),
    );

    const workflow = manifestToWorkflow(manifest);
    const runner = new WorkflowRunner(db, workflow, registry);
    const input: ProveWithLeanWorkflowInput = {
      propertyHash: "propBB000000bbbb",
      proofText: "bogus",
      producedBy: "lean-fake@test",
    };
    await runManifest(runner, registry, manifest, input, actionRegistry);

    const all = findAll(db, {
      bindingHash: "bindBB000000bbbb",
      propertyHash: "propBB000000bbbb",
    });
    const verdict = all.find((m) => m.producedBy === "lean-fake@test");
    expect(verdict).toBeDefined();
    expect(verdict!.verdict).toBe("undecidable");
  });

  // Real-lean test only runs if `lean` is on PATH. Per spec: skip
  // gracefully when the binary is unavailable.
  const realLean = leanIsAvailable() ? it : it.skip;
  realLean(
    "real `lean` proves forall (x: Int), x = x with `intro x; rfl`",
    async () => {
      const db = makeDb();
      mintFormulateMemento(db, "propCC000000cccc", "bindCC000000cccc");

      const manifest = loadProveWithLeanManifest();
      const { registry, actionRegistry } = registerProveWithLeanRegistries({ db });

      const workflow = manifestToWorkflow(manifest);
      const runner = new WorkflowRunner(db, workflow, registry);
      const input: ProveWithLeanWorkflowInput = {
        propertyHash: "propCC000000cccc",
        // Lean tactic block: introduce x, then rfl. Theorem source has
        // `:= by\n  sorry`; provideLeanProof splices in our proofText.
        proofText: "by intro x; rfl",
        producedBy: "lean-real@test",
      };
      await runManifest(runner, registry, manifest, input, actionRegistry);

      const all = findAll(db, {
        bindingHash: "bindCC000000cccc",
        propertyHash: "propCC000000cccc",
      });
      const verdict = all.find((m) => m.producedBy === "lean-real@test");
      expect(verdict).toBeDefined();
      expect(verdict!.verdict).toBe("holds");
    },
    120_000,
  );
});
