/**
 * Bundle stage tests. Mocks the underlying assembleBundle() to verify
 * the Stage cache contract without spinning up Oracle #10's full test
 * suite execution.
 *
 * Key invariants tested:
 *   - the FixBundle round-trips through the witness column (artifacts
 *     AND coherence verdicts intact);
 *   - same content inputs → same cache slot regardless of overlay path;
 *   - cache hit reconstructs the coherence verdicts so downstream
 *     consumers don't re-run Oracle #10.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { assembleBundleMock } = vi.hoisted(() => ({
  assembleBundleMock: vi.fn(),
}));
vi.mock("../../fix/stages/assembleBundle.js", () => ({
  assembleBundle: assembleBundleMock,
}));

import type {
  BugLocus,
  FixBundle,
  FixCandidate,
  IntentSignal,
  OverlayHandle,
  RemediationPlan,
  TestArtifact,
} from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeBundleStage,
  BUNDLE_CAPABILITY,
  type BundleStageInput,
} from "./bundle.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "bundle-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-bundle-test-v1" };

const fakeSignal: IntentSignal = {
  source: "test",
  rawText: "denominator may be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  bugClassHint: "division-by-zero",
};

const fakePlan: RemediationPlan = {
  signal: fakeSignal,
  locus: null,
  primaryLayer: "input-validation",
  secondaryLayers: [],
  artifacts: [],
  rationale: "guard the denominator",
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

const fakeFix: FixCandidate = {
  patch: { fileEdits: [] } as FixCandidate["patch"],
  source: "llm",
  llmRationale: "guard the denominator",
  llmConfidence: 0.9,
  invariantHoldsUnderOverlay: true,
  overlayZ3Verdict: "unsat",
  audit: {
    overlayCreated: true,
    patchApplied: true,
    overlayReindexed: true,
    z3RunMs: 42,
    overlayClosed: false,
  },
};

const fakeTest: TestArtifact = {
  source: "llm",
  testFilePath: "src/math.regression.test.ts",
  testName: "divide handles zero denominator",
  testCode: "/* test */",
  witnessInputs: { a: 1, b: 0 },
  passesOnFixedCode: true,
  failsOnOriginalCode: true,
  audit: {
    fixedRunStdout: "",
    fixedRunExitCode: 0,
    originalRunStdout: "",
    originalRunExitCode: 1,
    mutationApplied: true,
    mutationReverted: true,
  },
};

function fakeOverlay(worktreePath: string, baseRef: string): OverlayHandle {
  return {
    worktreePath,
    sastDbPath: join(worktreePath, "sast.db"),
    sastDb: openDb(join(tmpdir(), `unused-${Date.now()}-${Math.random()}.db`)),
    baseRef,
    modifiedFiles: new Set(),
    closed: false,
  };
}

const fakeBundle: FixBundle = {
  bundleId: 1,
  bundleType: "fix",
  bugSignal: fakeSignal,
  plan: fakePlan,
  artifacts: {
    primaryFix: fakeFix,
    complementary: [],
    test: fakeTest,
    principle: null,
    capabilitySpec: null,
  },
  coherence: {
    sastStructural: true,
    z3SemanticConsistency: true,
    fullSuiteGreen: true,
    noNewGapsIntroduced: true,
    migrationSafe: null,
    crossCodebaseRegression: null,
    extractorCoverage: null,
    substrateConsistency: null,
    principleNeedsCapability: null,
  },
  confidence: 0.9,
  auditTrail: [],
} as FixBundle;

function bundleInput(overlay: OverlayHandle): BundleStageInput {
  return {
    signal: fakeSignal,
    plan: fakePlan,
    locus: fakeLocus,
    fix: fakeFix,
    complementary: [],
    test: fakeTest,
    principle: null,
    overlay,
  };
}

describe("bundle Stage", () => {
  beforeEach(() => {
    assembleBundleMock.mockReset();
    assembleBundleMock.mockResolvedValue(fakeBundle);
  });

  it("runs through WorkflowRunner.runStage and returns the FixBundle", async () => {
    const db = makeDb();
    const stage = makeBundleStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(
      stage,
      bundleInput(fakeOverlay("/tmp/overlay-a", "abc123")),
    );

    expect(result.cacheHit).toBe(false);
    expect(result.output.coherence.sastStructural).toBe(true);
    expect(result.output.coherence.z3SemanticConsistency).toBe(true);
    expect(result.output.coherence.fullSuiteGreen).toBe(true);
    expect(result.output.artifacts.primaryFix?.invariantHoldsUnderOverlay).toBe(true);
    expect(assembleBundleMock).toHaveBeenCalledTimes(1);
  });

  it("memento captures the unit of work — coherence verdicts round-trip", async () => {
    const db = makeDb();
    const stage = makeBundleStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(
      stage,
      bundleInput(fakeOverlay("/tmp/overlay-a", "abc123")),
    );
    const b = await runner.runStage(
      stage,
      bundleInput(fakeOverlay("/tmp/overlay-a", "abc123")),
    );

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    // Coherence verdicts ARE the memento — downstream skips Oracle #10
    // by reading from witness rather than re-executing.
    expect(b.output.coherence.sastStructural).toBe(true);
    expect(b.output.coherence.z3SemanticConsistency).toBe(true);
    expect(b.output.coherence.fullSuiteGreen).toBe(true);
    expect(b.output.coherence.noNewGapsIntroduced).toBe(true);
    expect(assembleBundleMock).toHaveBeenCalledTimes(1);
  });

  it("same baseRef + different worktree paths hit the same cache slot", async () => {
    const db = makeDb();
    const stage = makeBundleStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(
      stage,
      bundleInput(fakeOverlay("/tmp/overlay-A", "abc123")),
    );
    const b = await runner.runStage(
      stage,
      bundleInput(fakeOverlay("/tmp/overlay-B-different-path", "abc123")),
    );

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(assembleBundleMock).toHaveBeenCalledTimes(1);
  });

  it("dispatches via the registry as capability 'bundle'", async () => {
    const db = makeDb();
    const stage = makeBundleStage({ db });
    const registry = new InMemoryRegistry();
    registry.register(BUNDLE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<BundleStageInput, FixBundle>(
      BUNDLE_CAPABILITY,
      bundleInput(fakeOverlay("/tmp/overlay", "abc123")),
    );

    expect(result.output.coherence.fullSuiteGreen).toBe(true);
  });
});
