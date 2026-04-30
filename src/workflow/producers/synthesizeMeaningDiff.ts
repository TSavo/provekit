/**
 * synthesizeMeaningDiff Stage — turn a diff-invariant-snapshots output
 * into the forensic punch list.
 *
 * Per row, attaches:
 *   - the code locus (file + function + line)
 *   - for `renamed` and `changed`: a directional implication verdict
 *     (strengthened / weakened / equivalent / incomparable / undecidable),
 *     computed by running check-implication on the SMT bodies
 *
 * Pure given the diff input. No LLM. The Z3 implication probes are run
 * inline by composing the check-implication Stage's logic — same module
 * boundary, same caching guarantees.
 *
 * Output is consumed by the diff workflow's terminal Action that prints
 * the human-readable report (or by other consumers wanting structured
 * forensic data).
 */

import type { Stage } from "../types.js";
import type {
  DiffInvariantSnapshotsOutput,
  DiffRow,
} from "./diffInvariantSnapshots.js";
export type { DiffRow };
import {
  makeCheckImplicationStage,
  type CheckImplicationOutput,
  type ImplicationVerdict,
  type Solver,
} from "./checkImplication.js";

export const SYNTHESIZE_MEANING_DIFF_CAPABILITY = "synthesize-meaning-diff";

export interface SynthesizeMeaningDiffInput {
  diff: DiffInvariantSnapshotsOutput;
  /**
   * The framework's solver — one or more SMT-LIB-2.6-conformant entries
   * composed under agreement semantics. The framework calls it
   * uniformly; whether it runs one binary or three is internal.
   *
   * When omitted, falls back to a single-entry Z3 invocation.
   */
  solver?: Solver;
}

export interface ForensicRow {
  kind: DiffRow["kind"];
  /** Stable identifier for the row; for renamed pairs we use `${fromId}→${toId}`. */
  rowId: string;
  /** Locus from the relevant invariant: `to` for added/changed/renamed/preserved, `from` for removed. */
  locus: {
    filePath: string;
    function: string | null;
    startLine: number;
    endLine: number;
  };
  /** The propertyHash CIDs involved. fromId is null for added rows; toId is null for removed. */
  fromId: string | null;
  toId: string | null;
  /** Free-text intent for human readability. */
  originatingBug: string;
  /**
   * Direction verdict. Present for `renamed` and `changed` rows.
   * - strengthened: new claim ⊨ old claim, but not the other way
   * - weakened:     old claim ⊨ new claim, but not the other way
   * - equivalent:   each ⊨ the other (semantic content unchanged; usually canonical-form refactor)
   * - incomparable: each accepts inputs the other rejects
   * - undecidable:  Z3 returned unknown/timeout on at least one direction
   */
  directionVerdict?: ImplicationVerdict;
  /** Underlying solver verdicts for transparency. */
  implicationProbes?: {
    newImpliesOld: CheckImplicationOutput["newImpliesOld"];
    oldImpliesNew: CheckImplicationOutput["oldImpliesNew"];
  };
  /**
   * Per-solver-entry verdicts when the framework's solver had more than
   * one entry. Each item pairs the entry's `type` label with its
   * directional verdict. `allAgreed` true iff every entry returned the
   * same final verdict.
   */
  perSolverVerdicts?: Array<{ solverType: string; verdict: ImplicationVerdict }>;
  /** True when all per-solver verdicts agree. */
  allAgreed?: boolean;
}

export interface SynthesizeMeaningDiffOutput {
  fromRef: string;
  toRef: string;
  rows: ForensicRow[];
  summary: DiffInvariantSnapshotsOutput["summary"];
}

export interface MakeSynthesizeMeaningDiffStageDeps {
  producerVersion?: string;
}

export function makeSynthesizeMeaningDiffStage(
  deps: MakeSynthesizeMeaningDiffStageDeps = {},
): Stage<SynthesizeMeaningDiffInput, SynthesizeMeaningDiffOutput> {
  const producedBy = deps.producerVersion ?? "synthesizeMeaningDiff@v1";
  const checkImpl = makeCheckImplicationStage();

  return {
    name: "synthesizeMeaningDiff",
    producedBy,

    serializeInput(input) {
      // The diff's row identities are content-addressed; serializing the
      // diff CIDs keeps the cache key compact. The solver composition
      // affects the verdicts, so include it.
      const solverSig = input.solver
        ? [...input.solver.entries]
            .sort((a, b) => a.type.localeCompare(b.type))
            .map((e) => `${e.type}:${e.binary}:${e.flags.join(",")}:${e.timeoutMs}`)
            .join("|")
        : "default";
      return {
        fromRef: input.diff.fromRef,
        toRef: input.diff.toRef,
        rowKey: input.diff.rows.map((r) => rowCacheKey(r)).join("|"),
        solverSig,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as SynthesizeMeaningDiffOutput;
    },

    async run(input) {
      const solver: Solver = input.solver ?? {
        entries: [{
          type: "z3",
          binary: "z3",
          flags: ["-in", "-T:{{TIMEOUT_S}}"],
          timeoutMs: 5000,
        }],
      };
      const out: ForensicRow[] = [];
      for (const row of input.diff.rows) {
        out.push(await buildRow(row, checkImpl, solver));
      }
      const result = {
        fromRef: input.diff.fromRef,
        toRef: input.diff.toRef,
        rows: out,
        summary: input.diff.summary,
      };
      // Render to stdout. Suppressed when called from another workflow
      // (PROVEKIT_DIFF_QUIET=1) so the structured value can be consumed
      // without console clutter.
      if (process.env.PROVEKIT_DIFF_QUIET !== "1") {
        renderToStdout(result);
      }
      return result;
    },
  };
}

function rowCacheKey(row: DiffRow): string {
  switch (row.kind) {
    case "preserved": return `p:${row.id}`;
    case "added":     return `a:${row.id}`;
    case "removed":   return `r:${row.id}`;
    case "changed":   return `c:${row.id}`;
    case "renamed":   return `n:${row.fromId}->${row.toId}`;
  }
}

async function buildRow(
  row: DiffRow,
  checkImpl: ReturnType<typeof makeCheckImplicationStage>,
  solver: Solver,
): Promise<ForensicRow> {
  switch (row.kind) {
    case "preserved":
    case "added": {
      const inv = row.invariant;
      return {
        kind: row.kind,
        rowId: row.id,
        locus: locusOf(inv),
        fromId: row.kind === "preserved" ? row.id : null,
        toId: row.id,
        originatingBug: inv.originatingBug,
      };
    }
    case "removed": {
      const inv = row.invariant;
      return {
        kind: row.kind,
        rowId: row.id,
        locus: locusOf(inv),
        fromId: row.id,
        toId: null,
        originatingBug: inv.originatingBug,
      };
    }
    case "renamed":
    case "changed": {
      const fromInv = row.fromInvariant;
      const toInv = row.toInvariant;
      const fromId = row.kind === "renamed" ? row.fromId : row.id;
      const toId = row.kind === "renamed" ? row.toId : row.id;

      const probe = await checkImpl.run({
        oldSmt: fromInv.smt.assertion,
        newSmt: toInv.smt.assertion,
        solver,
      });

      // The framework's solver may have one or many entries; the
      // checkImpl Stage handles both uniformly. Per-entry transparency
      // surfaces only when N > 1 to keep the simple case quiet.
      const isComposite = solver.entries.length > 1;
      const perSolverVerdicts = isComposite
        ? probe.perEntry.map((p) => ({
            solverType: p.solverType,
            verdict: classifyEntry(p.newImpliesOld, p.oldImpliesNew),
          }))
        : undefined;

      return {
        kind: row.kind,
        rowId: row.kind === "renamed" ? `${fromId}→${toId}` : fromId,
        locus: locusOf(toInv),
        fromId,
        toId,
        originatingBug: toInv.originatingBug,
        directionVerdict: probe.verdict,
        implicationProbes: {
          newImpliesOld: probe.newImpliesOld,
          oldImpliesNew: probe.oldImpliesNew,
        },
        ...(isComposite ? { perSolverVerdicts, allAgreed: probe.allAgreed } : {}),
      };
    }
  }
}

function classifyEntry(
  newImpliesOld: "sat" | "unsat" | "unknown" | "timeout",
  oldImpliesNew: "sat" | "unsat" | "unknown" | "timeout",
): ImplicationVerdict {
  if (newImpliesOld === "unknown" || newImpliesOld === "timeout" ||
      oldImpliesNew === "unknown" || oldImpliesNew === "timeout") return "undecidable";
  if (newImpliesOld === "unsat" && oldImpliesNew === "unsat") return "equivalent";
  if (newImpliesOld === "unsat" && oldImpliesNew === "sat") return "strengthened";
  if (newImpliesOld === "sat" && oldImpliesNew === "unsat") return "weakened";
  return "incomparable";
}

function locusOf(inv: ForensicRow extends never ? never : { callsite: { filePath: string; function: string | null; startLine: number; endLine: number } }): ForensicRow["locus"] {
  return {
    filePath: inv.callsite.filePath,
    function: inv.callsite.function,
    startLine: inv.callsite.startLine,
    endLine: inv.callsite.endLine,
  };
}

function renderToStdout(result: SynthesizeMeaningDiffOutput): void {
  const w = process.stdout.write.bind(process.stdout);
  w(`provekit diff ${result.fromRef} → ${result.toRef}\n`);
  w("\n");
  const s = result.summary;
  const total = s.preserved + s.added + s.removed + s.renamed + s.changed;
  if (total === 0) {
    w("No invariants in either snapshot.\n");
    return;
  }
  w(`${total} invariants total: `);
  const parts: string[] = [];
  if (s.preserved) parts.push(`${s.preserved} preserved`);
  if (s.added) parts.push(`${s.added} added`);
  if (s.removed) parts.push(`${s.removed} removed`);
  if (s.renamed) parts.push(`${s.renamed} renamed`);
  if (s.changed) parts.push(`${s.changed} changed`);
  w(parts.join(", ") + "\n");
  w("\n");

  const interesting = result.rows.filter((r) => r.kind !== "preserved");
  if (interesting.length === 0) {
    w("All invariants preserved (no meaning-level changes).\n");
    return;
  }
  for (const row of interesting) {
    const marker = ROW_MARKER[row.kind];
    w(`${marker} ${row.kind.toUpperCase()}: ${row.locus.filePath}:${row.locus.startLine}`);
    if (row.locus.function) w(` (${row.locus.function})`);
    w("\n");
    w(`    ${row.originatingBug}\n`);
    if (row.fromId) w(`    fromId: ${row.fromId}\n`);
    if (row.toId) w(`    toId:   ${row.toId}\n`);
    if (row.directionVerdict) {
      w(`    direction: ${row.directionVerdict}`);
      if (row.implicationProbes) {
        w(` (newImpliesOld=${row.implicationProbes.newImpliesOld}, oldImpliesNew=${row.implicationProbes.oldImpliesNew})`);
      }
      w("\n");
    }
    w("\n");
  }
}

const ROW_MARKER: Record<DiffRow["kind"], string> = {
  preserved: " ",
  added: "+",
  removed: "-",
  renamed: "~",
  changed: "!",
};
