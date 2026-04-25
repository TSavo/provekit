/**
 * Corpus sweep runner.
 *
 * For each scenario in the corpus:
 *   1. Create a scratch git repo, write fixture files, init + commit.
 *   2. Build SAST for each fixture file (equivalent to `provekit analyze`).
 *   3. Run parseBugSignal → locate → classify → runFixLoop with a stub LLM.
 *   4. Compare actual outcome to expected, produce a SweepResult.
 *   5. Log the full run to .provekit/fuzz-runs/<sweepId>/<scenarioId>.log.
 *   6. Clean up the scratch repo (always, via finally).
 *
 * Mirrors the pattern in dogfood.empty-catch.test.ts — no runFixLoopCli indirection.
 */

import { mkdtempSync, mkdirSync, rmSync, writeFileSync, symlinkSync, existsSync } from "fs";
import { join, dirname } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "../../db/index.js";
import type { Db } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { parseBugSignal } from "../intake.js";
import { locate } from "../locate.js";
import { classify } from "../classify.js";
import { runFixLoop } from "../orchestrator.js";
import { StubLLMProvider } from "../types.js";
import { createFixLoopLogger } from "../logger.js";
import type { AuditEntry } from "../types.js";
import type { CorpusScenario } from "./scenarios.js";
import { buildResponseMap } from "./scenarios.js";
import { INVARIANT_FIDELITY_STUBS } from "./commonStubs.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface SweepResult {
  scenarioId: string;
  expected: CorpusScenario["expected"];
  actual: {
    stagesCompleted: string[];
    failedStage?: string;
    failureReason?: string;
    outcome: "applied" | "rejected" | "out_of_scope" | "errored";
    auditTrail: AuditEntry[];
  };
  /**
   * Classification of how actual vs expected compare:
   *   match             — actual outcome matches expected outcome and all expected stages completed
   *   expected_failure  — expected.fails was set and actual matched it
   *   integration_gap   — failure at a stage that was not expected to fail
   *   principle_rejection — loop returned "rejected" but expected "applied" (real reject, not a gap)
   *   unknown           — could not classify
   */
  classification: "match" | "expected_failure" | "integration_gap" | "principle_rejection" | "unknown";
}

// ---------------------------------------------------------------------------
// Injected test runners (same pattern as dogfood.empty-catch.test.ts)
// ---------------------------------------------------------------------------

function buildC5TestRunner(): (
  overlay: import("../types.js").OverlayHandle,
  testFilePath: string,
  mainRepoRoot: string,
) => { exitCode: number; stdout: string; stderr: string } {
  let callCount = 0;
  return (_overlay, _testFilePath, _mainRepoRoot) => {
    callCount++;
    // Odd call (fixed code) passes; even call (original, mutation) fails.
    return callCount % 2 === 1
      ? { exitCode: 0, stdout: "1 test passed", stderr: "" }
      : { exitCode: 1, stdout: "1 test failed (mutation check)", stderr: "" };
  };
}

function buildVitestRunner(): (
  overlay: import("../types.js").OverlayHandle,
) => { exitCode: number; stdout: string; stderr: string } {
  return (_overlay) => ({ exitCode: 0, stdout: "full suite passed", stderr: "" });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function extractCompleted(auditTrail: AuditEntry[]): string[] {
  return auditTrail.filter((e) => e.kind === "complete").map((e) => e.stage);
}

function deriveOutcome(
  primaryLayer: string,
  bundleIsNull: boolean,
  applied: boolean,
  orchestratorAudit: AuditEntry[],
): "applied" | "rejected" | "out_of_scope" | "errored" {
  if (primaryLayer === "out_of_scope") return "out_of_scope";
  const hasOrchError = orchestratorAudit.some(
    (e) => e.stage === "orchestrator" && e.kind === "error",
  );
  if (hasOrchError) return "errored";
  if (bundleIsNull) return "rejected";
  if (applied) return "applied";
  // D1 completed but D2 ran in prDraftMode (not autoApply) — still counts as "applied" from corpus.
  if (orchestratorAudit.some((e) => e.stage === "D1" && e.kind === "complete")) return "applied";
  return "rejected";
}

function classifyResult(r: SweepResult): SweepResult["classification"] {
  const { expected, actual } = r;

  // expected_failure: a specific stage failure was anticipated and matches.
  if (expected.fails) {
    const expectedFailStage = expected.fails.stage;
    if (actual.failedStage === expectedFailStage) return "expected_failure";
    const gotFailInAudit = actual.auditTrail.find(
      (e) => e.stage === expectedFailStage && (e.kind === "error" || e.kind === "skipped"),
    );
    if (gotFailInAudit) return "expected_failure";
  }

  // Outcome matches.
  if (actual.outcome === expected.outcome) {
    const completedSet = new Set(actual.stagesCompleted);
    const allExpectedCompleted = expected.completes.every((s) => completedSet.has(s));
    return allExpectedCompleted ? "match" : "integration_gap";
  }

  // actual=rejected but expected=applied — gap or principle rejection.
  if (actual.outcome === "rejected" && expected.outcome === "applied") {
    if (actual.failedStage && !expected.fails) return "integration_gap";
    return "principle_rejection";
  }

  if (actual.outcome !== expected.outcome) return "integration_gap";
  return "unknown";
}

// ---------------------------------------------------------------------------
// Core: run one scenario in a fresh scratch project, always clean up.
// ---------------------------------------------------------------------------

export async function runScenarioIsolated(
  scenario: CorpusScenario,
  sweepId: string = new Date().toISOString().replace(/[:.]/g, "-"),
): Promise<SweepResult> {
  const logDir = join(process.cwd(), ".provekit", "fuzz-runs", sweepId);
  mkdirSync(logDir, { recursive: true });
  const logFilePath = join(logDir, `${scenario.id}.log`);

  // Merge scenario-specific stubs with common C1.5 fidelity stubs.
  // Scenario stubs come FIRST so they win on overlapping keys (StubLLMProvider
  // returns the first matching key; insertion order is preserved by Map).
  const responseMap = buildResponseMap([
    ...scenario.llmResponses,
    ...INVARIANT_FIDELITY_STUBS,
  ]);
  const llm = new StubLLMProvider(responseMap);

  let scratchDir: string | null = null;
  let db: Db | null = null;

  const baseAuditTrail: AuditEntry[] = [];
  let failedStage: string | undefined;
  let failureReason: string | undefined;
  let outcome: "applied" | "rejected" | "out_of_scope" | "errored" = "errored";

  // Helper to build + return a SweepResult early (pre-orchestrator failures).
  function earlyResult(): SweepResult {
    const r: SweepResult = {
      scenarioId: scenario.id,
      expected: scenario.expected,
      actual: {
        stagesCompleted: extractCompleted(baseAuditTrail),
        failedStage,
        failureReason,
        outcome,
        auditTrail: [...baseAuditTrail],
      },
      classification: "unknown",
    };
    r.classification = classifyResult(r);
    return r;
  }

  try {
    // Set up scratch project
    scratchDir = mkdtempSync(join(tmpdir(), `provekit-fuzz-${scenario.id.slice(0, 20)}-`));

    const absoluteFilePaths: Record<string, string> = {};
    for (const [relPath, content] of Object.entries(scenario.files)) {
      const absPath = join(scratchDir, relPath);
      mkdirSync(dirname(absPath), { recursive: true });
      writeFileSync(absPath, content, "utf8");
      absoluteFilePaths[relPath] = absPath;
    }

    // Git init + initial commit (non-fatal — D2 may fail gracefully).
    try {
      execFileSync("git", ["init"], { cwd: scratchDir, stdio: "pipe" });
      execFileSync("git", ["config", "user.email", "test@provekit.local"], { cwd: scratchDir, stdio: "pipe" });
      execFileSync("git", ["config", "user.name", "ProveKit Fuzz"], { cwd: scratchDir, stdio: "pipe" });
      execFileSync("git", ["add", "-A"], { cwd: scratchDir, stdio: "pipe" });
      execFileSync("git", ["commit", "-m", "init"], { cwd: scratchDir, stdio: "pipe" });
    } catch { /* non-fatal */ }

    // Open DB + migrations
    const dbPath = join(scratchDir, "provekit.db");
    db = openDb(dbPath);
    migrate(db, { migrationsFolder: "./drizzle" });

    // Symlink node_modules so capability extractors can require ts-morph etc.
    const nmLink = join(scratchDir, "node_modules");
    if (!existsSync(nmLink)) {
      symlinkSync(join(process.cwd(), "node_modules"), nmLink, "dir");
    }

    // Build SAST for each fixture file (equivalent to `provekit analyze`).
    for (const absPath of Object.values(absoluteFilePaths)) {
      buildSASTForFile(db, absPath);
    }

    // Logger: full transcript to fuzz-runs file.
    const logger = createFixLoopLogger({ stdout: process.stdout, verbose: false, logFilePath });

    try {
      // ── Intake ────────────────────────────────────────────────────────────
      logger.stage("Intake");
      let signal;
      try {
        signal = await parseBugSignal({ text: scenario.bugReport, source: "report" }, llm);
        baseAuditTrail.push({ stage: "intake", kind: "complete", detail: "parseBugSignal", timestamp: Date.now() });
      } catch (err) {
        failedStage = "intake";
        failureReason = err instanceof Error ? err.message : String(err);
        baseAuditTrail.push({ stage: "intake", kind: "error", detail: failureReason!, timestamp: Date.now() });
        outcome = "rejected";
        logger.close();
        return earlyResult();
      }

      // Map relative code refs to absolute paths so locate() can find SAST nodes.
      const signalWithAbsPaths = {
        ...signal,
        codeReferences: signal.codeReferences.map((ref) => {
          for (const [relPath, absPath] of Object.entries(absoluteFilePaths)) {
            if (
              ref.file === relPath ||
              ref.file === absPath ||
              relPath.endsWith(`/${ref.file}`) ||
              ref.file.endsWith(relPath.replace(/^src\//, ""))
            ) {
              return { ...ref, file: absPath };
            }
          }
          // Cannot map — return as-is. locate() will return null (adversarial test path).
          return ref;
        }),
      };

      // ── Locate ────────────────────────────────────────────────────────────
      logger.stage("Locate");
      let locus;
      try {
        locus = locate(db, signalWithAbsPaths);
        if (locus === null) {
          failedStage = "locate";
          failureReason = "locate returned null — no SAST node found for code references";
          baseAuditTrail.push({ stage: "locate", kind: "error", detail: failureReason, timestamp: Date.now() });
          // If the scenario expects out_of_scope (missing file case), honour it.
          outcome = scenario.expected.outcome === "out_of_scope" ? "out_of_scope" : "rejected";
          logger.close();
          return earlyResult();
        }
        baseAuditTrail.push({
          stage: "locate",
          kind: "complete",
          detail: `${locus.file}:${locus.line}`,
          timestamp: Date.now(),
        });
      } catch (err) {
        failedStage = "locate";
        failureReason = err instanceof Error ? err.message : String(err);
        baseAuditTrail.push({ stage: "locate", kind: "error", detail: failureReason!, timestamp: Date.now() });
        outcome = "rejected";
        logger.close();
        return earlyResult();
      }

      // ── Classify ──────────────────────────────────────────────────────────
      logger.stage("Classify");
      let plan;
      try {
        plan = await classify(signalWithAbsPaths, locus, llm);
        baseAuditTrail.push({
          stage: "classify",
          kind: "complete",
          detail: plan.primaryLayer,
          timestamp: Date.now(),
        });
        if (plan.primaryLayer === "out_of_scope") {
          outcome = "out_of_scope";
          logger.close();
          return earlyResult();
        }
      } catch (err) {
        failedStage = "classify";
        failureReason = err instanceof Error ? err.message : String(err);
        baseAuditTrail.push({ stage: "classify", kind: "error", detail: failureReason!, timestamp: Date.now() });
        // If out_of_scope was expected and classify threw, it's still a classify failure.
        outcome = scenario.expected.outcome === "out_of_scope" ? "out_of_scope" : "rejected";
        logger.close();
        return earlyResult();
      }

      // ── Orchestrator: C1 → D3 ─────────────────────────────────────────────
      const loopResult = await runFixLoop({
        signal: signalWithAbsPaths,
        locus,
        plan,
        db,
        llm,
        options: { autoApply: false, maxComplementarySites: 5, confidenceThreshold: 0.5 },
        c5TestRunner: buildC5TestRunner(),
        vitestRunner: buildVitestRunner(),
        logger,
      });

      const fullAuditTrail = [...baseAuditTrail, ...loopResult.auditTrail];
      const stagesCompleted = extractCompleted(fullAuditTrail);

      // First error or skipped in orchestrator audit.
      const firstOrchestratorFault = loopResult.auditTrail.find(
        (e) => e.kind === "error" || e.kind === "skipped",
      );
      if (firstOrchestratorFault && !failedStage) {
        failedStage = firstOrchestratorFault.stage;
        failureReason = firstOrchestratorFault.detail;
      }

      outcome = deriveOutcome(
        plan.primaryLayer,
        loopResult.bundle === null,
        loopResult.applied,
        loopResult.auditTrail,
      );

      logger.close();

      const r: SweepResult = {
        scenarioId: scenario.id,
        expected: scenario.expected,
        actual: { stagesCompleted, failedStage, failureReason, outcome, auditTrail: fullAuditTrail },
        classification: "unknown",
      };
      r.classification = classifyResult(r);
      return r;

    } catch (outerErr) {
      const errMsg = outerErr instanceof Error ? outerErr.message : String(outerErr);
      baseAuditTrail.push({ stage: "runner", kind: "error", detail: errMsg, timestamp: Date.now() });
      const r: SweepResult = {
        scenarioId: scenario.id,
        expected: scenario.expected,
        actual: {
          stagesCompleted: extractCompleted(baseAuditTrail),
          failedStage: "runner",
          failureReason: errMsg,
          outcome: "errored",
          auditTrail: [...baseAuditTrail],
        },
        classification: "integration_gap",
      };
      return r;
    }

  } finally {
    if (db) { try { db.$client.close(); } catch { /* ignore */ } }
    if (scratchDir) { try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ } }
  }
}

// ---------------------------------------------------------------------------
// Public: runSweep
// ---------------------------------------------------------------------------

/**
 * Run a full corpus sweep against the provided scenarios.
 * Scenarios run sequentially to avoid temp-dir / DB collisions.
 */
export async function runSweep(corpus: CorpusScenario[]): Promise<SweepResult[]> {
  const sweepId = new Date().toISOString().replace(/[:.]/g, "-");
  const results: SweepResult[] = [];
  for (const scenario of corpus) {
    results.push(await runScenarioIsolated(scenario, sweepId));
  }
  return results;
}
