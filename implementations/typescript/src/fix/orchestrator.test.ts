/**
 * B5: Orchestrator tests.
 *
 * All downstream stages are stubs that throw NotImplementedError.
 * These tests verify argument flow, error propagation, and audit-trail structure
 * WITHOUT needing any real C1-D3 implementation.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
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
      patch: {
        fileEdits: [{ file: "src/processData.ts", newContent: "// fixed" }],
        description: "Add null guard",
      },
      llmRationale: "Add null guard",
      llmConfidence: 0.9,
      invariantHoldsUnderOverlay: true,
      overlayZ3Verdict: "unsat",
      audit: {
        overlayCreated: true,
        patchApplied: true,
        overlayReindexed: true,
        z3RunMs: 10,
        overlayClosed: false,
      },
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

// ---------------------------------------------------------------------------
// #146 + #147: persistence-hygiene tests.
//
// These exercise the orchestrator's `finally`-block flush gating end-to-end.
// They use a real temp git repo (so `resolveProjectRoot` resolves) plus the
// existing vi.mock'd stages to drive specific success/failure shapes.
// ---------------------------------------------------------------------------

describe("orchestrator persistence hygiene (#146 + #147)", () => {
  // Use a fresh temp repo per test so the .provekit/invariants/ assertion
  // sees an isolated directory.
  let repoRoot: string;
  let locusFile: string;

  beforeEach(async () => {
    const fs = await import("fs");
    const os = await import("os");
    const path = await import("path");
    const cp = await import("child_process");
    repoRoot = fs.mkdtempSync(path.join(os.tmpdir(), "provekit-orch-hygiene-"));
    cp.execFileSync("git", ["init", "--initial-branch=main"], { cwd: repoRoot, stdio: "pipe" });
    cp.execFileSync("git", ["config", "user.email", "t@t"], { cwd: repoRoot, stdio: "pipe" });
    cp.execFileSync("git", ["config", "user.name", "t"], { cwd: repoRoot, stdio: "pipe" });
    fs.mkdirSync(path.join(repoRoot, "src"), { recursive: true });
    locusFile = path.join(repoRoot, "src", "buggy.ts");
    fs.writeFileSync(locusFile, "export function buggy(b) { return 1 / b; }\n", "utf-8");
    cp.execFileSync("git", ["add", "."], { cwd: repoRoot, stdio: "pipe" });
    cp.execFileSync("git", ["commit", "-m", "init"], { cwd: repoRoot, stdio: "pipe" });
    vi.clearAllMocks();
  });

  afterEach(async () => {
    const fs = await import("fs");
    fs.rmSync(repoRoot, { recursive: true, force: true });
  });

  function realLocus(): BugLocus {
    return {
      file: locusFile,
      line: 1,
      confidence: 0.9,
      primaryNode: "node-001",
      containingFunction: "node-002",
      relatedFunctions: [],
      dataFlowAncestors: [],
      dataFlowDescendants: [],
      dominanceRegion: [],
      postDominanceRegion: [],
    };
  }

  function realPlan(locus: BugLocus): RemediationPlan {
    return {
      signal: mockSignal,
      locus,
      primaryLayer: "code_invariant",
      secondaryLayers: [],
      artifacts: [],
      rationale: "stub",
    };
  }

  function makeRealMocks() {
    const validInvariant: InvariantClaim = {
      principleId: "div-by-zero",
      description: "divisor must not be zero",
      formalExpression: "(declare-const b Int)\n(assert (= b 0))\n(check-sat)",
      bindings: [
        { smt_constant: "b", source_line: 1, source_expr: "b", sort: "Int" },
      ],
      complexity: 1,
      witness: null,
    };
    const mockOverlay: OverlayHandle = {
      worktreePath: repoRoot, // ok for our test; not used by mocks
      sastDbPath: "",
      sastDb: {} as import("../db/index.js").Db,
      baseRef: "HEAD",
      modifiedFiles: new Set(),
      closed: false,
    };
    const mockFix: FixCandidate = {
      patch: {
        fileEdits: [
          {
            file: locusFile,
            newContent: "export function buggy(b) { if (b === 0) return 0; return 1 / b; }\n",
          },
        ],
        description: "guard divisor",
      },
      llmRationale: "guard divisor",
      llmConfidence: 0.9,
      invariantHoldsUnderOverlay: true,
      overlayZ3Verdict: "unsat",
      audit: {
        overlayCreated: true,
        patchApplied: true,
        overlayReindexed: true,
        z3RunMs: 1,
        overlayClosed: false,
      },
    };
    return { validInvariant, mockOverlay, mockFix };
  }

  it("#146: when D1 fails (bundle === null), the finally-block flush is skipped — no invariant lands on disk", async () => {
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    const { openOverlay } = await import("./stages/openOverlay.js");
    const { generateFixCandidate } = await import("./stages/generateFixCandidate.js");
    const { generateComplementary } = await import("./stages/generateComplementary.js");
    const { generateRegressionTest } = await import("./stages/generateRegressionTest.js");
    const { generatePrincipleCandidate } = await import("./stages/generatePrincipleCandidate.js");
    const { assembleBundle } = await import("./stages/assembleBundle.js");

    const { validInvariant, mockOverlay, mockFix } = makeRealMocks();
    vi.mocked(formulateInvariant).mockImplementationOnce(async () => validInvariant);
    vi.mocked(openOverlay).mockImplementationOnce(async () => mockOverlay);
    vi.mocked(generateFixCandidate).mockImplementationOnce(async () => mockFix);
    vi.mocked(generateComplementary).mockImplementationOnce(async () => [] as ComplementaryChange[]);
    vi.mocked(generateRegressionTest).mockImplementationOnce(async () => null);
    vi.mocked(generatePrincipleCandidate).mockImplementationOnce(async () => []);
    // D1 throws — this is the failure mode that motivated #146.
    vi.mocked(assembleBundle).mockImplementationOnce(async () => {
      throw new Error("D1 oracle gates rejected");
    });

    const locus = realLocus();
    const result = await runFixLoop({
      ...defaultArgs,
      locus,
      plan: realPlan(locus),
    });

    expect(result.applied).toBe(false);
    expect(result.bundle).toBeNull();
    // Critical: nothing should have been persisted.
    const fs = await import("fs");
    const path = await import("path");
    const invariantsDir = path.join(repoRoot, ".provekit", "invariants");
    if (fs.existsSync(invariantsDir)) {
      const files = fs.readdirSync(invariantsDir).filter((n) => n.endsWith(".json"));
      expect(files).toEqual([]);
    }
  });

  it("#146: when D1 succeeds, the early-path flush DOES persist the invariant (regression for the success path)", async () => {
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    const { openOverlay } = await import("./stages/openOverlay.js");
    const { generateFixCandidate } = await import("./stages/generateFixCandidate.js");
    const { generateComplementary } = await import("./stages/generateComplementary.js");
    const { generateRegressionTest } = await import("./stages/generateRegressionTest.js");
    const { generatePrincipleCandidate } = await import("./stages/generatePrincipleCandidate.js");
    const { assembleBundle } = await import("./stages/assembleBundle.js");
    const { applyBundle } = await import("./stages/applyBundle.js");

    const { validInvariant, mockOverlay, mockFix } = makeRealMocks();
    vi.mocked(formulateInvariant).mockImplementationOnce(async () => validInvariant);
    vi.mocked(openOverlay).mockImplementationOnce(async () => mockOverlay);
    vi.mocked(generateFixCandidate).mockImplementationOnce(async () => mockFix);
    vi.mocked(generateComplementary).mockImplementationOnce(async () => [] as ComplementaryChange[]);
    vi.mocked(generateRegressionTest).mockImplementationOnce(async () => null);
    vi.mocked(generatePrincipleCandidate).mockImplementationOnce(async () => []);
    vi.mocked(assembleBundle).mockImplementationOnce(async () => ({
      bundleId: "test-bundle",
      confidence: 0.95,
      auditTrail: [],
      signal: mockSignal,
      plan: realPlan(realLocus()),
      locus: realLocus(),
      fix: mockFix,
      complementary: [],
      test: null,
      principle: null,
      alternateShapes: [],
      overlay: mockOverlay,
      patch: mockFix.patch,
    } as unknown as import("./types.js").FixBundle));
    // D2 throws so the finally block has a chance to fire — but the early
    // path already flushed, so `invariantPersisted` is true and no double
    // write happens.
    vi.mocked(applyBundle).mockImplementationOnce(async () => {
      throw new Error("apply failed");
    });

    const locus = realLocus();
    const result = await runFixLoop({
      ...defaultArgs,
      locus,
      plan: realPlan(locus),
    });

    // D2 threw → result.applied is false, result.bundle still set if
    // captured; but the invariant should be on disk.
    void result;
    const fs = await import("fs");
    const path = await import("path");
    const invariantsDir = path.join(repoRoot, ".provekit", "invariants");
    expect(fs.existsSync(invariantsDir)).toBe(true);
    const files = fs.readdirSync(invariantsDir).filter((n) => n.endsWith(".json"));
    expect(files.length).toBe(1);
  });

  it("#147b: persistence backstop — an empty-bindings claim is refused even if it reaches the orchestrator", async () => {
    // Defense in depth: even if some future code path constructs a claim
    // with `bindings: []` and reaches the orchestrator's flush, the corpus
    // must not gain a malformed entry.
    const { formulateInvariant } = await import("./stages/formulateInvariant.js");
    const { openOverlay } = await import("./stages/openOverlay.js");
    const { generateFixCandidate } = await import("./stages/generateFixCandidate.js");
    const { generateComplementary } = await import("./stages/generateComplementary.js");
    const { generateRegressionTest } = await import("./stages/generateRegressionTest.js");
    const { generatePrincipleCandidate } = await import("./stages/generatePrincipleCandidate.js");
    const { assembleBundle } = await import("./stages/assembleBundle.js");

    const emptyBindingsInvariant: InvariantClaim = {
      principleId: null,
      description: "abstract",
      formalExpression: "(declare-const x Bool)\n(assert (= x true))\n(check-sat)",
      bindings: [],
      complexity: 1,
      witness: null,
    };
    const { mockOverlay, mockFix } = makeRealMocks();
    vi.mocked(formulateInvariant).mockImplementationOnce(async () => emptyBindingsInvariant);
    vi.mocked(openOverlay).mockImplementationOnce(async () => mockOverlay);
    vi.mocked(generateFixCandidate).mockImplementationOnce(async () => mockFix);
    vi.mocked(generateComplementary).mockImplementationOnce(async () => [] as ComplementaryChange[]);
    vi.mocked(generateRegressionTest).mockImplementationOnce(async () => null);
    vi.mocked(generatePrincipleCandidate).mockImplementationOnce(async () => []);
    vi.mocked(assembleBundle).mockImplementationOnce(async () => ({
      bundleId: "test-bundle",
      confidence: 0.95,
      auditTrail: [],
      signal: mockSignal,
      plan: realPlan(realLocus()),
      locus: realLocus(),
      fix: mockFix,
      complementary: [],
      test: null,
      principle: null,
      alternateShapes: [],
      overlay: mockOverlay,
      patch: mockFix.patch,
    } as unknown as import("./types.js").FixBundle));

    const locus = realLocus();
    const result = await runFixLoop({
      ...defaultArgs,
      locus,
      plan: realPlan(locus),
    });
    void result;

    // Even though D1 succeeded and the early-path flush ran, the
    // empty-bindings backstop must have refused to write.
    const fs = await import("fs");
    const path = await import("path");
    const invariantsDir = path.join(repoRoot, ".provekit", "invariants");
    if (fs.existsSync(invariantsDir)) {
      const files = fs.readdirSync(invariantsDir).filter((n) => n.endsWith(".json"));
      expect(files).toEqual([]);
    }
  });
});
