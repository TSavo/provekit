/**
 * B5: Orchestrator tests.
 *
 * All downstream stages are stubs that throw NotImplementedError.
 * These tests verify argument flow, error propagation, and audit-trail structure
 * WITHOUT needing any real C1-D3 implementation.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { runFixLoop } from "./orchestrator.js";
import { NotImplementedError } from "./types.js";
import type { BugSignal, BugLocus, RemediationPlan, LLMProvider, InvariantClaim, OverlayHandle, FixCandidate, ComplementaryChange } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// vi.mock: replace all stage stubs with controllable implementations.
// Default: throw NotImplementedError (mirrors real stub behaviour).
// Individual tests can override via mockImplementationOnce / mockImplementation.
// ---------------------------------------------------------------------------

vi.mock("./stages/formulateInvariant.js", () => ({
  formulateInvariant: vi.fn(async () => {
    throw new NotImplementedError("C1", "formulateInvariant (C1) not yet implemented");
  }),
}));

vi.mock("./stages/openOverlay.js", () => ({
  openOverlay: vi.fn(async () => {
    throw new NotImplementedError("C2", "openOverlay (C2) not yet implemented");
  }),
}));

vi.mock("./stages/generateFixCandidate.js", () => ({
  generateFixCandidate: vi.fn(async () => {
    throw new NotImplementedError("C3", "generateFixCandidate (C3) not yet implemented");
  }),
}));

vi.mock("./stages/generateComplementary.js", () => ({
  generateComplementary: vi.fn(async () => {
    throw new NotImplementedError("C4", "generateComplementary (C4) not yet implemented");
  }),
}));

vi.mock("./stages/generateRegressionTest.js", () => ({
  generateRegressionTest: vi.fn(async () => {
    throw new NotImplementedError("C5", "generateRegressionTest (C5) not yet implemented");
  }),
}));

vi.mock("./stages/generatePrincipleCandidate.js", () => ({
  generatePrincipleCandidate: vi.fn(async () => {
    throw new NotImplementedError("C6", "generatePrincipleCandidate (C6) not yet implemented");
  }),
}));

vi.mock("./stages/assembleBundle.js", () => ({
  assembleBundle: vi.fn(async () => {
    throw new NotImplementedError("D1", "assembleBundle (D1) not yet implemented");
  }),
}));

vi.mock("./stages/applyBundle.js", () => ({
  applyBundle: vi.fn(async () => {
    throw new NotImplementedError("D2", "applyBundle (D2) not yet implemented");
  }),
}));

vi.mock("./stages/learnFromBundle.js", () => ({
  learnFromBundle: vi.fn(async () => {
    throw new NotImplementedError("D3", "learnFromBundle (D3) not yet implemented");
  }),
}));

// ---------------------------------------------------------------------------
// Shared test fixtures
// ---------------------------------------------------------------------------

const mockSignal: BugSignal = {
  source: "test-adapter",
  rawText: "TypeError: cannot read property 'x' of undefined",
  summary: "Null dereference in processData",
  failureDescription: "processData throws when input is undefined",
  codeReferences: [{ file: "src/processData.ts", line: 42 }],
};

const mockLocus: BugLocus = {
  file: "src/processData.ts",
  line: 42,
  confidence: 0.9,
  primaryNode: "node-001",
  containingFunction: "node-002",
  relatedFunctions: [],
  dataFlowAncestors: [],
  dataFlowDescendants: [],
  dominanceRegion: [],
  postDominanceRegion: [],
};

const mockPlan: RemediationPlan = {
  signal: mockSignal,
  locus: mockLocus,
  primaryLayer: "null-check",
  secondaryLayers: [],
  artifacts: [],
  rationale: "Add null guard before accessing .x",
};

const mockDb = {} as unknown as Db;
const mockLlm: LLMProvider = {
  async complete() { return "stub"; },
};

const defaultArgs = {
  signal: mockSignal,
  locus: mockLocus,
  plan: mockPlan,
  db: mockDb,
  llm: mockLlm,
  options: {
    autoApply: false,
    maxComplementarySites: 10,
    confidenceThreshold: 0.8,
  },
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("orchestrator.runFixLoop", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset all mocks to their default (NotImplementedError-throwing) state.
  });

  it("test 1: C1 throws NotImplementedError → graceful abort with skipped audit entry", async () => {
    const result = await runFixLoop(defaultArgs);

    expect(result.applied).toBe(false);
    expect(result.bundle).toBeNull();
    expect(result.reason).toMatch(/C1/);

    // Must have a "start" entry for C1
    const c1Start = result.auditTrail.find((e) => e.stage === "C1" && e.kind === "start");
    expect(c1Start).toBeDefined();

    // Must have a "skipped" entry (graceful abort, not "error")
    const skipped = result.auditTrail.find((e) => e.kind === "skipped");
    expect(skipped).toBeDefined();
    expect(skipped?.stage).toBe("C1");

    // Must NOT have an "error" entry (error kind is for non-NotImplemented throws)
    const errorEntry = result.auditTrail.find((e) => e.kind === "error");
    expect(errorEntry).toBeUndefined();
  });

  it("test 2: audit trail is chronological (monotonically non-decreasing timestamps)", async () => {
    const result = await runFixLoop(defaultArgs);

    const timestamps = result.auditTrail.map((e) => e.timestamp);
    for (let i = 1; i < timestamps.length; i++) {
      expect(timestamps[i]).toBeGreaterThanOrEqual(timestamps[i - 1]!);
    }
  });

  it("test 3: signal and locus flow correctly into formulateInvariant (C1)", async () => {
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    const spy = vi.mocked(formulateInvariant);

    // Default throws NotImplementedError — that's fine, we just want the args.
    await runFixLoop(defaultArgs);

    expect(spy).toHaveBeenCalledOnce();
    const receivedArgs = spy.mock.calls[0]![0];
    expect(receivedArgs.signal).toBe(mockSignal);
    expect(receivedArgs.locus).toBe(mockLocus);
    expect(receivedArgs.db).toBe(mockDb);
    expect(receivedArgs.llm).toBe(mockLlm);
  });

  it("test 4: non-NotImplemented error is caught, recorded as 'error' audit entry", async () => {
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    vi.mocked(formulateInvariant).mockImplementationOnce(async () => {
      throw new Error("unexpected database failure");
    });

    const result = await runFixLoop(defaultArgs);

    expect(result.applied).toBe(false);
    expect(result.bundle).toBeNull();
    expect(result.reason).toMatch(/unexpected database failure/);

    const errorEntry = result.auditTrail.find((e) => e.kind === "error");
    expect(errorEntry).toBeDefined();
    expect(errorEntry?.detail).toMatch(/unexpected database failure/);

    // Error is recorded under the failing stage (C1), not under "orchestrator"
    expect(errorEntry?.stage).toBe("C1");
  });

  it("test 5: maxComplementarySites option propagates to generateComplementary (C4)", async () => {
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    const { openOverlay } = await import("./stages/openOverlay.js");
    const { generateFixCandidate } = await import("./stages/generateFixCandidate.js");
    const { generateComplementary } = await import("./stages/generateComplementary.js");

    const mockInvariant: InvariantClaim = {
      principleId: "null-deref",
      description: "Value must be non-null before dereference",
      formalExpression: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
      bindings: [],
      complexity: 0,
      witness: null,
    };
    const mockOverlay: OverlayHandle = {
      worktreePath: "/tmp/overlay",
      sastDbPath: "/tmp/overlay/sast.db",
      sastDb: {} as import("../db/index.js").Db,
      baseRef: "HEAD",
      modifiedFiles: new Set(),
      closed: false,
    };
    const mockFix: FixCandidate = {
      file: "src/processData.ts",
      patch: "diff...",
      rationale: "Add null guard",
      confidence: 0.9,
    };

    vi.mocked(formulateInvariant).mockImplementationOnce(async () => mockInvariant);
    vi.mocked(openOverlay).mockImplementationOnce(async () => mockOverlay);
    vi.mocked(generateFixCandidate).mockImplementationOnce(async () => mockFix);
    // generateComplementary still throws NotImplementedError — that's our abort point.

    const argsWithCustomSites = {
      ...defaultArgs,
      options: { ...defaultArgs.options, maxComplementarySites: 20 },
    };

    const result = await runFixLoop(argsWithCustomSites);

    // Should abort at C4
    expect(result.applied).toBe(false);
    expect(result.reason).toMatch(/C4/);

    const c4Spy = vi.mocked(generateComplementary);
    expect(c4Spy).toHaveBeenCalledOnce();
    const c4Args = c4Spy.mock.calls[0]![0];
    expect(c4Args.maxSites).toBe(20);
    expect(c4Args.fix).toBe(mockFix);
    expect(c4Args.locus).toBe(mockLocus);
  });
});
