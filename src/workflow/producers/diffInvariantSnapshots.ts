/**
 * diffInvariantSnapshots Stage — set-difference between two snapshots,
 * with mechanical pairing and direction classification.
 *
 * Input: two ResolveInvariantSnapshotOutput values (from / to).
 * Output: structured rows that name what changed at the meaning layer.
 *
 * Five row categories:
 *
 *   preserved — id present in both snapshots; invariant unchanged
 *   added     — id only in `to` (newly authored or re-minted)
 *   removed   — id only in `from` (retired or replaced)
 *   renamed   — best-effort pairing across an addition + removal where
 *               the human-readable originatingBug text matches; the
 *               LLM treats this as a re-mint of the same intent
 *   changed   — same id present in both BUT the file content differs
 *               (the StoredInvariant hash collision is rare; this row
 *               is a defensive catch)
 *
 * For "renamed" rows, the Stage emits the from/to id pair and the from/to
 * SMT bodies — downstream the diff workflow runs check-implication on
 * those to characterize direction (strengthened / weakened / etc).
 *
 * Pure. No LLM. No Z3 (the implication probe is a downstream Stage; this
 * one only does set arithmetic + name pairing).
 */

import type { Stage } from "../types.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import type { ResolveInvariantSnapshotOutput } from "./resolveInvariantSnapshot.js";

export const DIFF_INVARIANT_SNAPSHOTS_CAPABILITY = "diff-invariant-snapshots";

export interface DiffInvariantSnapshotsInput {
  from: ResolveInvariantSnapshotOutput;
  to: ResolveInvariantSnapshotOutput;
}

export interface PreservedRow {
  kind: "preserved";
  id: string;
  invariant: StoredInvariant;
}

export interface AddedRow {
  kind: "added";
  id: string;
  invariant: StoredInvariant;
}

export interface RemovedRow {
  kind: "removed";
  id: string;
  invariant: StoredInvariant;
}

export interface RenamedRow {
  kind: "renamed";
  fromId: string;
  toId: string;
  fromInvariant: StoredInvariant;
  toInvariant: StoredInvariant;
}

export interface ChangedRow {
  kind: "changed";
  id: string;
  fromInvariant: StoredInvariant;
  toInvariant: StoredInvariant;
}

export type DiffRow = PreservedRow | AddedRow | RemovedRow | RenamedRow | ChangedRow;

export interface DiffInvariantSnapshotsOutput {
  fromRef: string;
  toRef: string;
  rows: DiffRow[];
  summary: {
    preserved: number;
    added: number;
    removed: number;
    renamed: number;
    changed: number;
  };
}

export interface MakeDiffInvariantSnapshotsStageDeps {
  producerVersion?: string;
}

export function makeDiffInvariantSnapshotsStage(
  deps: MakeDiffInvariantSnapshotsStageDeps = {},
): Stage<DiffInvariantSnapshotsInput, DiffInvariantSnapshotsOutput> {
  const producedBy = deps.producerVersion ?? "diffInvariantSnapshots@v1";

  return {
    name: "diffInvariantSnapshots",
    producedBy,

    serializeInput(input) {
      return {
        fromRef: input.from.ref,
        fromIds: input.from.entries.map((e) => e.id).sort(),
        toRef: input.to.ref,
        toIds: input.to.entries.map((e) => e.id).sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as DiffInvariantSnapshotsOutput;
    },

    async run(input) {
      const fromIds = new Set(Object.keys(input.from.byId));
      const toIds = new Set(Object.keys(input.to.byId));
      const both = new Set([...fromIds].filter((id) => toIds.has(id)));
      const onlyFrom = [...fromIds].filter((id) => !toIds.has(id));
      const onlyTo = [...toIds].filter((id) => !fromIds.has(id));

      const rows: DiffRow[] = [];

      // Same-id rows: preserved or (rare) changed.
      for (const id of both) {
        const fromInv = input.from.byId[id]!;
        const toInv = input.to.byId[id]!;
        const sameContent = JSON.stringify(fromInv) === JSON.stringify(toInv);
        if (sameContent) {
          rows.push({ kind: "preserved", id, invariant: toInv });
        } else {
          rows.push({ kind: "changed", id, fromInvariant: fromInv, toInvariant: toInv });
        }
      }

      // Pair onlyFrom + onlyTo by originatingBug text. Pairs become
      // "renamed"; unpaired residue stays as added/removed.
      const removedById = new Map<string, StoredInvariant>();
      for (const id of onlyFrom) removedById.set(id, input.from.byId[id]!);

      const addedRows: AddedRow[] = [];
      const removedRows: RemovedRow[] = [];

      for (const toId of onlyTo) {
        const toInv = input.to.byId[toId]!;
        const matchKey = toInv.originatingBug.trim();
        let matchedFromId: string | null = null;
        for (const [fromId, fromInv] of removedById) {
          if (fromInv.originatingBug.trim() === matchKey) {
            matchedFromId = fromId;
            break;
          }
        }
        if (matchedFromId) {
          rows.push({
            kind: "renamed",
            fromId: matchedFromId,
            toId,
            fromInvariant: removedById.get(matchedFromId)!,
            toInvariant: toInv,
          });
          removedById.delete(matchedFromId);
        } else {
          addedRows.push({ kind: "added", id: toId, invariant: toInv });
        }
      }
      for (const [id, invariant] of removedById) {
        removedRows.push({ kind: "removed", id, invariant });
      }
      rows.push(...addedRows, ...removedRows);

      const summary = {
        preserved: rows.filter((r) => r.kind === "preserved").length,
        added: rows.filter((r) => r.kind === "added").length,
        removed: rows.filter((r) => r.kind === "removed").length,
        renamed: rows.filter((r) => r.kind === "renamed").length,
        changed: rows.filter((r) => r.kind === "changed").length,
      };

      return { fromRef: input.from.ref, toRef: input.to.ref, rows, summary };
    },
  };
}
