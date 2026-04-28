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
 *
 * Step 7 (adversarial cross-path scan):
 *
 *   For invariants with `scope: "sink"`, --adversarial mode iterates
 *   `reverseReachableNodes(db, callsiteNodeId)` and runs `pathsTo` from
 *   EACH upstream node. The same Z3 checker runs on each enumerated
 *   path. Catches "new method introduced with same bug class" and
 *   "upstream data-path refactor adds a new path" — both invisible to
 *   single-callsite enumeration. Callsite-scoped invariants ignore the
 *   flag (acceptance: "for callsite-scoped invariants, behavior is
 *   unchanged from non-adversarial mode").
 */

import { existsSync, readFileSync } from "fs";
import type { StoredInvariant } from "./invariantStore.js";
import { readInvariants } from "./invariantStore.js";
import { openSubstrateDb, resolveCallsiteNodeId } from "./substrate.js";
import { pathsTo, reverseReachableNodes } from "./pathEnumerator.js";
import { checkPath, type PathVerdict } from "./pathChecker.js";
import type { Path } from "./pathEnumerator.js";
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
  /**
   * Step 7 (adversarial cross-path scan): when this verdict was produced
   * under --adversarial AND the invariant is scope="sink", the scanner
   * walks `reverseReachableNodes(callsite)` and runs path enumeration
   * from each upstream node. This block records the scope of the scan +
   * the upstream entry for the violating path (if any).
   *
   * Absent on non-adversarial verdicts and on adversarial verdicts for
   * callsite-scoped invariants (acceptance: callsite-scoped invariants
   * see no behavior change under --adversarial).
   */
  adversarial?: {
    /**
     * Number of nodes in `reverseReachableNodes(db, callsiteNodeId)` —
     * i.e., how broad the cross-path scan was for this invariant.
     */
    reverseReachableCount: number;
    /**
     * The upstream nodeId that anchored the violating path's
     * enumeration. Only set when status === "violated"; this is the
     * "first step" the formatter surfaces as the path entry.
     */
    violatingEntryNodeId?: string;
  };
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
  /**
   * Step 7: adversarial cross-path scan. When true, invariants whose
   * scope === "sink" are checked across the entire reverse-reachable
   * set of the callsite (every node that can transitively feed the
   * sink), not just the original callsite. Callsite-scoped invariants
   * ignore the flag.
   *
   * Defaults to false. Even when true, performance is bounded by
   * `maxPaths` (per-node enumeration cap, default 200 in adversarial
   * mode) and `adversarialPathBudget` (global cap on Z3 invocations
   * across all reverse-reachable nodes, default 1000).
   */
  adversarial?: boolean;
  /**
   * Step 7: global cap on the total number of paths checked per
   * adversarial invariant. The product of `reverseReachableNodes.length`
   * and `maxPaths` can otherwise blow the spec's 30s-2min budget.
   * Default 1000. Early-out on first violation still applies.
   */
  adversarialPathBudget?: number;
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
  // Resolve relative filePath to absolute before substrate lookup. The
  // invariant persists relative paths (relative to project root), but the
  // substrate `files` table stores absolute paths. resolveAbs is the same
  // helper used for binding lookups — apply the same normalization here.
  const callsiteAbsPath = resolveAbs(projectRoot, invariant.callsite.filePath);
  const callsiteNodeId = resolveCallsiteNodeId(
    db,
    callsiteAbsPath,
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
// Adversarial cross-path scan (step 7)
// ---------------------------------------------------------------------------

/**
 * Aggregated result of an adversarial cross-path scan over a single
 * sink-scoped invariant. Same shape as `PathPhaseResult` but with
 * `reverseReachableCount` + `violatingEntryNodeId` so the verdict can
 * surface "which upstream node fed the violating path."
 *
 * `pathVerdicts` is capped at 100 entries (matches the task spec) to
 * bound memory; `pathCount` reflects the real total of paths checked
 * across all reverse-reachable nodes.
 */
type AdversarialPhaseResult =
  | {
      kind: "violated";
      witness?: string;
      pathVerdicts: PathVerdict[];
      pathCount: number;
      reverseReachableCount: number;
      violatingEntryNodeId?: string;
    }
  | {
      kind: "holds";
      pathVerdicts: PathVerdict[];
      pathCount: number;
      reverseReachableCount: number;
    }
  | {
      kind: "undecidable";
      pathVerdicts: PathVerdict[];
      pathCount: number;
      reverseReachableCount: number;
      note: string;
    }
  | { kind: "callsite_decayed"; reason: string };

const ADVERSARIAL_DEFAULT_MAX_PATHS = 200;
const ADVERSARIAL_DEFAULT_PATH_BUDGET = 1000;
const ADVERSARIAL_MAX_PATH_VERDICTS_RETAINED = 100;

/**
 * Step 7: enumerate every node that can transitively feed the sink, run
 * `pathsTo` from each, and Z3-check each path. Return as soon as we hit
 * a violation. Path enumeration from each upstream node already produces
 * source→node paths; Z3's symbolic execution evaluates the invariant
 * along that path, which is exactly the property the spec wants
 * checked ("if any reverse-reachable upstream method violates, the
 * invariant fires"). The reachability that the upstream node feeds the
 * sink is implicit in it being in the reverse-reachable set.
 *
 * Empty reverse-reachable set → fall through to standard callsite
 * behavior; the callsite is vacuously the only relevant path source.
 */
async function runAdversarialPathPhase(
  invariant: StoredInvariant,
  db: Db,
  projectRoot: string,
  options: VerifyOptions,
): Promise<AdversarialPhaseResult> {
  // Same absolute-path normalization as runPathPhase. Invariant stores
  // relative paths; substrate files table has absolute paths.
  const callsiteAbsPath = resolveAbs(projectRoot, invariant.callsite.filePath);
  const callsiteNodeId = resolveCallsiteNodeId(
    db,
    callsiteAbsPath,
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

  const reverseSet = reverseReachableNodes(db, callsiteNodeId);

  // The transitive-closure table is reflexive in some substrate variants
  // and not in others; defensively ensure the callsite itself is part
  // of the scan. Adversarial mode SHOULD include the original callsite.
  const upstreamNodes = new Set<string>(reverseSet);
  upstreamNodes.add(callsiteNodeId);

  const reverseReachableCount = upstreamNodes.size;

  const perNodeMax = options.maxPaths ?? ADVERSARIAL_DEFAULT_MAX_PATHS;
  const globalBudget =
    options.adversarialPathBudget ?? ADVERSARIAL_DEFAULT_PATH_BUDGET;

  const pathVerdicts: PathVerdict[] = [];
  let totalPathsChecked = 0;
  let budgetExhausted = false;

  // Iterate in deterministic order so two runs against the same
  // substrate produce the same first-violating-path. Map.values() of a
  // Set preserves insertion order; lift to an array + sort.
  const orderedNodes = [...upstreamNodes].sort();

  for (const upstream of orderedNodes) {
    if (totalPathsChecked >= globalBudget) {
      budgetExhausted = true;
      break;
    }

    const remaining = globalBudget - totalPathsChecked;
    const cap = Math.min(perNodeMax, remaining);
    const paths: Path[] = pathsTo(db, upstream, { maxPaths: cap });
    if (paths.length === 0) continue;

    for (const path of paths) {
      const v = await checkPath(path, invariant, db, {
        timeoutMs: options.timeoutMs,
        projectRoot,
      });
      totalPathsChecked++;

      // Bound memory: only retain the first N path verdicts, but keep
      // checking — a violation 200 paths in still matters.
      if (pathVerdicts.length < ADVERSARIAL_MAX_PATH_VERDICTS_RETAINED) {
        pathVerdicts.push(v);
      }

      if (v.status === "violated") {
        // Violating path's "upstream entry" is the first step's nodeId
        // (the path is ordered source-first, sink-last).
        const entry =
          path.steps.length > 0 ? path.steps[0]!.nodeId : upstream;
        return {
          kind: "violated",
          witness: v.witness,
          pathVerdicts,
          pathCount: totalPathsChecked,
          reverseReachableCount,
          violatingEntryNodeId: entry,
        };
      }

      if (totalPathsChecked >= globalBudget) {
        budgetExhausted = true;
        break;
      }
    }

    if (budgetExhausted) break;
  }

  if (totalPathsChecked === 0) {
    // No paths anywhere in the reverse-reachable set: vacuously holds.
    // Same posture as the single-callsite phase's empty-paths branch.
    return {
      kind: "holds",
      pathVerdicts,
      pathCount: 0,
      reverseReachableCount,
    };
  }

  const allUndecidable =
    pathVerdicts.length > 0 &&
    pathVerdicts.every((v) => v.status === "undecidable");
  if (allUndecidable) {
    const budgetSuffix = budgetExhausted
      ? ` (path budget ${globalBudget} reached; further paths not checked)`
      : "";
    return {
      kind: "undecidable",
      pathVerdicts,
      pathCount: totalPathsChecked,
      reverseReachableCount,
      note:
        `${totalPathsChecked} adversarial path(s) across ${reverseReachableCount} reverse-reachable node(s) all undecidable; ` +
        `v1 symbolic execution did not produce an informative constraint${budgetSuffix}`,
    };
  }

  return {
    kind: "holds",
    pathVerdicts,
    pathCount: totalPathsChecked,
    reverseReachableCount,
  };
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

    // Step 7 dispatch: sink-scoped invariants under --adversarial run
    // through the cross-path scan. Callsite-scoped invariants stay on
    // the original single-callsite path even when --adversarial is set
    // (acceptance: callsite-scoped behavior is unchanged).
    const isAdversarialRun =
      options.adversarial === true && inv.scope === "sink";

    if (isAdversarialRun) {
      const phase = await runAdversarialPathPhase(inv, db, projectRoot, options);

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
          note:
            `adversarial scan: violation found across ${phase.reverseReachableCount} reverse-reachable node(s)`,
          adversarial: {
            reverseReachableCount: phase.reverseReachableCount,
            violatingEntryNodeId: phase.violatingEntryNodeId,
          },
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
          adversarial: {
            reverseReachableCount: phase.reverseReachableCount,
          },
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
        note:
          `adversarial scan: ${phase.pathCount} path(s) across ${phase.reverseReachableCount} reverse-reachable node(s) all hold`,
        adversarial: {
          reverseReachableCount: phase.reverseReachableCount,
        },
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
    if (v.adversarial) {
      // Sink-scoped + adversarial: name the sink so it's clear the
      // violation is upstream of the original callsite.
      const fnTag = v.invariant.callsite.function
        ? ` (fn ${v.invariant.callsite.function})`
        : "";
      lines.push(
        `      sink: ${v.invariant.callsite.filePath}:${v.invariant.callsite.startLine}${fnTag} [scope=sink, adversarial]`,
      );
      lines.push(
        `      reverse-reachable nodes: ${v.adversarial.reverseReachableCount}`,
      );
      if (v.adversarial.violatingEntryNodeId) {
        lines.push(
          `      upstream entry of violating path: ${v.adversarial.violatingEntryNodeId}`,
        );
      }
    } else {
      lines.push(`      callsite: ${v.invariant.callsite.filePath}:${v.invariant.callsite.startLine}`);
    }
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
    // For adversarial verdicts, surface the failing path's full step
    // list (nodeIds + slot labels) so the human reader can trace it.
    if (v.adversarial && v.pathVerdicts && v.pathVerdicts.length > 0) {
      const failing = v.pathVerdicts.findIndex((p) => p.status === "violated");
      if (failing >= 0) {
        const failedPath = v.pathVerdicts[failing];
        if (failedPath?.reason) {
          lines.push(`      reason: ${failedPath.reason}`);
        }
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
