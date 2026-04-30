/**
 * End-to-end smoke for the bug-fix workflow (task #7).
 *
 * Drives the on-disk `bug-fix.workflow.yaml` against a real fixture
 * project (a temp git repo with a known divide-by-zero bug). Every
 * producer wrapper runs — Stage and Action serialization, memento
 * write/read, runner topo-sort, $action resource flow, $node deep
 * references — but the heavy inner functions (formulateInvariant,
 * doTheWork, generateComplementary, generatePrincipleCandidate,
 * assembleBundle) are mocked at the module boundary so the smoke stays
 * offline (no Z3, no vitest invocation, no real LLM agent).
 *
 * What's exercised for real:
 *   - intake (StubLLMProvider with prefix-keyed responses)
 *   - investigate (real prompt + stub LLM response)
 *   - locate (real SAST queries against a populated DB)
 *   - classify (real prompt + stub LLM response)
 *   - recognize (real DSL evaluator; no principles dir → empty match)
 *   - openOverlay (real `git worktree add --detach`; that's the point of
 *     having a real git repo as the fixture)
 *   - The full memento DAG: every stage's verdict-bearing memento, the
 *     action's audit-only memento, and the workflow-level wrapper.
 *
 * What's mocked at the module boundary:
 *   - formulateInvariant (skips Z3)
 *   - doTheWork (skips agent invocation, Oracle #2, Oracle #9, vitest)
 *   - generateComplementary (skips LLM + SAST traversal — returns [])
 *   - generatePrincipleCandidate (skips LLM substrate path — returns [])
 *   - assembleBundle (skips Oracle #10 + DB persistence — returns canned
 *     FixBundle stub)
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

// Module-boundary mocks for the heavy inner functions. The producer
// wrappers (makeFormulateStage, makeDoTheWorkStage, ...) still run —
// serialize, hash, write/read mementos — they just don't invoke Z3 or
// the real agent.
const {
  formulateInvariantMock,
  doTheWorkMock,
  generateComplementaryMock,
  generatePrincipleCandidateMock,
  assembleBundleMock,
} = vi.hoisted(() => ({
  formulateInvariantMock: vi.fn(),
  doTheWorkMock: vi.fn(),
  generateComplementaryMock: vi.fn(),
  generatePrincipleCandidateMock: vi.fn(),
  assembleBundleMock: vi.fn(),
}));
vi.mock("../fix/stages/formulateInvariant.js", () => ({
  formulateInvariant: formulateInvariantMock,
}));
vi.mock("../fix/stages/doTheWork.js", () => ({
  doTheWork: doTheWorkMock,
}));
vi.mock("../fix/stages/generateComplementary.js", () => ({
  generateComplementary: generateComplementaryMock,
}));
vi.mock("../fix/stages/generatePrincipleCandidate.js", () => ({
  generatePrincipleCandidate: generatePrincipleCandidateMock,
}));
vi.mock("../fix/stages/assembleBundle.js", () => ({
  assembleBundle: assembleBundleMock,
}));

import { openDb, type Db } from "../db/index.js";
import { _clearIntakeRegistry } from "../fix/intake.js";
import { registerAll } from "../fix/intakeAdapters/index.js";
import { StubLLMProvider } from "../fix/types.js";
import type {
  ComplementaryChange,
  FixBundle,
  FixCandidate,
  InvariantClaim,
  PrincipleCandidate,
  TestArtifact,
} from "../fix/types.js";
import type { DoTheWorkResult } from "../fix/stages/doTheWork.js";
import { stats as mementoStats } from "../fix/runtime/mementoStore.js";
import { buildSASTForFile } from "../sast/builder.js";
import { WorkflowRunner } from "../workflow/runner.js";
import {
  runManifest,
  manifestToWorkflow,
} from "../workflow/manifest.js";
import {
  loadBugFixManifest,
  registerBugFixRegistries,
} from "../workflows/bug-fix.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

// ---------------------------------------------------------------------------
// Fixture: a real temp git repo with a known divide-by-zero bug.
// openOverlay does `git worktree add --detach`, so the fixture must be a
// real repo with at least one commit.
// ---------------------------------------------------------------------------

interface Fixture {
  projectRoot: string;
  db: Db;
  /** Absolute path to the locus file (passed into intake). */
  locusFileAbs: string;
  /** Project-relative locus path (for reporting / human readability). */
  locusFileRel: string;
}

function git(repoRoot: string, args: string[]): void {
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf-8",
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "smoke",
      GIT_AUTHOR_EMAIL: "smoke@example.com",
      GIT_COMMITTER_NAME: "smoke",
      GIT_COMMITTER_EMAIL: "smoke@example.com",
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
}

function makeDivideByZeroFixture(): Fixture {
  const projectRoot = mkdtempSync(join(tmpdir(), "bugfix-smoke-divzero-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  mkdirSync(join(projectRoot, "src"), { recursive: true });

  const locusFileRel = "src/math.ts";
  const locusFileAbs = join(projectRoot, locusFileRel);
  writeFileSync(
    locusFileAbs,
    [
      "export function calculate(numerator: number, denominator: number): number {",
      "  return numerator / denominator;",
      "}",
      "",
    ].join("\n"),
    "utf-8",
  );

  // Real git repo. openOverlay calls `git worktree add --detach HEAD`,
  // so we need init + one commit to give it a HEAD to detach from.
  git(projectRoot, ["init", "--initial-branch=main"]);
  git(projectRoot, ["add", "."]);
  git(projectRoot, ["commit", "-m", "initial fixture"]);

  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  buildSASTForFile(db, locusFileAbs);

  return { projectRoot, db, locusFileAbs, locusFileRel };
}

// ---------------------------------------------------------------------------
// Stub LLM — keyed by prompt prefix substrings emitted by each stage.
// ---------------------------------------------------------------------------

function buildStubLLM(fixture: Fixture): StubLLMProvider {
  // intake codeReferences MUST point at the absolute path so that
  // openOverlay's `dirname(locus.file)` resolves to a directory inside
  // the git repo. The locate stage's resolveFile() does suffix matching,
  // so absolute paths still resolve against the SAST DB.
  const intakeJson = JSON.stringify({
    summary: "Division crashes when denominator is 0.",
    failureDescription: "Division-by-zero in calculate.",
    fixHint: "Guard before dividing.",
    codeReferences: [
      { file: fixture.locusFileAbs, line: 2, function: "calculate" },
    ],
    bugClassHint: "divide-by-zero",
  });

  const investigateJson = JSON.stringify({
    symptomSummary: "Division crashes when denominator is 0.",
    rootCauseHypothesis:
      "calculate() does not check that denominator is non-zero before dividing.",
    fixHypothesis:
      "Throw or return a sentinel when denominator === 0.",
    primaryLocation: {
      file: fixture.locusFileAbs,
      function: "calculate",
      lineRange: [1, 3],
      rationale: "The locus function is the only candidate site.",
      confidence: "high",
    },
    candidateLocations: [],
  });

  const classifyJson = JSON.stringify({
    primaryLayer: "code_invariant",
    secondaryLayers: [],
    artifacts: [
      {
        kind: "code-patch",
        rationale: "Patch the locus function in place.",
      },
    ],
    rationale: "The intent maps cleanly onto the locus function.",
  });

  return new StubLLMProvider(
    new Map<string, string>([
      ["You are a bug-report parser", intakeJson],
      ["You are the Investigate stage", investigateJson],
      ["You are classifying an intent", classifyJson],
    ]),
  );
}

// ---------------------------------------------------------------------------
// Canned outputs for the mocked inner stages.
// ---------------------------------------------------------------------------

function fakeInvariantClaim(file: string): InvariantClaim {
  return {
    principleId: null,
    description: "denominator must not be zero",
    formalExpression: "(declare-const b Int) (assert (not (= b 0)))",
    bindings: [
      {
        smt_constant: "b",
        source_expr: "denominator",
        file,
        line: 2,
      } as InvariantClaim["bindings"][number],
    ],
    complexity: 3,
    witness: "(model (define-fun b () Int 1))",
    source: "llm",
  };
}

function fakeFixCandidate(): FixCandidate {
  return {
    patch: {
      fileEdits: [
        {
          file: "src/math.ts",
          newContent:
            "export function calculate(numerator: number, denominator: number): number {\n" +
            "  if (denominator === 0) throw new Error('denominator must not be zero');\n" +
            "  return numerator / denominator;\n" +
            "}\n",
        },
      ],
      description: "guard the division",
    },
    source: "llm",
    llmRationale: "Add a divide-by-zero guard.",
    llmConfidence: 1.0,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      stage: "C3",
      kind: "fix-candidate",
      verdict: "unsat",
      verdictHash: "abc123",
      patchHash: "def456",
      timestamp: Date.now(),
    } as FixCandidate["audit"],
  };
}

function fakeTestArtifact(): TestArtifact {
  return {
    source: "llm",
    testFilePath: "src/math.regression.test.ts",
    testName: "regression: divide-by-zero",
    testCode:
      'import { test, expect } from "vitest";\n' +
      'import { calculate } from "./math.js";\n' +
      'test("denominator zero throws", () => {\n' +
      "  expect(() => calculate(1, 0)).toThrow();\n" +
      "});\n",
    witnessInputs: [],
    passesOnFixedCode: true,
    failsOnOriginalCode: true,
    audit: {
      stage: "C5",
      kind: "regression-test",
      verdict: "ok",
      verdictHash: "ghi789",
      patchHash: "def456",
      timestamp: Date.now(),
    } as TestArtifact["audit"],
  };
}

function fakeDoTheWorkResult(): DoTheWorkResult {
  return {
    fix: fakeFixCandidate(),
    test: fakeTestArtifact(),
    rationale: "Add a divide-by-zero guard and lock it in with a test.",
    turnsUsed: 1,
  };
}

function fakeFixBundle(): FixBundle {
  return {
    bundleId: "bundle-smoke-1",
    fix: fakeFixCandidate(),
    test: fakeTestArtifact(),
    complementary: [],
    principle: null,
    alternateShapes: [],
    coherence: {
      sastStructural: "passed",
      z3SemanticConsistency: "passed",
      fullSuiteGreen: "passed",
    } as unknown as FixBundle["coherence"],
    auditTrail: [],
    timestamp: Date.now(),
  } as unknown as FixBundle;
}

// ---------------------------------------------------------------------------
// Test setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  _clearIntakeRegistry();
  registerAll();
  formulateInvariantMock.mockReset();
  doTheWorkMock.mockReset();
  generateComplementaryMock.mockReset();
  generatePrincipleCandidateMock.mockReset();
  assembleBundleMock.mockReset();
});

function primeMocks(fixture: Fixture): void {
  formulateInvariantMock.mockResolvedValue(fakeInvariantClaim(fixture.locusFileAbs));
  doTheWorkMock.mockResolvedValue(fakeDoTheWorkResult());
  generateComplementaryMock.mockResolvedValue([] as ComplementaryChange[]);
  generatePrincipleCandidateMock.mockResolvedValue([] as PrincipleCandidate[]);
  assembleBundleMock.mockResolvedValue(fakeFixBundle());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("bug-fix workflow integration smoke", () => {
  it("runs the on-disk bug-fix.workflow.yaml end-to-end against a real git fixture", async () => {
    const fixture = makeDivideByZeroFixture();
    primeMocks(fixture);
    const llm = buildStubLLM(fixture);

    const manifest = loadBugFixManifest();
    const { registry, actionRegistry } = registerBugFixRegistries({
      db: fixture.db,
      llm,
      projectRoot: fixture.projectRoot,
    });
    const runner = new WorkflowRunner(
      fixture.db,
      manifestToWorkflow(manifest),
      registry,
    );

    const result = await runManifest(
      runner,
      registry,
      manifest,
      {
        text: "Division crashes when denominator is 0 in calculate(). Add a guard.",
        source: "report",
        projectRoot: fixture.projectRoot,
      },
      actionRegistry,
    );

    // Terminal output: the FixBundle from assembleBundle.
    expect(result.cacheHit).toBe(false);
    const bundle = result.output as FixBundle;
    expect(bundle.bundleId).toBe("bundle-smoke-1");
    expect(bundle.fix.patch.fileEdits[0]?.file).toBe("src/math.ts");

    // Every mocked inner function ran exactly once.
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
    expect(doTheWorkMock).toHaveBeenCalledTimes(1);
    expect(generateComplementaryMock).toHaveBeenCalledTimes(1);
    expect(generatePrincipleCandidateMock).toHaveBeenCalledTimes(1);
    expect(assembleBundleMock).toHaveBeenCalledTimes(1);

    // do-the-work received the live OverlayHandle, including a real
    // worktree path on disk (the action's actual side effect).
    const dtwArgs = doTheWorkMock.mock.calls[0][0];
    expect(typeof dtwArgs.overlay.worktreePath).toBe("string");
    expect(existsSync(dtwArgs.overlay.worktreePath)).toBe(true);
    expect(typeof dtwArgs.overlay.baseRef).toBe("string");
    expect(dtwArgs.overlay.baseRef).toMatch(/^[0-9a-f]{40}$/);

    // formulate received the InvestigateReport (not the wrapping
    // InvestigateResult) — the manifest threads $node.investigate.output.report.
    const formArgs = formulateInvariantMock.mock.calls[0][0];
    expect(formArgs.investigateReport.symptomSummary).toMatch(/Division/);

    // generate-principle-candidate received fixCandidate, not the full
    // DoTheWorkResult — the manifest threads $node.do-the-work.output.fix.
    const gpcArgs = generatePrincipleCandidateMock.mock.calls[0][0];
    expect(gpcArgs.fixCandidate.patch).toBeDefined();
    expect(gpcArgs.fixCandidate.test).toBeUndefined();

    // bundle received fix and test as separate fields.
    const bundleArgs = assembleBundleMock.mock.calls[0][0];
    expect(bundleArgs.fix.patch).toBeDefined();
    expect(bundleArgs.test?.testFilePath).toBe("src/math.regression.test.ts");

    // Memento store: 10 stage mementos + 1 workflow wrapper + 1 action
    // audit memento. Verdict counts:
    //   holds: 10 stages + 1 wrapper + 1 audit = 12 (lower bound).
    const after = mementoStats(fixture.db);
    expect(after.uniqueKeys).toBeGreaterThanOrEqual(12);
    expect(after.byProducer["intake@v1"]).toBe(1);
    expect(after.byProducer["investigate@v1"]).toBe(1);
    expect(after.byProducer["locate@v1"]).toBe(1);
    expect(after.byProducer["classify@v1"]).toBe(1);
    expect(after.byProducer["recognize@v1"]).toBe(1);
    expect(after.byProducer["formulate@v1"]).toBe(1);
    expect(after.byProducer["do-the-work@v1"]).toBe(1);
    expect(after.byProducer["generateComplementary@v1"]).toBe(1);
    expect(after.byProducer["generatePrincipleCandidate@v1"]).toBe(1);
    expect(after.byProducer["bundle@v1"]).toBe(1);
    // The action's audit memento.
    expect(after.byProducer["openOverlay@v1"]).toBeGreaterThanOrEqual(1);
  });

  it("workflow-level cache hits on the second run; no inner function reruns", async () => {
    const fixture = makeDivideByZeroFixture();
    primeMocks(fixture);
    const llm = buildStubLLM(fixture);

    const manifest = loadBugFixManifest();
    const { registry, actionRegistry } = registerBugFixRegistries({
      db: fixture.db,
      llm,
      projectRoot: fixture.projectRoot,
    });
    const runner = new WorkflowRunner(
      fixture.db,
      manifestToWorkflow(manifest),
      registry,
    );

    const input = {
      text: "Division crashes when denominator is 0 in calculate(). Add a guard.",
      source: "report",
      projectRoot: fixture.projectRoot,
    };

    const first = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );
    const second = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    expect(first.cacheHit).toBe(false);
    expect(second.cacheHit).toBe(true);
    expect(second.cid).toBe(first.cid);

    // The workflow-level cache short-circuits the body — every inner
    // function was invoked exactly once across both runs.
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
    expect(doTheWorkMock).toHaveBeenCalledTimes(1);
    expect(generateComplementaryMock).toHaveBeenCalledTimes(1);
    expect(generatePrincipleCandidateMock).toHaveBeenCalledTimes(1);
    expect(assembleBundleMock).toHaveBeenCalledTimes(1);
  });
});
