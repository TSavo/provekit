/**
 * OpenOverlay stage tests. Mocks openOverlay() because the real
 * implementation needs a live git repo + tree-sitter SAST + sqlite
 * migrations on disk — overkill for the Stage-wrapper smoke test.
 *
 * The load-bearing claim is the cache contract is DEFEATED: identical
 * logical input must miss cache every time, producing a fresh
 * OverlayHandle on each call. The salt in serializeInput drives a
 * fresh propertyHash for every invocation, so the runner always falls
 * through to run().
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { openOverlayMock } = vi.hoisted(() => ({ openOverlayMock: vi.fn() }));
vi.mock("../../fix/stages/openOverlay.js", () => ({
  openOverlay: openOverlayMock,
}));

import type { BugLocus, OverlayHandle } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeOpenOverlayStage,
  OPEN_OVERLAY_CAPABILITY,
  type OpenOverlayStageInput,
} from "./openOverlay.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "open-overlay-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-open-overlay-test-v1" };

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

let overlayCounter = 0;
function buildFakeOverlay(): OverlayHandle {
  overlayCounter += 1;
  const tmpPath = join(tmpdir(), `fake-overlay-${overlayCounter}-${Date.now()}`);
  return {
    worktreePath: tmpPath,
    sastDbPath: join(tmpPath, "sast.db"),
    sastDb: openDb(join(tmpdir(), `sast-stub-${overlayCounter}-${Date.now()}.db`)),
    baseRef: "abc123",
    modifiedFiles: new Set(),
    closed: false,
  };
}

describe("openOverlay Stage", () => {
  beforeEach(() => {
    openOverlayMock.mockReset();
    openOverlayMock.mockImplementation(async () => buildFakeOverlay());
    overlayCounter = 0;
  });

  it("runs through WorkflowRunner.runStage and returns an OverlayHandle", async () => {
    const db = makeDb();
    const stage = makeOpenOverlayStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { locus: fakeLocus });

    expect(result.cacheHit).toBe(false);
    expect(result.output.baseRef).toBe("abc123");
    expect(result.output.worktreePath).toMatch(/fake-overlay-/);
    expect(openOverlayMock).toHaveBeenCalledTimes(1);
    const callArg = openOverlayMock.mock.calls[0][0];
    expect(callArg.locus).toBe(fakeLocus);
    expect(callArg.db).toBe(db);
  });

  it("cache is DEFEATED — second call with identical input produces a fresh worktree", async () => {
    const db = makeDb();
    const stage = makeOpenOverlayStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { locus: fakeLocus });
    const b = await runner.runStage(stage, { locus: fakeLocus });

    // Both calls must MISS — neither is a cache hit, both invoked run().
    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    // Different memento rows — the salt drives different propertyHash.
    expect(b.cid).not.toBe(a.cid);
    // Distinct OverlayHandles — different worktree paths.
    expect(b.output.worktreePath).not.toBe(a.output.worktreePath);
    expect(openOverlayMock).toHaveBeenCalledTimes(2);
  });

  it("dispatches via the registry as capability 'openOverlay'", async () => {
    const db = makeDb();
    const stage = makeOpenOverlayStage({ db });
    const registry = new InMemoryRegistry();
    registry.register(OPEN_OVERLAY_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<OpenOverlayStageInput, OverlayHandle>(
      OPEN_OVERLAY_CAPABILITY,
      { locus: fakeLocus },
    );

    expect(result.output.baseRef).toBe("abc123");
    expect(result.output.worktreePath).toMatch(/fake-overlay-/);
  });

  it("deserializeOutput throws — accidental cache reconstruction must fail loudly", () => {
    const db = makeDb();
    const stage = makeOpenOverlayStage({ db });

    expect(() =>
      stage.deserializeOutput(
        JSON.stringify({
          worktreePath: "/tmp/foo",
          sastDbPath: "/tmp/foo/sast.db",
          baseRef: "abc123",
        }),
      ),
    ).toThrow(/cache reconstruction not supported/);
  });
});
