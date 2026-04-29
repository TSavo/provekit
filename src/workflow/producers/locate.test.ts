/**
 * Locate stage tests. Mocks the underlying locate() to verify the
 * Stage wrapper without requiring a populated SAST DB. The locate()
 * function has its own coverage of the actual ranking + fallback
 * logic.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { locateMock } = vi.hoisted(() => ({ locateMock: vi.fn() }));
vi.mock("../../fix/locate.js", () => ({ locate: locateMock }));

import type { BugLocus, IntentSignal } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import { makeLocateStage, LOCATE_CAPABILITY } from "./locate.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "locate-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-locate-test-v1" };

const fakeSignal: IntentSignal = {
  source: "test",
  rawText: "denominator may be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  bugClassHint: "division-by-zero",
};

const fakeLocus: BugLocus = {
  file: "src/math.ts",
  line: 1,
  confidence: 1.0,
  primaryNode: "node-1",
  containingFunction: "node-1",
  relatedFunctions: [],
  dataFlowAncestors: [],
  dataFlowDescendants: [],
  dominanceRegion: [],
  postDominanceRegion: [],
};

describe("locate Stage", () => {
  beforeEach(() => {
    locateMock.mockReset();
    locateMock.mockReturnValue(fakeLocus);
  });

  it("runs through WorkflowRunner.runStage and returns the BugLocus", async () => {
    const db = makeDb();
    const stage = makeLocateStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { signal: fakeSignal });

    expect(result.cacheHit).toBe(false);
    expect(result.output?.file).toBe("src/math.ts");
    expect(locateMock).toHaveBeenCalledTimes(1);
    expect(locateMock).toHaveBeenCalledWith(db, fakeSignal);
  });

  it("returns null when locate() finds no candidate", async () => {
    const db = makeDb();
    locateMock.mockReturnValue(null);
    const stage = makeLocateStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { signal: fakeSignal });
    expect(result.output).toBeNull();
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const stage = makeLocateStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal });
    const b = await runner.runStage(stage, { signal: fakeSignal });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(locateMock).toHaveBeenCalledTimes(1);
  });

  it("dispatches via the registry as capability 'locate'", async () => {
    const db = makeDb();
    const stage = makeLocateStage({ db });
    const registry = new InMemoryRegistry();
    registry.register(LOCATE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<{ signal: IntentSignal }, BugLocus | null>(
      LOCATE_CAPABILITY,
      { signal: fakeSignal },
    );

    expect(result.output?.primaryNode).toBe("node-1");
  });
});
