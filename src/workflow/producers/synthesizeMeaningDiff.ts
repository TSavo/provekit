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
} from "./checkImplication.js";

export const SYNTHESIZE_MEANING_DIFF_CAPABILITY = "synthesize-meaning-diff";

export interface SynthesizeMeaningDiffInput {
  diff: DiffInvariantSnapshotsOutput;
  /** Which SMT solver runs the implication probes. Default: "z3". */
  solver?: "z3" | "cvc5";
  /** Per-probe timeout in ms. Default: 5000. */
  timeoutMs?: number;
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
  /** Underlying Z3 verdicts for transparency. */
  implicationProbes?: {
    newImpliesOld: CheckImplicationOutput["newImpliesOld"];
    oldImpliesNew: CheckImplicationOutput["oldImpliesNew"];
  };
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
      // diff CIDs keeps the cache key compact.
      return {
        fromRef: input.diff.fromRef,
        toRef: input.diff.toRef,
        rowKey: input.diff.rows.map((r) => rowCacheKey(r)).join("|"),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as SynthesizeMeaningDiffOutput;
    },

    async run(input) {
      const out: ForensicRow[] = [];
      for (const row of input.diff.rows) {
        out.push(await buildRow(row, checkImpl, input.solver, input.timeoutMs));
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
  solver: "z3" | "cvc5" | undefined,
  timeoutMs: number | undefined,
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
      const fromInv = row.kind === "renamed" ? row.fromInvariant : row.fromInvariant;
      const toInv = row.kind === "renamed" ? row.toInvariant : row.toInvariant;
      const fromId = row.kind === "renamed" ? row.fromId : row.id;
      const toId = row.kind === "renamed" ? row.toId : row.id;
      const probe = await checkImpl.run({
        oldSmt: fromInv.smt.assertion,
        newSmt: toInv.smt.assertion,
        solver,
        timeoutMs,
      });
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
      };
    }
  }
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
