/**
 * Step 5 of the standing-invariant-runtime spec: wire the path enumerator
 * (step 3) and Z3 path checker (step 4) into the verifier so each
 * invariant's verdict reflects actual whole-program reasoning, not just
 * binding decay.
 *
 * Verdict aggregation per invariant:
 *
 *   1. Resolve every binding against the current filesystem.
 *      Any binding decay  →  status: "decayed". Skip path checks.
 *
 *   2. Open the substrate (.provekit/provekit.db). Missing substrate
 *      →  status: "holds" with note "substrate not built; bindings
 *      resolve but path verification skipped". Per spec this is
 *      explicitly NOT a failure — the user just hasn't run `analyze`.
 *
 *   3. Resolve invariant.callsite to a substrate node id. If the node
 *      no longer exists  →  status: "decayed" with a synthetic
 *      "callsite no longer in substrate" entry. Same remediation as
 *      any other decay.
 *
 *   4. Enumerate paths via `pathsTo(callsiteNodeId)`. Check each via
 *      `checkPath`.
 *
 *   5. Aggregate per-path PathVerdicts:
 *
 *        any "violated"               →  status: "violated"
 *                                        (witness from the first
 *                                         violating path is surfaced)
 *        else all "holds" / mixed     →  status: "holds"
 *        else all "undecidable"       →  status: "holds" with note
 *                                        "all paths undecidable"
 *                                        (the spec-sanctioned gray
 *                                         zone — undecidable is a
 *                                         soft warn, not a CI fail)
 *
 * Decay categories (unchanged from step 2):
 *   - DECAYED_DELETED: file the binding pointed at no longer exists
 *   - DECAYED_CHANGED: file exists, but the line range is out of bounds,
 *                      or its content doesn't match the recorded node hash
 *   - DECAYED_SUBSTRATE: substrate exists but the callsite node doesn't
 *                        resolve (architecturally moved/extracted out)
 */

import { existsSync, readFileSync } from "fs";
import type { StoredInvariant } from "./invariantStore.js";
import { readInvariants } from "./invariantStore.js";
import { openSubstrateDb, resolveCallsiteNodeId } from "./substrate.js";
import { pathsTo } from "./pathEnumerator.js";
import { checkPath, type PathVerdict } from "./pathChecker.js";
import type { Db } from "../../db/index.js";

// ---------------------------------------------------------------------------
// Verdict types
// ---------------------------------------------------------------------------

export type DecayKind = "deleted" | "changed" | "substrate";

export interface BindingResolution {
  smt_constant: string;
  status: "resolved" | "decayed";
  decayKind?: DecayKind;
  reason?: string;
}

export interface InvariantVerdict {
  invariant: StoredInvariant;
  /**
   * - "holds":    every binding resolves AND every path is holds-or-undecidable
   *               (spec: undecidable is a soft warn, not a fail).
   * - "decayed":  at least one binding decays, OR substrate is present but
   *               the callsite node id no longer resolves.
   * - "violated": at least one enumerated path produced a "violated"
   *               PathVerdict; the failing witness is surfaced on this
   *               object.
   */
  status: "holds" | "decayed" | "violated";
  bindings: BindingResolution[];
  /**
   * One of:
   *   - "skipped":     decay short-circuited path verification, or
   *                    the substrate isn't built.
   *   - "holds":       Z3 proved the invariant on every enumerated path
   *                    (or returned undecidable — see undecidable note).
   *   - "violated":    at least one path produced a Z3 SAT witness.
   *   - "undecidable": every path returned undecidable; we treat that
   *                    as a soft warn per spec, but surface the
   *                    aggregate state separately so callers can render
   *                    a yellow-instead-of-green dot.
   */
  pathCheck: "skipped" | "holds" | "violated" | "undecidable";
  /** Total number of paths enumerated for this invariant (0 when skipped). */
  pathCount?: number;
  /**
   * Per-path verdicts. Populated when path checking ran. Useful for
   * debug + JSON output; not surfaced by the human formatter unless
   * verbose.
   */
  pathVerdicts?: PathVerdict[];
  /**
   * Z3 witness from the first violating path. Only populated on
   * status === "violated".
   */
  witness?: string;
  /**
   * Free-text annotation explaining why path verification was skipped
   * or why all paths returned undecidable. Surfaced by formatReport.
   */
  note?: string;
}

export interface VerifyReport {
  verdicts: InvariantVerdict[];
  /** Counts for human-readable summary + exit-code derivation. */
  summary: {
    total: number;
    holds: number;
    decayed: number;
    violated: number;
  };
}

// ---------------------------------------------------------------------------
// Binding resolution
// ---------------------------------------------------------------------------

/**
 * Attempt to resolve every binding in an invariant against the current
 * filesystem. Returns one BindingResolution per binding. The invariant
 * itself is "decayed" if any binding decays; "holds" only if every
 * binding resolves cleanly.
 *
 * In v1 we check:
 *   1. binding.node.filePath exists in the project
 *   2. the file's line count covers binding.node.startLine and endLine
 *   3. (when nodeHash is non-empty) the recorded line range's content
 *      hashes to the recorded value
 */
export function resolveBindings(
  invariant: StoredInvariant,
  projectRoot: string,
): BindingResolution[] {
  const out: BindingResolution[] = [];
  for (const b of invariant.bindings) {
    const absPath = resolveAbs(projectRoot, b.node.filePath);

    if (!existsSync(absPath)) {
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "deleted",
        reason: `file not found: ${b.node.filePath}`,
      });
      continue;
    }

    let content: string;
    try {
      content = readFileSync(absPath, "utf-8");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "changed",
        reason: `read failed: ${msg}`,
      });
      continue;
    }

    const lines = content.split("\n");
    if (b.node.startLine > lines.length || b.node.endLine > lines.length) {
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "changed",
        reason:
          `line range ${b.node.startLine}-${b.node.endLine} exceeds file length ${lines.length}`,
      });
      continue;
    }

    if (b.node.nodeHash) {
      const span = lines.slice(b.node.startLine - 1, b.node.endLine).join("\n");
      const currentHash = sha256Prefix16(span);
      if (currentHash !== b.node.nodeHash) {
        out.push({
          smt_constant: b.smt_constant,
          status: "decayed",
          decayKind: "changed",
          reason: `node hash mismatch (was ${b.node.nodeHash}, now ${currentHash})`,
        });
        continue;
      }
    }

    out.push({ smt_constant: b.smt_constant, status: "resolved" });
  }
  return out;
}

function sha256Prefix16(s: string): string {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { createHash } = require("crypto");
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function resolveAbs(projectRoot: string, p: string): string {
  if (p.startsWith("/")) return p;
  return `${projectRoot}/${p}`;
}

// ---------------------------------------------------------------------------
// Per-invariant path verification
// ---------------------------------------------------------------------------

export interface VerifyOptions {
  /** Per-Z3-query timeout in milliseconds. Defaults to checkPath's 30s. */
  timeoutMs?: number;
  /** Path enumeration cap. Defaults to pathsTo's 50. */
  maxPaths?: number;
}

/**
 * Run path enumeration + Z3 checking for one invariant. Caller has
 * already confirmed all bindings resolve. The substrate handle is
 * required.
 *
 * Returns the aggregated path-level verdict shape:
 *
 *   - { kind: "violated", witness, pathVerdicts, pathCount }
 *   - { kind: "holds", pathVerdicts, pathCount }
 *   - { kind: "undecidable", pathVerdicts, pathCount, note }
 *   - { kind: "callsite_decayed", reason }
 */
type PathPhaseResult =
  | { kind: "violated"; witness?: string; pathVerdicts: PathVerdict[]; pathCount: number }
  | { kind: "holds"; pathVerdicts: PathVerdict[]; pathCount: number }
  | { kind: "undecidable"; pathVerdicts: PathVerdict[]; pathCount: number; note: string }
  | { kind: "callsite_decayed"; reason: string };

async function runPathPhase(
  invariant: StoredInvariant,
  db: Db,
  projectRoot: string,
  options: VerifyOptions,
): Promise<PathPhaseResult> {
  const callsiteNodeId = resolveCallsiteNodeId(
    db,
    invariant.callsite.filePath,
    invariant.callsite.startLine,
  );
  if (!callsiteNodeId) {
    return {
      kind: "callsite_decayed",
      reason:
        `callsite ${invariant.callsite.filePath}:${invariant.callsite.startLine} ` +
        `no longer resolves in substrate (re-run \`provekit analyze\` first; if the function moved, re-run \`provekit fix\` on the new locus)`,
    };
  }

  const paths = pathsTo(db, callsiteNodeId, {
    maxPaths: options.maxPaths,
  });

  if (paths.length === 0) {
    // No paths to the callsite: in dataflow terms, nothing flows into
    // this node. Treat as holds with a note — vacuously true is still
    // true. (Adversarial scan via --adversarial in step 7 would walk
    // beyond the callsite to other sinks; that's not v1 of this CLI.)
    return {
      kind: "holds",
      pathVerdicts: [],
      pathCount: 0,
    };
  }

  const pathVerdicts: PathVerdict[] = [];
  for (const path of paths) {
    const v = await checkPath(path, invariant, db, {
      timeoutMs: options.timeoutMs,
      projectRoot,
    });
    pathVerdicts.push(v);
    // Early-out: a violation aborts further path checking. The first
    // failing witness is sufficient evidence; we save Z3 budget for
    // the next invariant.
    if (v.status === "violated") {
      return {
        kind: "violated",
        witness: v.witness,
        pathVerdicts,
        pathCount: paths.length,
      };
    }
  }

  const allUndecidable = pathVerdicts.every((v) => v.status === "undecidable");
  if (allUndecidable && pathVerdicts.length > 0) {
    return {
      kind: "undecidable",
      pathVerdicts,
      pathCount: paths.length,
      note: `${pathVerdicts.length} path(s) all undecidable; v1 symbolic execution did not produce an informative constraint`,
    };
  }

  return { kind: "holds", pathVerdicts, pathCount: paths.length };
}

// ---------------------------------------------------------------------------
// Top-level verify
// ---------------------------------------------------------------------------

/**
 * Load every (non-retired) invariant, resolve bindings, then for each
 * invariant whose bindings resolve cleanly: open the substrate, resolve
 * the callsite, enumerate paths, Z3-check each path, aggregate.
 *
 * Substrate-missing case: per spec, NOT a failure. We surface the
 * invariant as "holds" with a note. Users get useful decay reporting
 * even before they've run `provekit analyze`.
 */
export async function verifyAll(
  projectRoot: string,
  options: VerifyOptions = {},
): Promise<VerifyReport> {
  const invariants = readInvariants(projectRoot);

  // Open substrate ONCE for the whole batch — it's a read-only handle.
  const db = openSubstrateDb(projectRoot);

  const verdicts: InvariantVerdict[] = [];
  for (const inv of invariants) {
    const bindings = resolveBindings(inv, projectRoot);
    const anyDecayed = bindings.some((b) => b.status === "decayed");

    if (anyDecayed) {
      verdicts.push({
        invariant: inv,
        status: "decayed",
        bindings,
        pathCheck: "skipped",
      });
      continue;
    }

    if (!db) {
      verdicts.push({
        invariant: inv,
        status: "holds",
        bindings,
        pathCheck: "skipped",
        note: "substrate not built; bindings resolve but path verification skipped (run `provekit analyze` to enable)",
      });
      continue;
    }

    const phase = await runPathPhase(inv, db, projectRoot, options);

    if (phase.kind === "callsite_decayed") {
      verdicts.push({
        invariant: inv,
        status: "decayed",
        bindings: [
          ...bindings,
          {
            smt_constant: "<callsite>",
            status: "decayed",
            decayKind: "substrate",
            reason: phase.reason,
          },
        ],
        pathCheck: "skipped",
      });
      continue;
    }

    if (phase.kind === "violated") {
      verdicts.push({
        invariant: inv,
        status: "violated",
        bindings,
        pathCheck: "violated",
        pathCount: phase.pathCount,
        pathVerdicts: phase.pathVerdicts,
        witness: phase.witness,
      });
      continue;
    }

    if (phase.kind === "undecidable") {
      verdicts.push({
        invariant: inv,
        status: "holds",
        bindings,
        pathCheck: "undecidable",
        pathCount: phase.pathCount,
        pathVerdicts: phase.pathVerdicts,
        note: phase.note,
      });
      continue;
    }

    // phase.kind === "holds"
    verdicts.push({
      invariant: inv,
      status: "holds",
      bindings,
      pathCheck: "holds",
      pathCount: phase.pathCount,
      pathVerdicts: phase.pathVerdicts,
    });
  }

  const summary = {
    total: verdicts.length,
    holds: verdicts.filter((v) => v.status === "holds").length,
    decayed: verdicts.filter((v) => v.status === "decayed").length,
    violated: verdicts.filter((v) => v.status === "violated").length,
  };

  return { verdicts, summary };
}

// ---------------------------------------------------------------------------
// Human-readable formatting
// ---------------------------------------------------------------------------

export function formatReport(report: VerifyReport, options: { verbose?: boolean } = {}): string {
  const lines: string[] = [];
  const { summary, verdicts } = report;

  lines.push(`provekit verify: ${summary.total} invariant${summary.total === 1 ? "" : "s"}`);

  if (summary.total === 0) {
    lines.push(`  (no invariants in .provekit/invariants/)`);
    lines.push(`  the standing runtime activates as soon as the fix loop ships its first bundle.`);
    return lines.join("\n");
  }

  if (summary.holds > 0) {
    const holdVerdicts = verdicts.filter((v) => v.status === "holds");
    if (options.verbose) {
      lines.push(`  holds (${summary.holds}):`);
      for (const v of holdVerdicts) {
        const tag = v.pathCheck === "undecidable"
          ? " [paths undecidable]"
          : v.pathCheck === "skipped"
            ? " [paths skipped]"
            : v.pathCount !== undefined
              ? ` [${v.pathCount} path${v.pathCount === 1 ? "" : "s"} checked]`
              : "";
        lines.push(`    ${v.invariant.id}${tag} — ${v.invariant.originatingBug.slice(0, 80)}`);
        if (v.note) lines.push(`        note: ${v.note}`);
      }
    } else {
      lines.push(`  holds (${summary.holds}): ${summary.holds} invariant${summary.holds === 1 ? "" : "s"} pass binding resolution + path verification`);
    }
  }

  for (const v of verdicts.filter((x) => x.status === "decayed")) {
    lines.push(`  decay: ${v.invariant.id} — ${v.invariant.originatingBug.slice(0, 80)}`);
    for (const b of v.bindings.filter((x) => x.status === "decayed")) {
      lines.push(`      binding ${b.smt_constant} (${b.decayKind}): ${b.reason}`);
    }
    lines.push(
      `      remediation: re-run \`provekit fix\` against this locus, or retire with \`provekit invariants retire ${v.invariant.id}\``,
    );
  }

  for (const v of verdicts.filter((x) => x.status === "violated")) {
    lines.push(`  violated: ${v.invariant.id} — ${v.invariant.originatingBug.slice(0, 80)}`);
    lines.push(`      callsite: ${v.invariant.callsite.filePath}:${v.invariant.callsite.startLine}`);
    if (v.pathCount !== undefined) {
      const failing = v.pathVerdicts?.findIndex((p) => p.status === "violated");
      const failTag = failing !== undefined && failing >= 0 ? ` (failing path #${failing + 1} of ${v.pathCount})` : "";
      lines.push(`      paths: ${v.pathCount} enumerated${failTag}`);
    }
    if (v.witness) {
      // Indent the multi-line Z3 model for readability.
      lines.push(`      Z3 witness:`);
      for (const wl of v.witness.split("\n")) {
        lines.push(`        ${wl}`);
      }
    }
    lines.push(
      `      remediation: inspect the failing path; the invariant claims a property the current source contradicts. ` +
      `Re-run \`provekit fix\` to re-derive, or retire if the invariant is no longer wanted.`,
    );
  }

  return lines.join("\n");
}

/**
 * Map a verify report to the spec's exit codes:
 *   0 = all hold or undecidable
 *   1 = at least one violation
 *   2 = at least one decay, no violations
 *   3 = internal error (handled by the caller)
 */
export function exitCodeFor(report: VerifyReport): number {
  if (report.summary.violated > 0) return 1;
  if (report.summary.decayed > 0) return 2;
  return 0;
}
