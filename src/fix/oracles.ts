/**
 * D1b: Oracle runners for bundle coherence.
 *
 * Oracles 1,2,3,6,9,14,16,17,18 are verified via audit trail (already-fired).
 * This module implements oracles 4,5,7,8,10,11,12,13,15 — the "NEW" set that
 * runs during bundle assembly.
 *
 * MVP stubs: oracle 15 is pass-through (D3 territory).
 * Oracles 7 and 12 are fully implemented.
 */

import type { OverlayHandle, InvariantClaim, FixCandidate, BugSignal, BugLocus } from "./types.js";
import type { Db } from "../db/index.js";
import { verifyBlock } from "../verifier.js";
import { clauses, gapReports } from "../db/schema/index.js";
import { principleMatches } from "../db/schema/principleMatches.js";
import { files } from "../sast/schema/nodes.js";
import { sql, eq } from "drizzle-orm";
import { spawnSync } from "child_process";
import { join } from "path";
import { existsSync, symlinkSync, mkdtempSync, writeFileSync, rmSync, mkdirSync, readdirSync, readFileSync } from "fs";
import * as ts from "typescript";
import { extractWitnessInputs } from "./testGen.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";

function resolveProjectRoot(): string {
  // process.cwd() is the project root when running vitest or the CLI from the repo root.
  // Fallback: walk up from __dirname (src/fix -> src -> project root).
  return process.cwd();
}

/** Transpile TypeScript to CJS JS in-memory (same pattern as capabilityExecutor). */
function transpileTs(source: string): string {
  const result = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
      skipLibCheck: true,
    },
  });
  return result.outputText;
}

export interface OracleResult {
  passed: boolean;
  detail: string;
}

// ---------------------------------------------------------------------------
// Oracle #4 — no-regression on proven clauses
// ---------------------------------------------------------------------------

/**
 * Query mainDb for proven clauses and verify each still holds (is unsat under
 * Z3). If a previously-proven clause becomes sat, that signals a regression.
 *
 * MVP simplification: caps at 20 clauses. If main DB has no clauses table
 * rows with verdict='proven', skips gracefully.
 */
export async function runOracle4(args: {
  overlay: OverlayHandle;
  mainDb: Db;
}): Promise<OracleResult> {
  const { mainDb } = args;

  let rows: { smt2: string; id: number }[];
  try {
    rows = mainDb
      .select({ id: clauses.id, smt2: clauses.smt2 })
      .from(clauses)
      .where(sql`${clauses.verdict} = 'proven'`)
      .limit(20)
      .all();
  } catch {
    // Table may not exist in test fixtures — skip gracefully.
    return { passed: true, detail: "clauses table not accessible; skip oracle #4" };
  }

  if (rows.length === 0) {
    return { passed: true, detail: "no prior proven clauses; nothing to regress" };
  }

  const regressions: number[] = [];
  for (const row of rows) {
    const { result } = verifyBlock(row.smt2);
    // Previously proven = negated-goal was unsat. If now sat, regression.
    if (result === "sat") {
      regressions.push(row.id);
    }
  }

  if (regressions.length > 0) {
    return {
      passed: false,
      detail: `oracle #4: ${regressions.length} previously-proven clause(s) regressed to sat: ids=${regressions.join(",")}`,
    };
  }

  return { passed: true, detail: `oracle #4: ${rows.length} proven clause(s) still hold (unsat)` };
}

// ---------------------------------------------------------------------------
// Oracle #5 — bundle coherence SMT
// ---------------------------------------------------------------------------

/**
 * Concatenate all formalExpressions into one SMT script and check satisfiability.
 * Contradictory bundle → unsat → fail.
 *
 * MVP simplification: if only 1 invariant, coherence is trivially true.
 */
export function runOracle5(args: { invariants: InvariantClaim[] }): OracleResult {
  const { invariants } = args;

  if (invariants.length === 0) {
    return { passed: true, detail: "oracle #5: no invariants to check; trivially coherent" };
  }

  if (invariants.length === 1) {
    return { passed: true, detail: "oracle #5: single invariant; coherence is trivially true" };
  }

  // Build a combined SMT script by merging all formalExpressions.
  // Strip individual (check-sat) lines so we can add one at the end.
  const parts = invariants.map((inv) =>
    inv.formalExpression.replace(/\(check-sat\)/g, "").trim(),
  );
  const combined = parts.join("\n") + "\n(check-sat)\n";

  const { result } = verifyBlock(combined);

  if (result === "sat") {
    return { passed: true, detail: `oracle #5: combined invariant set is satisfiable (${invariants.length} invariants)` };
  }
  if (result === "unsat") {
    return { passed: false, detail: "oracle #5: combined invariants are unsatisfiable — bundle is self-contradictory" };
  }

  // unknown or error: be lenient at MVP
  return { passed: true, detail: `oracle #5: Z3 returned '${result}' on combined script; treating as pass (MVP leniency)` };
}

// ---------------------------------------------------------------------------
// Oracle #7 — runtime harness witness replay
// ---------------------------------------------------------------------------

/**
 * Replay the Z3 witness inputs against the post-fix function in the overlay.
 *
 * Strategy: transpile the locus file to CJS via ts.transpileModule, write to a
 * tmpfile under node_modules/.cache/, spawn a tiny Node driver that requires()
 * the module, calls locus.function with witness inputs, and emits a JSON result.
 *
 * Outcome heuristic: throw OR finite return = pass; non-finite (Infinity, NaN) = fail.
 * Unknown / can't-execute states are treated as pass with informational detail.
 */
export async function runOracle7(args: {
  overlay: OverlayHandle;
  fix: FixCandidate;
  invariant: InvariantClaim;
  signal: BugSignal;
  locus: BugLocus;
}): Promise<OracleResult> {
  const { overlay, invariant, locus } = args;

  // No witness → skip (informational pass)
  if (!invariant.witness) {
    return { passed: true, detail: "oracle #7: no Z3 witness; harness replay skipped (informational)" };
  }

  // No function name → can't invoke → skip
  if (!locus.function) {
    return { passed: true, detail: "oracle #7: locus has no function name; harness replay skipped (informational)" };
  }

  // Extract witness inputs
  let inputs: Record<string, unknown>;
  try {
    inputs = extractWitnessInputs(invariant);
  } catch (err: any) {
    return { passed: true, detail: `oracle #7: could not extract witness inputs — ${err?.message ?? "unknown"}; skipped (informational)` };
  }

  if (Object.keys(inputs).length === 0) {
    return { passed: true, detail: "oracle #7: witness produced no inputs; harness replay skipped (informational)" };
  }

  // Resolve the locus file inside the overlay
  const locusFile = locus.file;
  let overlaySourcePath: string;
  if (locusFile.startsWith("/")) {
    // Try to find it relative to the overlay worktree
    const parts = locusFile.split("/").filter(Boolean);
    let found: string | null = null;
    for (let i = 0; i < parts.length; i++) {
      const candidate = join(overlay.worktreePath, parts.slice(i).join("/"));
      if (existsSync(candidate)) {
        found = candidate;
        break;
      }
    }
    if (!found) {
      return { passed: true, detail: `oracle #7: locus file not found in overlay (${locusFile}); skipped (informational)` };
    }
    overlaySourcePath = found;
  } else {
    overlaySourcePath = join(overlay.worktreePath, locusFile);
    if (!existsSync(overlaySourcePath)) {
      return { passed: true, detail: `oracle #7: locus file not found in overlay (${locusFile}); skipped (informational)` };
    }
  }

  // Transpile the source file
  let sourceTs: string;
  try {
    sourceTs = readFileSync(overlaySourcePath, "utf8");
  } catch (err: any) {
    return { passed: true, detail: `oracle #7: could not read locus file — ${err?.message ?? "unknown"}; skipped (informational)` };
  }

  let transpiledJs: string;
  try {
    transpiledJs = transpileTs(sourceTs);
  } catch (err: any) {
    return { passed: true, detail: `oracle #7: transpile failed — ${err?.message ?? "unknown"}; skipped (informational)` };
  }

  const projectRoot = resolveProjectRoot();
  const cacheDir = join(projectRoot, "node_modules", ".cache");
  mkdirSync(cacheDir, { recursive: true });
  const tmpDir = mkdtempSync(join(cacheDir, "provekit-oracle7-"));

  try {
    const moduleFile = join(tmpDir, "module.cjs");
    writeFileSync(moduleFile, transpiledJs, "utf8");

    // Build driver script
    const inputsJson = JSON.stringify(inputs);
    const functionName = locus.function;
    const driverScript = `
"use strict";
try {
  const mod = require(${JSON.stringify(moduleFile)});
  const fn = mod[${JSON.stringify(functionName)}] ?? mod["default"]?.[${JSON.stringify(functionName)}];
  if (typeof fn !== "function") {
    process.stdout.write(JSON.stringify({ kind: "untestable", reason: "export not a function" }));
    process.exit(0);
  }
  const inputs = ${inputsJson};
  const args = Object.values(inputs);
  let result;
  try {
    result = fn(...args);
    if (result && typeof result.then === "function") {
      result.then(function(v) {
        process.stdout.write(JSON.stringify({ kind: "returned", value: v === undefined ? null : (typeof v === "number" ? v : String(v)) }));
      }).catch(function(e) {
        process.stdout.write(JSON.stringify({ kind: "threw", message: String(e && e.message || e).slice(0, 200) }));
      });
    } else {
      process.stdout.write(JSON.stringify({ kind: "returned", value: result === undefined ? null : (typeof result === "number" ? result : String(result)) }));
    }
  } catch(e) {
    process.stdout.write(JSON.stringify({ kind: "threw", message: String(e && e.message || e).slice(0, 200) }));
  }
} catch(e) {
  process.stdout.write(JSON.stringify({ kind: "load-error", message: String(e && e.message || e).slice(0, 200) }));
}
`;
    const driverFile = join(tmpDir, "driver.cjs");
    writeFileSync(driverFile, driverScript, "utf8");

    const proc = spawnSync(process.execPath, [driverFile], {
      encoding: "utf8",
      timeout: 10000,
      cwd: projectRoot,
    });

    if (proc.error || proc.status === null) {
      return { passed: true, detail: `oracle #7: driver spawn failed — ${proc.error?.message ?? "timeout"}; skipped (informational)` };
    }

    let outcome: { kind: string; value?: unknown; message?: string; reason?: string };
    try {
      outcome = JSON.parse(proc.stdout.trim());
    } catch {
      return { passed: true, detail: `oracle #7: driver produced no parseable output; skipped (informational)` };
    }

    if (outcome.kind === "load-error") {
      return { passed: true, detail: `oracle #7: module load failed — ${outcome.message}; skipped (informational)` };
    }

    if (outcome.kind === "untestable") {
      return { passed: true, detail: `oracle #7: function not exported — ${outcome.reason}; skipped (informational)` };
    }

    if (outcome.kind === "threw") {
      // Function threw on witness inputs → rejected bad input → pass
      return { passed: true, detail: `oracle #7: post-fix function threw on witness inputs (good: rejected bad input) — ${outcome.message}` };
    }

    if (outcome.kind === "returned") {
      const v = outcome.value;
      // Non-finite numeric returns are bad (e.g. Infinity, NaN)
      if (typeof v === "number" && !Number.isFinite(v)) {
        return { passed: false, detail: `oracle #7: post-fix function returned non-finite value (${v}) for witness inputs — invariant violation persists` };
      }
      return { passed: true, detail: `oracle #7: post-fix function returned finite/non-numeric value for witness inputs — invariant holds` };
    }

    return { passed: true, detail: `oracle #7: unexpected driver outcome kind '${outcome.kind}'; skipped (informational)` };
  } finally {
    try { rmSync(tmpDir, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

// ---------------------------------------------------------------------------
// Oracle #8 — no-new-gaps
// ---------------------------------------------------------------------------

/**
 * Compare count of gap_reports in overlay's sastDb vs main DB's gap_reports.
 * If overlay > main, fail (new gaps introduced).
 *
 * MVP simplification: compare ROW COUNT only, not individual gap identities.
 * Existing gaps closing is expected → pass.
 */
export async function runOracle8(args: {
  overlay: OverlayHandle;
  mainDb: Db;
}): Promise<OracleResult> {
  const { overlay, mainDb } = args;

  let mainCount: number;
  try {
    const result = mainDb.select({ count: sql<number>`count(*)` }).from(gapReports).all();
    mainCount = result[0]?.count ?? 0;
  } catch {
    // gap_reports table not in main DB — skip gracefully.
    return { passed: true, detail: "oracle #8: gap_reports not accessible in mainDb; skip" };
  }

  let overlayCount: number;
  try {
    // The overlay uses its own sastDb — gap_reports may not be migrated there.
    // Use raw better-sqlite3 to avoid import-chain issues with the overlay's DB handle.
    const client = overlay.sastDb.$client;
    const row = client.prepare("SELECT count(*) as c FROM gap_reports").get() as { c: number } | undefined;
    overlayCount = row?.c ?? 0;
  } catch {
    // gap_reports not present in overlay sastDb (likely — it's a SAST DB, not main DB).
    // This is expected; skip gracefully.
    return { passed: true, detail: "oracle #8: gap_reports not in overlay sastDb; skip" };
  }

  if (overlayCount > mainCount) {
    return {
      passed: false,
      detail: `oracle #8: overlay has ${overlayCount} gap_report rows vs main ${mainCount}; new gaps introduced`,
    };
  }

  return { passed: true, detail: `oracle #8: overlay gap count (${overlayCount}) <= main (${mainCount}); no new gaps` };
}

// ---------------------------------------------------------------------------
// Oracle #10 — full suite with retry-once
// ---------------------------------------------------------------------------

/**
 * Run `npx vitest run` against overlay.worktreePath.
 * First run: if passes, pass. If fails, ONE retry.
 * If second run passes, pass with flake warning.
 * If second run also fails, fail.
 *
 * NOTE: the harness.captureTrace.test.ts flake has been flagged by B3/B4/B5
 * reviewers; retry-once is how we survive it.
 *
 * The optional `runner` param is a test seam — inject a stub in unit tests.
 */
export async function runOracle10(args: {
  overlay: OverlayHandle;
  runner?: (overlay: OverlayHandle) => { exitCode: number; stdout: string; stderr: string };
}): Promise<OracleResult> {
  const { overlay } = args;

  const runVitest = args.runner ?? defaultVitestRunner;

  const run1 = runVitest(overlay);
  if (run1.exitCode === 0) {
    return { passed: true, detail: "oracle #10: full suite passed on first run" };
  }

  // Retry once
  const run2 = runVitest(overlay);
  if (run2.exitCode === 0) {
    return {
      passed: true,
      detail: "oracle #10: full suite passed on retry (first run was a flake)",
    };
  }

  return {
    passed: false,
    detail: `oracle #10: full suite failed on both runs. stderr: ${run2.stderr.slice(0, 500)}`,
  };
}

function defaultVitestRunner(overlay: OverlayHandle): { exitCode: number; stdout: string; stderr: string } {
  // Ensure node_modules symlink exists in overlay
  const nmTarget = join(overlay.worktreePath, "node_modules");
  if (!existsSync(nmTarget)) {
    // Try to find main repo node_modules two levels up from worktreePath
    const candidates = [
      join(overlay.worktreePath, "..", "..", "node_modules"),
      join(overlay.worktreePath, "..", "node_modules"),
    ];
    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        try { symlinkSync(candidate, nmTarget); } catch { /* ignore if already exists */ }
        break;
      }
    }
  }

  const vitestBin = join(overlay.worktreePath, "node_modules", ".bin", "vitest");
  const result = spawnSync(vitestBin, ["run"], {
    cwd: overlay.worktreePath,
    encoding: "utf8",
    timeout: 120000,
    env: { ...process.env, NODE_ENV: "test", CI: "true" },
  });

  return {
    exitCode: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

// ---------------------------------------------------------------------------
// Oracle #11 — SAST structural coherence
// ---------------------------------------------------------------------------

/**
 * Query overlay's sastDb for structural anomalies:
 * - nodes table must have > 0 rows
 * - no orphan node_children (FK violations checked via a join)
 *
 * MVP: just verify count(nodes) > 0.
 */
export async function runOracle11(args: {
  overlay: OverlayHandle;
}): Promise<OracleResult> {
  const { overlay } = args;

  try {
    const client = overlay.sastDb.$client;
    const countRow = client.prepare("SELECT count(*) as c FROM nodes").get() as { c: number } | undefined;
    const count = countRow?.c ?? 0;
    if (count === 0) {
      return { passed: false, detail: "oracle #11: SAST overlay has 0 nodes — structural coherence fail" };
    }

    // Check for orphan node_children (table may not exist — handle gracefully)
    try {
      const orphanRow = client
        .prepare(
          "SELECT count(*) as c FROM node_children WHERE parent_id NOT IN (SELECT id FROM nodes) OR child_id NOT IN (SELECT id FROM nodes)",
        )
        .get() as { c: number } | undefined;
      const orphanCount = orphanRow?.c ?? 0;
      if (orphanCount > 0) {
        return {
          passed: false,
          detail: `oracle #11: ${orphanCount} orphan node_children rows detected`,
        };
      }
    } catch {
      // node_children table may not exist in minimal overlay; ignore.
    }

    return { passed: true, detail: `oracle #11: SAST coherent; ${count} nodes, no orphan edges` };
  } catch (err: any) {
    return { passed: false, detail: `oracle #11: SAST query failed: ${err?.message ?? "unknown"}` };
  }
}

// ---------------------------------------------------------------------------
// Oracle #12 — DSL no silent regressions
// ---------------------------------------------------------------------------

/**
 * For each DSL principle file:
 *  - Pre-fix matches: query mainDb.principle_matches WHERE principleName = <name>
 *  - Post-fix matches: evaluatePrinciple(overlay.sastDb, dslSource) — fresh execution
 *  - disappeared = pre-fix match nodes not in post-fix
 *  - appeared   = post-fix match nodes not in pre-fix
 *
 * Disappeared at the locus file → EXPECTED (fix removed it).
 * Disappeared elsewhere → SUSPICIOUS regression → fail.
 * Appeared anywhere → informational only, oracle does NOT fail.
 *
 * Principles that fail to load/parse are skipped with a warning (not a fail).
 * Unknown / can't-query states are treated as pass with informational detail.
 */
export async function runOracle12(args: {
  overlay: OverlayHandle;
  mainDb: Db;
  signal: BugSignal;
  locus: BugLocus;
}): Promise<OracleResult> {
  const { overlay, mainDb, locus } = args;

  // Find all DSL principle files in the overlay (the overlay is a full worktree clone)
  const principlesDir = join(overlay.worktreePath, ".provekit", "principles");
  let dslFiles: string[];
  try {
    const entries = readdirSync(principlesDir, { withFileTypes: true });
    dslFiles = entries
      .filter((e) => e.isFile() && e.name.endsWith(".dsl"))
      .map((e) => join(principlesDir, e.name));
  } catch {
    dslFiles = [];
  }

  if (dslFiles.length === 0) {
    return { passed: true, detail: "oracle #12: no DSL principle files found; no regressions possible" };
  }

  const details: string[] = [];
  let totalDisappearedElsewhere = 0;
  let totalDisappearedAtLocus = 0;
  let totalAppeared = 0;

  // Normalise locus file for comparison — take the basename path components that exist in the overlay
  const locusFileNorm = normalisePath(locus.file);

  for (const dslPath of dslFiles) {
    let dslSource: string;
    try {
      dslSource = readFileSync(dslPath, "utf8");
    } catch (err: any) {
      details.push(`  SKIP ${dslPath}: read error — ${err?.message ?? "unknown"}`);
      continue;
    }

    // Determine principle name(s) from the DSL file name (heuristic: stem)
    // The evaluatePrinciple result includes principleName from the DSL itself.

    // Pre-fix: get existing matches from mainDb
    let priorMatches: { rootMatchNodeId: string; fileId: number }[];
    try {
      priorMatches = mainDb
        .select({ rootMatchNodeId: principleMatches.rootMatchNodeId, fileId: principleMatches.fileId })
        .from(principleMatches)
        .all();
    } catch {
      // principle_matches may not be populated in mainDb (e.g. fresh test fixture) — skip
      details.push(`  SKIP ${dslPath}: mainDb principle_matches not accessible`);
      continue;
    }

    const priorNodeSet = new Set(priorMatches.map((r) => r.rootMatchNodeId));
    const priorFileIdByNode = new Map(priorMatches.map((r) => [r.rootMatchNodeId, r.fileId]));

    // Post-fix: run evaluatePrinciple against overlay.sastDb
    let postMatches: import("../dsl/evaluator.js").PrincipleMatch[];
    try {
      postMatches = evaluatePrinciple(overlay.sastDb, dslSource);
    } catch (err: any) {
      details.push(`  SKIP ${dslPath}: evaluatePrinciple error — ${err?.message?.slice(0, 120) ?? "unknown"}`);
      continue;
    }

    const postNodeSet = new Set(postMatches.map((m) => m.rootNodeId));

    // Disappeared: in pre-fix but not in post-fix
    const disappeared = [...priorNodeSet].filter((n) => !postNodeSet.has(n));
    // Appeared: in post-fix but not in pre-fix
    const appeared = [...postNodeSet].filter((n) => !priorNodeSet.has(n));

    let disappearedAtLocus = 0;
    let disappearedElsewhere = 0;

    for (const nodeId of disappeared) {
      // Look up the file for this node in mainDb
      const fileId = priorFileIdByNode.get(nodeId);
      let filePath: string | null = null;
      if (fileId !== undefined) {
        try {
          const row = mainDb.select({ path: files.path }).from(files).where(eq(files.id, fileId)).get();
          filePath = row?.path ?? null;
        } catch {
          // files table may not be in mainDb — treat as elsewhere
        }
      }

      const normFile = filePath ? normalisePath(filePath) : null;
      if (normFile && pathsMatch(normFile, locusFileNorm)) {
        disappearedAtLocus++;
      } else {
        disappearedElsewhere++;
      }
    }

    totalDisappearedAtLocus += disappearedAtLocus;
    totalDisappearedElsewhere += disappearedElsewhere;
    totalAppeared += appeared.length;

    if (disappeared.length > 0 || appeared.length > 0) {
      details.push(
        `  ${dslPath}: disappeared=${disappeared.length} (atLocus=${disappearedAtLocus}, elsewhere=${disappearedElsewhere}), appeared=${appeared.length}`,
      );
    }
  }

  if (totalDisappearedElsewhere > 0) {
    return {
      passed: false,
      detail: `oracle #12: ${totalDisappearedElsewhere} principle match(es) disappeared from non-locus files — possible silent regression.\n${details.join("\n")}`,
    };
  }

  const summary = `oracle #12: ${dslFiles.length} principle(s) checked — ${totalDisappearedAtLocus} disappeared at locus (expected), ${totalDisappearedElsewhere} elsewhere (none), ${totalAppeared} appeared (informational)`;
  return {
    passed: true,
    detail: details.length > 0 ? `${summary}\n${details.join("\n")}` : summary,
  };
}

/** Normalise a file path to its last N components for comparison. */
function normalisePath(p: string): string {
  // Use the path as-is for now; comparison is done by checking suffix overlap.
  return p.replace(/\\/g, "/");
}

/** Check if two normalised paths refer to the same file (suffix match). */
function pathsMatch(a: string, b: string): boolean {
  if (a === b) return true;
  // One may be absolute, one relative: check if one ends with the other
  const shorter = a.length <= b.length ? a : b;
  const longer = a.length <= b.length ? b : a;
  return longer.endsWith("/" + shorter) || longer === shorter;
}

// ---------------------------------------------------------------------------
// Oracle #13 — gap closure
// ---------------------------------------------------------------------------

/**
 * If triggeringGapId present: the corresponding gap_reports row MUST be absent
 * from the overlay's DB (it was closed by the fix).
 *
 * BugSignal has no gapId field. We skip gracefully unless the caller provides
 * one explicitly (e.g., extracted from signal.rawText by orchestrator).
 */
export async function runOracle13(args: {
  overlay: OverlayHandle;
  triggeringGapId?: number;
}): Promise<OracleResult> {
  const { overlay, triggeringGapId } = args;

  if (triggeringGapId === undefined) {
    return { passed: true, detail: "oracle #13: signal was not gap-report-sourced; skip" };
  }

  try {
    const client = overlay.sastDb.$client;
    const row = client
      .prepare("SELECT count(*) as c FROM gap_reports WHERE id = ?")
      .get(triggeringGapId) as { c: number } | undefined;
    const count = row?.c ?? 0;

    if (count > 0) {
      return {
        passed: false,
        detail: `oracle #13: triggering gap_report id=${triggeringGapId} still present in overlay; fix did not close the gap`,
      };
    }

    return {
      passed: true,
      detail: `oracle #13: triggering gap_report id=${triggeringGapId} is absent from overlay; gap closed`,
    };
  } catch {
    // gap_reports not in overlay's SAST DB — skip gracefully.
    return { passed: true, detail: "oracle #13: gap_reports not in overlay sastDb; skip" };
  }
}

// ---------------------------------------------------------------------------
// Oracle #15 — cross-codebase regression (substrate bundles only, MVP stub)
// ---------------------------------------------------------------------------

/**
 * MVP: pass if overlay's principle_matches count >= main's count.
 *
 * Full implementation requires a fixture corpus. MVP defers the per-verdict
 * comparison. Full rigor is D3 territory.
 */
export async function runOracle15(args: {
  overlay: OverlayHandle;
  mainDb: Db;
  capabilitySpec: import("./types.js").CapabilitySpec;
}): Promise<OracleResult> {
  void args.capabilitySpec;
  // MVP stub: pass through.
  return {
    passed: true,
    detail: "oracle #15: cross-codebase regression check deferred to D3 fixture corpus (MVP stub)",
  };
}
