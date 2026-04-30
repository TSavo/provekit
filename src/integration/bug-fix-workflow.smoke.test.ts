/**
 * End-to-end smoke for the bug-fix workflow (task #7).
 *
 * Validates that the workflow infrastructure (intake → investigate → locate →
 * classify → formulate) actually runs against a real fixture project and
 * deposits verdict-bearing mementos in the store. Exercises the YAML manifest
 * loader, the producer registry, the topo-sorted dispatcher in
 * `runManifest`, and the cache-key cascade in `WorkflowRunner`.
 *
 * --- What this smoke does NOT cover, and why ----------------------------
 *
 * The on-disk `bug-fix.workflow.yaml` references 11 capabilities. As of
 * 2026-04-29 the registry returned by `registerBugFixCapabilities` wires
 * up only 7 of them. The 4 newer producer modules
 * (`recognize`, `openOverlay`, `generateComplementary`,
 * `generatePrincipleCandidate`) exist but are not yet plugged in. Three
 * specific blockers prevent the full manifest from running here:
 *
 *   1. Capability-name mismatch — the manifest uses kebab-case
 *      (`open-overlay`, `generate-complementary`, `generate-principle-candidate`)
 *      while three of the new producer constants ship as camelCase
 *      (`openOverlay`, `generateComplementary`, `generatePrincipleCandidate`).
 *      Only `recognize` matches.
 *   2. Action vs Stage mismatch — `openOverlay` is implemented as an
 *      `Action` (side-effecting; mints a real git worktree + sqlite SAST
 *      db), but the manifest declares `open-overlay` as a stage `node`.
 *   3. Wiring gap — the four producers are not registered by
 *      `registerBugFixCapabilities`; `PENDING_CAPABILITIES` still lists
 *      them as not-yet-authored.
 *
 * Plus: `do-the-work` requires a live `OverlayHandle` (worktree path +
 * sqlite handle), so even if #1–#3 were resolved, running the full
 * pipeline in this offline smoke would require a fixture git repo.
 *
 * The smoke therefore terminates the chain at `formulate` (the last
 * stage that does NOT require an overlay) and runs against a synthetic
 * manifest that exercises every registered capability EXCEPT bundle and
 * do-the-work. Two additional tests cover the existing regression
 * surface: the on-disk manifest still raises the runner's standard
 * "not registered" error, and the workflow-level cache hits on a second
 * run.
 *
 * The four findings above are filed in the smoke-test report; resolving
 * them is out of scope for this task per the cut list.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

// formulateInvariant is mocked at the module boundary so the smoke can
// assert real wiring without spinning up Z3. The Stage wrapper in
// makeFormulateStage still does the input canonicalization + memento
// write/read; only the underlying solver call is replaced. See
// src/workflow/producers/formulate.test.ts for the same pattern.
const { formulateInvariantMock } = vi.hoisted(() => ({
  formulateInvariantMock: vi.fn(),
}));
vi.mock("../fix/stages/formulateInvariant.js", () => ({
  formulateInvariant: formulateInvariantMock,
}));

import { openDb, type Db } from "../db/index.js";
import { _clearIntakeRegistry } from "../fix/intake.js";
import { registerAll } from "../fix/intakeAdapters/index.js";
import { StubLLMProvider } from "../fix/types.js";
import type { InvariantClaim } from "../fix/types.js";
import { stats as mementoStats } from "../fix/runtime/mementoStore.js";
import { buildSASTForFile } from "../sast/builder.js";
import { WorkflowRunner } from "../workflow/runner.js";
import {
  parseManifest,
  runManifest,
  manifestToWorkflow,
} from "../workflow/manifest.js";
import {
  loadBugFixManifest,
  registerBugFixCapabilities,
} from "../workflows/bug-fix.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

interface Fixture {
  projectRoot: string;
  db: Db;
  /** The locus file relative path inside the project root. */
  locusFile: string;
  /** Absolute path to the locus file (for buildSASTForFile). */
  locusFileAbs: string;
}

/**
 * Lay down a minimal TypeScript project on disk and populate its SAST
 * database from the source. The fixture is what makes this an
 * integration test rather than a unit test — locate() will run real DB
 * queries against real SAST rows.
 */
function makeDivideByZeroFixture(): Fixture {
  const projectRoot = mkdtempSync(join(tmpdir(), "bugfix-smoke-divzero-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  mkdirSync(join(projectRoot, "src"), { recursive: true });

  const locusFile = "src/math.ts";
  const locusFileAbs = join(projectRoot, locusFile);
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

  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  buildSASTForFile(db, locusFileAbs);

  return { projectRoot, db, locusFile, locusFileAbs };
}

function makeOffByOneFixture(): Fixture {
  const projectRoot = mkdtempSync(join(tmpdir(), "bugfix-smoke-offby1-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  mkdirSync(join(projectRoot, "src"), { recursive: true });

  const locusFile = "src/dateValidator.ts";
  const locusFileAbs = join(projectRoot, locusFile);
  writeFileSync(
    locusFileAbs,
    [
      "export function isLeapYear(year: number): boolean {",
      "  return year % 4 === 0 && year % 100 !== 0;",
      "}",
      "",
    ].join("\n"),
    "utf-8",
  );

  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  buildSASTForFile(db, locusFileAbs);

  return { projectRoot, db, locusFile, locusFileAbs };
}

// ---------------------------------------------------------------------------
// Stub LLM — keyed by prompt prefix substrings emitted by each stage.
// ---------------------------------------------------------------------------

function buildStubLLM(fixture: Fixture, opts: {
  summary: string;
  failureDescription: string;
  bugClassHint: string;
  fixHint: string;
  rootCause: string;
  fixHypothesis: string;
}): StubLLMProvider {
  const intakeJson = JSON.stringify({
    summary: opts.summary,
    failureDescription: opts.failureDescription,
    fixHint: opts.fixHint,
    codeReferences: [
      { file: fixture.locusFile, line: 2, function: "calculate" },
    ],
    bugClassHint: opts.bugClassHint,
  });

  const investigateJson = JSON.stringify({
    symptomSummary: opts.summary,
    rootCauseHypothesis: opts.rootCause,
    fixHypothesis: opts.fixHypothesis,
    primaryLocation: {
      file: fixture.locusFile,
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
// Synthetic manifest — exercises every REGISTERED bug-fix capability whose
// inputs are satisfied without a live overlay. Mirrors the on-disk YAML's
// dependency shape but stops at formulate.
// ---------------------------------------------------------------------------

const SUBSET_MANIFEST_YAML = `
name: bug-fix-smoke-subset
cid: wf-bugfix-smoke-subset-v1
description: >-
  Subset of bug-fix.workflow.yaml exercised in the integration smoke.
  Drops recognize/open-overlay/do-the-work/generate-*/bundle (overlay-
  dependent or not-yet-registered). Terminal node: formulate.
nodes:
  - id: intake
    capability: intake
    input:
      text: $input.text
      source: $input.source
  - id: investigate
    capability: investigate
    input:
      signal: $node.intake.output
      projectRoot: $input.projectRoot
  - id: locate
    capability: locate
    input:
      signal: $node.intake.output
  - id: classify
    capability: classify
    input:
      signal: $node.intake.output
      locus: $node.locate.output
  - id: formulate
    capability: formulate
    input:
      signal: $node.intake.output
      locus: $node.locate.output
      investigateReport: $node.investigate.output
output: $node.formulate.output
`;

// ---------------------------------------------------------------------------
// Test setup
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

beforeEach(() => {
  _clearIntakeRegistry();
  registerAll();
  formulateInvariantMock.mockReset();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("bug-fix workflow integration smoke", () => {
  it("runs intake → investigate → locate → classify → formulate against a divide-by-zero fixture", async () => {
    const fixture = makeDivideByZeroFixture();
    const llm = buildStubLLM(fixture, {
      summary: "Division crashes when denominator is 0.",
      failureDescription: "Division-by-zero in calculate.",
      fixHint: "Guard before dividing.",
      bugClassHint: "divide-by-zero",
      rootCause: "calculate() does not check that denominator is non-zero.",
      fixHypothesis: "Throw or return a sentinel when denominator === 0.",
    });

    formulateInvariantMock.mockResolvedValue(fakeInvariantClaim(fixture.locusFile));

    const manifest = parseManifest(SUBSET_MANIFEST_YAML);
    const registry = registerBugFixCapabilities({
      db: fixture.db,
      llm,
      projectRoot: fixture.projectRoot,
    });
    const runner = new WorkflowRunner(
      fixture.db,
      manifestToWorkflow(manifest),
      registry,
    );

    const result = await runManifest(runner, registry, manifest, {
      text: "Division crashes when denominator is 0 in calculate(). Add a guard.",
      source: "report",
      projectRoot: fixture.projectRoot,
    });

    expect(result.cacheHit).toBe(false);
    const claim = result.output as InvariantClaim;
    expect(claim.formalExpression).toMatch(/declare-const b Int/);
    expect(claim.bindings[0]?.file).toBe(fixture.locusFile);

    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
    const formulateArgs = formulateInvariantMock.mock.calls[0][0];
    expect(formulateArgs.signal.codeReferences[0]?.file).toBe(fixture.locusFile);
    expect(formulateArgs.locus.file).toBe(fixture.locusFile);
    // The on-disk manifest threads investigate's full InvestigateResult
    // ({report, reportPath, codeReferences}) into formulate's
    // investigateReport slot; the inner report is one level deeper.
    // Assert the symptomSummary surfaces under either shape.
    const investigateReport = formulateArgs.investigateReport;
    const symptomSummary =
      investigateReport?.symptomSummary ?? investigateReport?.report?.symptomSummary;
    expect(symptomSummary).toMatch(/Division/);

    const after = mementoStats(fixture.db);
    // 5 stage mementos (intake/investigate/locate/classify/formulate) +
    // 1 workflow-level wrapper memento. SAST tables are separate and
    // don't add rows to the verifications table.
    expect(after.uniqueKeys).toBeGreaterThanOrEqual(6);
    expect(after.byVerdict.holds).toBeGreaterThanOrEqual(6);
    expect(after.byProducer["intake@v1"]).toBe(1);
    expect(after.byProducer["investigate@v1"]).toBe(1);
    expect(after.byProducer["locate@v1"]).toBe(1);
    expect(after.byProducer["classify@v1"]).toBe(1);
    expect(after.byProducer["formulate@v1"]).toBe(1);
  });

  it("runs against an off-by-one fixture (different bug shape)", async () => {
    const fixture = makeOffByOneFixture();
    const llm = buildStubLLM(fixture, {
      summary: "isLeapYear returns wrong answer for centuries divisible by 400.",
      failureDescription: "Off-by-one in isLeapYear: 2000 reports false.",
      fixHint: "Add the year % 400 === 0 branch.",
      bugClassHint: "off-by-one",
      rootCause: "Leap-year logic ignores the century-of-400 exception.",
      fixHypothesis: "Add `|| year % 400 === 0` to the return expression.",
    });

    formulateInvariantMock.mockResolvedValue(fakeInvariantClaim(fixture.locusFile));

    const manifest = parseManifest(SUBSET_MANIFEST_YAML);
    const registry = registerBugFixCapabilities({
      db: fixture.db,
      llm,
      projectRoot: fixture.projectRoot,
    });
    const runner = new WorkflowRunner(
      fixture.db,
      manifestToWorkflow(manifest),
      registry,
    );

    const result = await runManifest(runner, registry, manifest, {
      text: "isLeapYear(2000) returns false but the year is a leap year.",
      source: "report",
      projectRoot: fixture.projectRoot,
    });

    expect(result.cacheHit).toBe(false);
    const claim = result.output as InvariantClaim;
    expect(claim.bindings[0]?.file).toBe(fixture.locusFile);
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
  });

  it("workflow-level cache hits on the second run with identical input", async () => {
    const fixture = makeDivideByZeroFixture();
    const llm = buildStubLLM(fixture, {
      summary: "Division crashes when denominator is 0.",
      failureDescription: "Division-by-zero in calculate.",
      fixHint: "Guard before dividing.",
      bugClassHint: "divide-by-zero",
      rootCause: "calculate() does not check that denominator is non-zero.",
      fixHypothesis: "Throw or return a sentinel when denominator === 0.",
    });
    formulateInvariantMock.mockResolvedValue(fakeInvariantClaim(fixture.locusFile));

    const manifest = parseManifest(SUBSET_MANIFEST_YAML);
    const registry = registerBugFixCapabilities({
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

    const first = await runManifest(runner, registry, manifest, input);
    const second = await runManifest(runner, registry, manifest, input);

    expect(first.cacheHit).toBe(false);
    expect(second.cacheHit).toBe(true);
    expect(second.cid).toBe(first.cid);
    // Workflow-level cache short-circuits the body — no producer reruns.
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
  });

  it("on-disk bug-fix.workflow.yaml still raises 'not registered' for the 4 pending capabilities", async () => {
    // The full manifest references recognize / open-overlay /
    // generate-complementary / generate-principle-candidate. The wiring
    // gap (see file docblock) is intentional and surfaces here. If this
    // assertion ever flips, the fix is to update PENDING_CAPABILITIES
    // and either resolve the kebab-vs-camel name mismatch or move
    // open-overlay into the manifest's actions: block.
    const fixture = makeDivideByZeroFixture();
    const llm = buildStubLLM(fixture, {
      summary: "irrelevant — should fail before any stage runs",
      failureDescription: "n/a",
      fixHint: "n/a",
      bugClassHint: "n/a",
      rootCause: "n/a",
      fixHypothesis: "n/a",
    });

    const manifest = loadBugFixManifest();
    const registry = registerBugFixCapabilities({
      db: fixture.db,
      llm,
      projectRoot: fixture.projectRoot,
    });
    const runner = new WorkflowRunner(
      fixture.db,
      manifestToWorkflow(manifest),
      registry,
    );

    await expect(
      runManifest(runner, registry, manifest, {
        text: "anything",
        source: "report",
        projectRoot: fixture.projectRoot,
      }),
    ).rejects.toThrow(/not registered/);
  });
});
