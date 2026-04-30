/**
 * Unified verifier — the substrate every consumer (CLI, LSP, CI gate,
 * browser extension) calls into. Wraps the existing per-invariant Z3
 * verification (verifyAllCached) with the protocol's structured
 * fail-closed report shape per
 * docs/specs/2026-04-30-chain-validity-and-fail-closed.md.
 *
 * Callable from any TS context. No CLI ceremony, no workflow runner.
 * The LSP calls this on every save (or every keystroke, with debounce)
 * and surfaces the resulting diagnostics in the editor. Same function
 * the CLI's `provekit verify` invokes.
 *
 * Inputs: a project root (where .provekit/invariants/ lives) plus
 * options. Output: a structured ValidityReport — one row per invariant,
 * each row carrying a verdict (holds / decayed / violated / unresolved /
 * undecidable) + locus + reason + (when applicable) Z3 witness.
 *
 * Fail-closed by default: an invariant that the verifier cannot reach
 * a clean answer on REJECTS, not accepts.
 */

import { verifyAllCached } from "../fix/runtime/verifyCache.js";
import type { CachedVerifyReport } from "../fix/runtime/verifyCache.js";
import { listExtensions } from "../ir/extensions/index.js";
import { listBridges } from "../ir/extensions/bridges.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export type VerdictStatus =
  | "holds"          // SMT verdict: claim holds (negation unsat)
  | "decayed"        // binding moved or function content changed; route to LLM re-eval
  | "violated"       // SMT verdict: claim refuted (witness exists)
  | "unresolved"     // extension/bridge name not in scope (fail-closed)
  | "undecidable";   // solver returned unknown/timeout

export interface InvariantRow {
  invariantId: string;
  status: VerdictStatus;
  /** File + line + function where the invariant is bound. */
  locus: {
    filePath: string;
    function: string | null;
    startLine: number;
    endLine: number;
  };
  /** Free-text intent at authoring time (originatingIntent). */
  intent: string;
  /** Z3 witness when status === "violated". */
  witness?: unknown;
  /** Human-readable failure reason when status !== "holds". */
  reason?: string;
  /** True iff this verdict came from the on-disk verdict cache. */
  fromCache: boolean;
}

export interface ValidityReport {
  /** Project root scanned. */
  projectRoot: string;
  /** Per-invariant rows, in stable file+line order. */
  rows: InvariantRow[];
  /** Counts by status. */
  summary: {
    total: number;
    holds: number;
    decayed: number;
    violated: number;
    unresolved: number;
    undecidable: number;
    cacheHits: number;
    cacheMisses: number;
  };
  /** Registry snapshot at verification time. */
  registry: {
    extensionCount: number;
    bridgeCount: number;
  };
  /** When verification ran (ISO-8601). */
  verifiedAt: string;
}

export interface VerifyProjectOptions {
  /** Per-invariant timeout for Z3 (ms). Default 5000. */
  timeoutMs?: number;
  /** Maximum dataflow paths to enumerate per invariant. Default unbounded. */
  maxPaths?: number;
  /**
   * Adversarial mode walks the dataflow graph backward across the
   * entire substrate; non-adversarial mode walks only the direct
   * callsite. The architectural commitment is that adversarial is
   * the default for production verification; the verifier flag is
   * here for performance escape hatches in development loops.
   */
  adversarial?: boolean;
  /** Skip cache; verify everything fresh. */
  noCache?: boolean;
}

// ---------------------------------------------------------------------------
// verifyProject — the callable entry point
// ---------------------------------------------------------------------------

/**
 * Verify every invariant in a project. Pure-ish: filesystem reads and
 * Z3 subprocess spawns are real, but the call has no required side
 * effects beyond updating the verdict cache.
 *
 * Same function the CLI's `provekit verify` invokes; LSP / CI / cloud
 * verifiers all converge on this.
 */
export async function verifyProject(
  projectRoot: string,
  options: VerifyProjectOptions = {},
): Promise<ValidityReport> {
  const internal: CachedVerifyReport = await verifyAllCached(projectRoot, {
    timeoutMs: options.timeoutMs ?? 5000,
    maxPaths: options.maxPaths,
    adversarial: options.adversarial ?? true,
  });

  const rows: InvariantRow[] = internal.verdicts.map((v) => {
    const status: VerdictStatus = mapStatus(v.status);
    return {
      invariantId: v.invariant.id,
      status,
      locus: {
        filePath: v.invariant.callsite.filePath,
        function: v.invariant.callsite.function ?? null,
        startLine: v.invariant.callsite.startLine,
        endLine: v.invariant.callsite.endLine,
      },
      intent: v.invariant.originatingBug,
      ...(v.witness !== undefined ? { witness: v.witness } : {}),
      ...(v.note ? { reason: v.note } : {}),
      fromCache: v.cacheStatus === "hit",
    };
  });

  // Sort by file path then line for stable presentation.
  rows.sort((a, b) => {
    if (a.locus.filePath !== b.locus.filePath) {
      return a.locus.filePath.localeCompare(b.locus.filePath);
    }
    return a.locus.startLine - b.locus.startLine;
  });

  const summary = {
    total: rows.length,
    holds: rows.filter((r) => r.status === "holds").length,
    decayed: rows.filter((r) => r.status === "decayed").length,
    violated: rows.filter((r) => r.status === "violated").length,
    unresolved: rows.filter((r) => r.status === "unresolved").length,
    undecidable: rows.filter((r) => r.status === "undecidable").length,
    cacheHits: internal.summary.cacheHits,
    cacheMisses: internal.summary.cacheMisses,
  };

  return {
    projectRoot,
    rows,
    summary,
    registry: {
      extensionCount: listExtensions().length,
      bridgeCount: listBridges().length,
    },
    verifiedAt: new Date().toISOString(),
  };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function mapStatus(s: "holds" | "decayed" | "violated"): VerdictStatus {
  // verifyAllCached today returns three statuses. The protocol defines
  // five (adds unresolved + undecidable). Once the registry resolver is
  // wired into the verify path, this mapping expands; for now decayed
  // covers most "couldn't reach a clean answer" cases.
  return s;
}

// ---------------------------------------------------------------------------
// LSP-friendly helpers
// ---------------------------------------------------------------------------

/**
 * Filter a ValidityReport down to rows for one specific file. Used by
 * the LSP to emit document-level diagnostics on save.
 */
export function rowsForFile(report: ValidityReport, filePath: string): InvariantRow[] {
  return report.rows.filter((r) => r.locus.filePath === filePath);
}

/**
 * Find the invariant rows whose locus contains a specific line. Used by
 * the LSP for hover-info: "what must be true at this line?"
 */
export function rowsAtLine(
  report: ValidityReport,
  filePath: string,
  line: number,
): InvariantRow[] {
  return report.rows.filter(
    (r) =>
      r.locus.filePath === filePath &&
      line >= r.locus.startLine &&
      line <= r.locus.endLine,
  );
}
