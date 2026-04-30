/**
 * Load-catalog stage — migrate workflow's kit-catalog retrieval (M1).
 *
 * Looks up a kit-catalog memento by proofHash from the local memento
 * store and reads its bridge children's propertyHashes. The catalog
 * is a memento whose `cid` IS the kit's proofHash (per
 * docs/specs/2026-04-29-correctness-is-a-hash.md §"Three-coordinate
 * artifact identity") and whose `inputCids` list the bridge mementos
 * the kit publishes.
 *
 * Spec gap: this Stage assumes the consumer's local proofkit has
 * already imported the published kit's mementos into its DB (the
 * "leaves and roots" boundary the substrate spec calls out — auditors
 * walk; the framework lists). When migrate is invoked against a
 * proofHash whose memento is not in the local DB,
 * load-catalog returns `{ found: false }`. The Stage explicitly does
 * NOT fetch from a registry; that's an auditor's job per the
 * framework's discipline.
 *
 * Pure: no LLM, no fs writes. Reads the memento store via findByCid +
 * one direct lookup per inputCid. The DB connection is a runtime
 * dependency, not part of the Stage input — same pattern as
 * recognize / locate.
 */

import type { Stage } from "../types.js";
import type { Db } from "../../db/index.js";
import {
  findByCid,
  type Memento,
} from "../../fix/runtime/mementoStore.js";

export const LOAD_CATALOG_CAPABILITY = "load-catalog";

/**
 * Per-bridge declaration — one row per child memento the kit catalog
 * references via inputCids. Captures the propertyHash so the diff
 * stage can compute Added/Removed/Modified. The optional `name` is
 * pulled from the bridge's evidence body when available (bridge
 * variant carries `sourceSymbol`); when not available, callers fall
 * back to propertyHash for naming.
 */
export interface CatalogDeclaration {
  /** The bridge memento's CID — same value that lives in inputCids. */
  cid: string;
  /** The propertyHash claimed by the bridge memento. */
  propertyHash: string;
  /** The bindingHash claimed by the bridge memento. */
  bindingHash: string;
  /** Producer identity that signed the bridge memento. */
  producedBy: string;
  /**
   * Symbolic name when available (e.g. "global.parseInt") — pulled
   * from a bridge-variant evidence body. Null when the bridge isn't a
   * canonical kit-bridge variant (e.g. legacy-witness wrappers).
   */
  name: string | null;
}

export type LoadCatalogResult =
  | {
      found: false;
      proofHash: string;
    }
  | {
      found: true;
      proofHash: string;
      /** The kit catalog's own producer identity (from the catalog memento). */
      producedBy: string;
      /** Per-bridge declarations, sorted by propertyHash for stability. */
      declarations: CatalogDeclaration[];
    };

export interface LoadCatalogStageInput {
  /** The kit's proofHash — the CID of the catalog memento. */
  proofHash: string;
}

export interface MakeLoadCatalogStageDeps {
  db: Db;
  producerVersion?: string;
}

export function makeLoadCatalogStage(
  deps: MakeLoadCatalogStageDeps,
): Stage<LoadCatalogStageInput, LoadCatalogResult> {
  const producedBy = deps.producerVersion ?? "load-catalog@v1";

  return {
    name: "load-catalog",
    producedBy,

    serializeInput(input) {
      return { proofHash: input.proofHash };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LoadCatalogResult;
    },

    async run(input) {
      const catalog = findByCid(deps.db, input.proofHash);
      if (!catalog) {
        return { found: false, proofHash: input.proofHash };
      }
      const declarations: CatalogDeclaration[] = [];
      for (const bridgeCid of catalog.inputCids ?? []) {
        const bridge = findByCid(deps.db, bridgeCid);
        if (!bridge) continue;
        declarations.push({
          cid: bridgeCid,
          propertyHash: bridge.propertyHash,
          bindingHash: bridge.bindingHash,
          producedBy: bridge.producedBy,
          name: extractName(bridge),
        });
      }
      declarations.sort((a, b) => a.propertyHash.localeCompare(b.propertyHash));
      return {
        found: true,
        proofHash: input.proofHash,
        producedBy: catalog.producedBy,
        declarations,
      };
    },
  };
}

function extractName(memento: Memento): string | null {
  const evidence = memento.evidence;
  if (!evidence) return null;
  if (evidence.kind === "bridge") {
    const body = evidence.body as { sourceSymbol?: unknown };
    return typeof body.sourceSymbol === "string" ? body.sourceSymbol : null;
  }
  return null;
}
