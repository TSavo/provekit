/**
 * Diff-catalogs stage — migrate workflow's contract-surface diff (M2).
 *
 * Pure data transformation. Given two LoadCatalogResults, compute the
 * propertyHashes that were Added (in new but not old), Removed (in old
 * but not new), and Modified (same name appears in both, but the
 * propertyHash changed — the kit author renamed or weakened the
 * contract under the same symbol).
 *
 * "Same name" rules: a Modified declaration is one whose `name` is
 * non-null AND appears in both old and new declaration sets, but with a
 * different propertyHash. When `name` is null on either side, that
 * declaration cannot participate in the Modified relation — it's
 * either purely Added (in new) or purely Removed (in old). This is
 * conservative: the contract spec calls Modified out as a distinct
 * relation only when the kit author has a stable identifier across
 * versions; without one, the diff falls back to add/remove.
 *
 * Output is content-addressable per the task spec: same two catalog
 * proofHashes always yield the same diff.
 */

import type { Stage } from "../types.js";
import type {
  CatalogDeclaration,
  LoadCatalogResult,
} from "./loadCatalog.js";

export const DIFF_CATALOGS_CAPABILITY = "diff-catalogs";

export interface DiffCatalogsStageInput {
  oldCatalog: LoadCatalogResult;
  newCatalog: LoadCatalogResult;
}

export interface ModifiedDeclaration {
  name: string;
  oldPropertyHash: string;
  newPropertyHash: string;
}

export interface DiffCatalogsResult {
  oldFound: boolean;
  newFound: boolean;
  oldProofHash: string;
  newProofHash: string;
  /** Declarations present in new but not in old (by propertyHash). */
  added: CatalogDeclaration[];
  /** Declarations present in old but not in new (by propertyHash). */
  removed: CatalogDeclaration[];
  /**
   * Declarations whose `name` matches across versions but whose
   * propertyHash differs. Empty when one side has no named
   * declarations or when names are all unique to a single side.
   */
  modified: ModifiedDeclaration[];
  /**
   * Convenience flag: true when nothing changed (added + removed +
   * modified all empty). False otherwise.
   */
  identical: boolean;
}

export interface MakeDiffCatalogsStageDeps {
  producerVersion?: string;
}

export function makeDiffCatalogsStage(
  deps: MakeDiffCatalogsStageDeps = {},
): Stage<DiffCatalogsStageInput, DiffCatalogsResult> {
  const producedBy = deps.producerVersion ?? "diff-catalogs@v1";

  return {
    name: "diff-catalogs",
    producedBy,

    serializeInput(input) {
      return {
        old: catalogFingerprint(input.oldCatalog),
        new: catalogFingerprint(input.newCatalog),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as DiffCatalogsResult;
    },

    async run(input) {
      const oldDecls = input.oldCatalog.found
        ? input.oldCatalog.declarations
        : [];
      const newDecls = input.newCatalog.found
        ? input.newCatalog.declarations
        : [];

      const oldByHash = new Map(oldDecls.map((d) => [d.propertyHash, d]));
      const newByHash = new Map(newDecls.map((d) => [d.propertyHash, d]));

      const added: CatalogDeclaration[] = [];
      const removed: CatalogDeclaration[] = [];
      for (const d of newDecls) {
        if (!oldByHash.has(d.propertyHash)) added.push(d);
      }
      for (const d of oldDecls) {
        if (!newByHash.has(d.propertyHash)) removed.push(d);
      }

      // Modified: same name on both sides, different propertyHash.
      // Only consider declarations whose name survived to the candidate
      // pool (i.e. wasn't already paired by exact propertyHash match).
      const oldByName = new Map<string, CatalogDeclaration>();
      const newByName = new Map<string, CatalogDeclaration>();
      for (const d of removed) if (d.name) oldByName.set(d.name, d);
      for (const d of added) if (d.name) newByName.set(d.name, d);

      const modified: ModifiedDeclaration[] = [];
      for (const [name, oldDecl] of oldByName) {
        const newDecl = newByName.get(name);
        if (!newDecl) continue;
        modified.push({
          name,
          oldPropertyHash: oldDecl.propertyHash,
          newPropertyHash: newDecl.propertyHash,
        });
      }

      // Strip Modified pairs out of Added/Removed so they're reported
      // exactly once. Without this, a renamed-but-otherwise-equivalent
      // declaration would show up in all three lists.
      const modifiedNames = new Set(modified.map((m) => m.name));
      const finalAdded = added.filter(
        (d) => !d.name || !modifiedNames.has(d.name),
      );
      const finalRemoved = removed.filter(
        (d) => !d.name || !modifiedNames.has(d.name),
      );

      finalAdded.sort((a, b) => a.propertyHash.localeCompare(b.propertyHash));
      finalRemoved.sort((a, b) => a.propertyHash.localeCompare(b.propertyHash));
      modified.sort((a, b) => a.name.localeCompare(b.name));

      return {
        oldFound: input.oldCatalog.found,
        newFound: input.newCatalog.found,
        oldProofHash: input.oldCatalog.proofHash,
        newProofHash: input.newCatalog.proofHash,
        added: finalAdded,
        removed: finalRemoved,
        modified,
        identical:
          finalAdded.length === 0 &&
          finalRemoved.length === 0 &&
          modified.length === 0,
      };
    },
  };
}

/**
 * Hashable signature of a catalog. Includes only the declaration set's
 * propertyHashes + names — those are the only inputs to the diff. The
 * catalog's own bindingHash and producedBy don't enter the diff and so
 * don't enter the cache key.
 */
function catalogFingerprint(catalog: LoadCatalogResult): unknown {
  if (!catalog.found) {
    return { found: false, proofHash: catalog.proofHash };
  }
  const declarations = [...catalog.declarations]
    .map((d) => ({ propertyHash: d.propertyHash, name: d.name }))
    .sort((a, b) => a.propertyHash.localeCompare(b.propertyHash));
  return { found: true, proofHash: catalog.proofHash, declarations };
}
