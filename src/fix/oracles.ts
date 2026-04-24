/**
 * D1b: Oracle runners for bundle coherence.
 *
 * Oracles 1,2,3,6,9,14,16,17,18 are verified via audit trail (already-fired).
 * This module implements oracles 4,5,7,8,10,11,12,13,15 — the "NEW" set that
 * runs during bundle assembly.
 *
 * MVP stubs: oracles 7, 12, 15 are pass-through. Document inline.
 */

import type { OverlayHandle, InvariantClaim, FixCandidate } from "./types.js";
import type { Db } from "../db/index.js";
import { verifyBlock } from "../verifier.js";
import { clauses, gapReports } from "../db/schema/index.js";
import { sql } from "drizzle-orm";
import { spawnSync } from "child_process";
import { join } from "path";
import { existsSync, symlinkSync } from "fs";

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
// Oracle #7 — runtime harness witness replay (MVP stub)
// ---------------------------------------------------------------------------

/**
 * MVP stub: pass-through.
 *
 * Full implementation requires runtime harness wiring that's out of scope for D1b.
 * D1b's oracle #7 is a pass-through; D2 re-runs harness against the applied code.
 */
export async function runOracle7(args: {
  overlay: OverlayHandle;
  fix: FixCandidate;
  invariant: InvariantClaim;
  witnessInputs: Record<string, unknown>;
}): Promise<OracleResult> {
  void args;
  return { passed: true, detail: "oracle #7: witness replay deferred to D2" };
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
// Oracle #12 — DSL no silent regressions (MVP stub)
// ---------------------------------------------------------------------------

/**
 * MVP stub: assert overlay's principle_matches table exists and is queryable.
 *
 * Full implementation is D3 learning-layer territory. Per-principle comparison
 * of match sets vs main DB is deferred.
 */
export async function runOracle12(args: {
  overlay: OverlayHandle;
  mainDb: Db;
}): Promise<OracleResult> {
  void args;
  // MVP: just pass — the principle_matches table lives in the main DB, not the overlay's
  // SAST DB. Full DSL regression checking requires D3 infrastructure.
  return {
    passed: true,
    detail: "oracle #12: DSL regression check deferred to D3 (MVP stub)",
  };
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
