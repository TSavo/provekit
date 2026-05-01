/**
 * prove-cross-solver workflow end-to-end smoke.
 *
 * Three test classes:
 *   1. Manifest + registry. Always runs. Verifies the manifest parses
 *      and all capabilities register.
 *   2. Stub-driven. Always runs. Replaces invoke-z3 and invoke-cvc5
 *      with fakes returning canned verdicts. Asserts (a) agreement
 *      mints a memento with the correct verdict, (b) disagreement
 *      mints an "undecidable" memento, (c) the memento's inputCids
 *      contain the source IR memento CID (the architectural claim
 *      under test).
 *   3. Real binaries. Only runs when BOTH `z3` and `cvc5` are on
 *      PATH. Drives `forall (x: Int), x = x` end-to-end and asserts
 *      both solvers say "unsat" on the negation.
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
  loadProveCrossSolverManifest,
  registerProveCrossSolverRegistries,
  PROVE_CROSS_SOLVER_STAGE_CAPABILITIES,
  PROVE_CROSS_SOLVER_ACTION_CAPABILITIES,
  type ProveCrossSolverWorkflowInput,
} from "./prove-cross-solver.js";
import {
  INVOKE_Z3_CAPABILITY,
  type InvokeZ3StageInput,
  type InvokeZ3StageOutput,
} from "../workflow/producers/invokeZ3.js";
import {
  INVOKE_CVC5_CAPABILITY,
  type InvokeCvc5StageInput,
  type InvokeCvc5StageOutput,
} from "../workflow/producers/invokeCvc5.js";
import type { IrFormula } from "../ir/formulas.js";
import type { Stage } from "../workflow/types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "prove-cross-solver-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

// forall (x: Int), x = x. The canonical solver-trivial property.
// Both Z3 and CVC5 say "unsat" on the negation. Used as the
// real-binary smoke test; the stub-driven tests don't actually feed
// SMT-LIB into a solver, so the formula's semantics there don't matter.
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

function fakeInvokeZ3Stage(
  out: InvokeZ3StageOutput,
): Stage<InvokeZ3StageInput, InvokeZ3StageOutput> {
  return {
    name: "invoke-z3",
    producedBy: "z3-fake@test",
    serializeInput: (input) => ({
      smtLib: input.smtLib,
      timeoutMs: input.timeoutMs ?? 30_000,
      binary: input.binary ?? "z3",
    }),
    serializeOutput: (o) => JSON.stringify(o),
    deserializeOutput: (w) => JSON.parse(w) as InvokeZ3StageOutput,
    run: async () => out,
  };
}

function fakeInvokeCvc5Stage(
  out: InvokeCvc5StageOutput,
): Stage<InvokeCvc5StageInput, InvokeCvc5StageOutput> {
  return {
    name: "invoke-cvc5",
    producedBy: "cvc5-fake@test",
    serializeInput: (input) => ({
      smtLib: input.smtLib,
      timeoutMs: input.timeoutMs ?? 30_000,
      binary: input.binary ?? "cvc5",
    }),
    serializeOutput: (o) => JSON.stringify(o),
    deserializeOutput: (w) => JSON.parse(w) as InvokeCvc5StageOutput,
    run: async () => out,
  };
}

function binaryAvailable(name: string): boolean {
  try {
    const r = spawnSync(name, ["--version"], { encoding: "utf-8" });
    return r.status === 0;
  } catch {
    return false;
  }
}

describe("prove-cross-solver workflow (manifest + registry)", () => {
  it("loads the manifest and registers all required capabilities", () => {
    const manifest = loadProveCrossSolverManifest();
    expect(manifest.name).toBe("prove-cross-solver");

    const db = makeDb();
    const { registry, actionRegistry } = registerProveCrossSolverRegistries({ db });
    for (const cap of PROVE_CROSS_SOLVER_STAGE_CAPABILITIES) {
      expect(registry.resolve(cap)).not.toBeNull();
    }
    for (const cap of PROVE_CROSS_SOLVER_ACTION_CAPABILITIES) {
      expect(actionRegistry.resolve(cap)).not.toBeNull();
    }
  });

  it("mints an agreement memento (verdict 'holds') when both solvers return unsat", async () => {
    const db = makeDb();
    const sourceCid = mintFormulateMemento(
      db,
      "propAA111111aaaa",
      "bindAA111111aaaa",
    );

    const manifest = loadProveCrossSolverManifest();
    const { registry, actionRegistry } = registerProveCrossSolverRegistries({ db });
    registry.replace(
      INVOKE_Z3_CAPABILITY,
      fakeInvokeZ3Stage({
        z3Verdict: "unsat",
        stdout: "unsat\n",
        stderr: "",
        z3RunMs: 4,
      }),
    );
    registry.replace(
      INVOKE_CVC5_CAPABILITY,
      fakeInvokeCvc5Stage({
        cvc5Verdict: "unsat",
        stdout: "unsat\n",
        stderr: "",
        cvc5RunMs: 6,
      }),
    );

    const workflow = manifestToWorkflow(manifest);
    const runner = new WorkflowRunner(db, workflow, registry);
    const input: ProveCrossSolverWorkflowInput = {
      propertyHash: "propAA111111aaaa",
      producedBy: "cross-solver-fake@test",
    };
    await runManifest(runner, registry, manifest, input, actionRegistry);

    const all = findAll(db, {
      bindingHash: "bindAA111111aaaa",
      propertyHash: "propAA111111aaaa",
    });
    const memento = all.find((m) => m.producedBy === "cross-solver-fake@test");
    expect(memento).toBeDefined();
    expect(memento!.verdict).toBe("holds");

    // The architectural claim under test: the cross-solver memento's
    // inputCids must reference the source IR memento CID, threading the
    // verdict provenance back to propertyHash CID identity.
    const memInputCids = memento!.inputCids ?? [];
    expect(memInputCids).toContain(sourceCid);
  });

  it("mints a disagreement memento (verdict 'undecidable') when solvers split", async () => {
    const db = makeDb();
    mintFormulateMemento(db, "propBB222222bbbb", "bindBB222222bbbb");

    const manifest = loadProveCrossSolverManifest();
    const { registry, actionRegistry } = registerProveCrossSolverRegistries({ db });
    registry.replace(
      INVOKE_Z3_CAPABILITY,
      fakeInvokeZ3Stage({
        z3Verdict: "unsat",
        stdout: "unsat\n",
        stderr: "",
        z3RunMs: 4,
      }),
    );
    registry.replace(
      INVOKE_CVC5_CAPABILITY,
      fakeInvokeCvc5Stage({
        cvc5Verdict: "unknown",
        stdout: "unknown\n",
        stderr: "",
        cvc5RunMs: 9,
      }),
    );

    const workflow = manifestToWorkflow(manifest);
    const runner = new WorkflowRunner(db, workflow, registry);
    const input: ProveCrossSolverWorkflowInput = {
      propertyHash: "propBB222222bbbb",
      producedBy: "cross-solver-disagree@test",
    };
    await runManifest(runner, registry, manifest, input, actionRegistry);

    const all = findAll(db, {
      bindingHash: "bindBB222222bbbb",
      propertyHash: "propBB222222bbbb",
    });
    const memento = all.find((m) => m.producedBy === "cross-solver-disagree@test");
    expect(memento).toBeDefined();
    // Disagreement is captured as data, not a hard error: verdict is
    // "undecidable" even though Z3 was definite.
    expect(memento!.verdict).toBe("undecidable");

    // The witness should record both solver verdicts so disagreement
    // is auditable.
    const witness = JSON.parse(memento!.witness ?? "{}");
    expect(witness.agree).toBe(false);
    expect(witness.z3Verdict).toBe("unsat");
    expect(witness.cvc5Verdict).toBe("unknown");
  });

  it("mints a violated memento when both solvers return sat", async () => {
    const db = makeDb();
    mintFormulateMemento(db, "propCC333333cccc", "bindCC333333cccc");

    const manifest = loadProveCrossSolverManifest();
    const { registry, actionRegistry } = registerProveCrossSolverRegistries({ db });
    registry.replace(
      INVOKE_Z3_CAPABILITY,
      fakeInvokeZ3Stage({
        z3Verdict: "sat",
        stdout: "sat\n",
        stderr: "",
        z3RunMs: 4,
      }),
    );
    registry.replace(
      INVOKE_CVC5_CAPABILITY,
      fakeInvokeCvc5Stage({
        cvc5Verdict: "sat",
        stdout: "sat\n",
        stderr: "",
        cvc5RunMs: 7,
      }),
    );

    const workflow = manifestToWorkflow(manifest);
    const runner = new WorkflowRunner(db, workflow, registry);
    const input: ProveCrossSolverWorkflowInput = {
      propertyHash: "propCC333333cccc",
      producedBy: "cross-solver-violated@test",
    };
    await runManifest(runner, registry, manifest, input, actionRegistry);

    const all = findAll(db, {
      bindingHash: "bindCC333333cccc",
      propertyHash: "propCC333333cccc",
    });
    const memento = all.find((m) => m.producedBy === "cross-solver-violated@test");
    expect(memento).toBeDefined();
    expect(memento!.verdict).toBe("violated");
  });

  // Real-binary test only runs when BOTH z3 and cvc5 are present.
  // Mirrors the prove-with-lean test's `realLean` skip pattern.
  const bothBinaries = binaryAvailable("z3") && binaryAvailable("cvc5");
  const realCross = bothBinaries ? it : it.skip;
  realCross(
    "real z3 + cvc5 both prove forall (x: Int), x = x",
    async () => {
      const db = makeDb();
      mintFormulateMemento(db, "propDD444444dddd", "bindDD444444dddd");

      const manifest = loadProveCrossSolverManifest();
      const { registry, actionRegistry } = registerProveCrossSolverRegistries({ db });

      const workflow = manifestToWorkflow(manifest);
      const runner = new WorkflowRunner(db, workflow, registry);
      const input: ProveCrossSolverWorkflowInput = {
        propertyHash: "propDD444444dddd",
        producedBy: "cross-solver-real@test",
        timeoutMs: 60_000,
      };
      await runManifest(runner, registry, manifest, input, actionRegistry);

      const all = findAll(db, {
        bindingHash: "bindDD444444dddd",
        propertyHash: "propDD444444dddd",
      });
      const memento = all.find((m) => m.producedBy === "cross-solver-real@test");
      expect(memento).toBeDefined();
      expect(memento!.verdict).toBe("holds");

      const witness = JSON.parse(memento!.witness ?? "{}");
      expect(witness.agree).toBe(true);
      expect(witness.z3Verdict).toBe("unsat");
      expect(witness.cvc5Verdict).toBe("unsat");
    },
    120_000,
  );
});
