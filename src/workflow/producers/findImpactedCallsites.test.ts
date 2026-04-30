/**
 * find-impacted-callsites stage tests. Builds a real
 * `.provekit/invariants/` corpus via writeInvariant() and asserts the
 * Stage flags only invariants whose id (== propertyHash) appears in
 * the catalog diff's Removed or Modified set.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import {
  writeInvariant,
  type StoredInvariant,
} from "../../fix/runtime/invariantStore.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeFindImpactedCallsitesStage,
  FIND_IMPACTED_CALLSITES_CAPABILITY,
  type FindImpactedCallsitesResult,
  type FindImpactedCallsitesStageInput,
} from "./findImpactedCallsites.js";
import type {
  CatalogDeclaration,
  LoadCatalogResult,
} from "./loadCatalog.js";
import type { DiffCatalogsResult } from "./diffCatalogs.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "find-impacts-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

function makeInv(id: string): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: `bug-${id}`,
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (not (= x 0)))",
    },
    bindings: [
      {
        type: "local",
        smt_constant: "x",
        source_expr: "denominator",
        sort: "Int",
        node: {
          filePath: "src/m.ts",
          nodeHash: "h",
          startLine: 1,
          endLine: 1,
        },
      },
    ],
    callsite: {
      filePath: "src/m.ts",
      function: "divide",
      startLine: 12,
      endLine: 12,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

function diffWith(removed: string[], modified: Array<[string, string]>): DiffCatalogsResult {
  const decl = (h: string): CatalogDeclaration => ({
    cid: `c-${h}`,
    propertyHash: h,
    bindingHash: `b-${h}`,
    producedBy: "test@1",
    name: null,
  });
  return {
    oldFound: true,
    newFound: true,
    oldProofHash: "old",
    newProofHash: "new",
    added: [],
    removed: removed.map(decl),
    modified: modified.map(([oh, nh]) => ({
      name: oh,
      oldPropertyHash: oh,
      newPropertyHash: nh,
    })),
    identical: false,
  };
}

const wf = { name: "test-wf", cid: "wf-find-impacts-test-v1" };

describe("find-impacted-callsites Stage", () => {
  it("flags invariants whose id appears in the Removed list", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInv("aa"));
    writeInvariant(projectRoot, makeInv("bb"));
    const stage = makeFindImpactedCallsitesStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      projectRoot,
      diff: diffWith(["aa"], []),
    });

    expect(output.scanned).toBe(2);
    expect(output.impacted).toHaveLength(1);
    expect(output.impacted[0].invariantId).toBe("aa");
    expect(output.impacted[0].reason).toBe("removed");
    expect(output.impacted[0].newPropertyHash).toBeNull();
    expect(output.matchStrategy).toBe("propertyHash-collision-v1");
  });

  it("flags invariants whose id appears in the Modified list with newPropertyHash threaded through", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInv("aa"));
    const stage = makeFindImpactedCallsitesStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      projectRoot,
      diff: diffWith([], [["aa", "aaprime"]]),
    });

    expect(output.impacted).toHaveLength(1);
    expect(output.impacted[0].reason).toBe("modified");
    expect(output.impacted[0].newPropertyHash).toBe("aaprime");
  });

  it("returns no impact when project invariants don't intersect the diff", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInv("aa"));
    const stage = makeFindImpactedCallsitesStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      projectRoot,
      diff: diffWith(["bb"], []),
    });

    expect(output.scanned).toBe(1);
    expect(output.impacted).toEqual([]);
  });

  it("caches identical input — second run is a hit", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInv("aa"));
    const stage = makeFindImpactedCallsitesStage();
    const runner = new WorkflowRunner(db, wf);
    const diff = diffWith(["aa"], []);

    const a = await runner.runStage(stage, { projectRoot, diff });
    const b = await runner.runStage(stage, { projectRoot, diff });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'find-impacted-callsites'", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const stage = makeFindImpactedCallsitesStage();
    const registry = new InMemoryRegistry();
    registry.register(FIND_IMPACTED_CALLSITES_CAPABILITY, stage);
    const runner = new WorkflowRunner(db, wf, registry);

    const result = await runner.request<
      FindImpactedCallsitesStageInput,
      FindImpactedCallsitesResult
    >(FIND_IMPACTED_CALLSITES_CAPABILITY, {
      projectRoot,
      diff: diffWith([], []),
    });

    expect(result.output.scanned).toBe(0);
  });
});
