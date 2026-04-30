/**
 * Mint-migration-plan action — migrate workflow's terminal write (M4).
 *
 * Side-effecting: writes a markdown migration plan to a project-local
 * path. Action — not Stage — because the plan file lands on disk and
 * exists to be read by humans. The audit memento records the path and
 * the plan's key counts (added/removed/modified) so a forensic walk
 * can reach the file without parsing it.
 *
 * The plan format:
 *   - Header with old/new proofHashes, scan timestamp.
 *   - Three counted sections: Added, Removed, Modified.
 *   - "Impacted callsites" section listing each invariant the user
 *     project depends on (per find-impacted-callsites' v1 collision
 *     heuristic — see that stage's spec-gap doc).
 *   - "Method" footnote: documents that v1 matches by propertyHash
 *     collision, not by composition (inputCids).
 *
 * Output path: `<projectRoot>/.provekit/migrations/<oldProofHash>-to-<newProofHash>.md`.
 * Idempotent: rewriting produces an identical file.
 */

import { mkdirSync, writeFileSync } from "fs";
import { join } from "path";
import type { Action } from "../types.js";
import type { DiffCatalogsResult } from "./diffCatalogs.js";
import type { FindImpactedCallsitesResult } from "./findImpactedCallsites.js";

export const MINT_MIGRATION_PLAN_CAPABILITY = "mint-migration-plan";

export interface MintMigrationPlanActionInput {
  projectRoot: string;
  diff: DiffCatalogsResult;
  impacts: FindImpactedCallsitesResult;
}

export interface MigrationPlanResource {
  /** Absolute path to the written markdown file. */
  planPath: string;
  /**
   * Inline counts captured at write time so callers can branch on
   * outcome without re-reading the markdown.
   */
  counts: {
    added: number;
    removed: number;
    modified: number;
    impactedCallsites: number;
  };
}

export interface MakeMintMigrationPlanActionDeps {
  producerVersion?: string;
}

export function makeMintMigrationPlanAction(
  deps: MakeMintMigrationPlanActionDeps = {},
): Action<MintMigrationPlanActionInput, MigrationPlanResource> {
  const producedBy = deps.producerVersion ?? "mint-migration-plan@v1";

  return {
    name: "mint-migration-plan",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        oldProofHash: input.diff.oldProofHash,
        newProofHash: input.diff.newProofHash,
        counts: {
          added: input.diff.added.length,
          removed: input.diff.removed.length,
          modified: input.diff.modified.length,
          impacted: input.impacts.impacted.length,
        },
      };
    },

    describeResource(resource) {
      return `migration plan at ${resource.planPath} (added=${resource.counts.added} removed=${resource.counts.removed} modified=${resource.counts.modified} impacted=${resource.counts.impactedCallsites})`;
    },

    async run(input) {
      const dir = join(input.projectRoot, ".provekit", "migrations");
      mkdirSync(dir, { recursive: true });
      const filename = `${input.diff.oldProofHash}-to-${input.diff.newProofHash}.md`;
      const planPath = join(dir, filename);
      const markdown = renderPlan(input.diff, input.impacts);
      writeFileSync(planPath, markdown, "utf-8");
      return {
        planPath,
        counts: {
          added: input.diff.added.length,
          removed: input.diff.removed.length,
          modified: input.diff.modified.length,
          impactedCallsites: input.impacts.impacted.length,
        },
      };
    },
  };
}

function renderPlan(
  diff: DiffCatalogsResult,
  impacts: FindImpactedCallsitesResult,
): string {
  const lines: string[] = [];
  lines.push(`# Migration plan: ${diff.oldProofHash} → ${diff.newProofHash}`);
  lines.push("");
  if (!diff.oldFound || !diff.newFound) {
    lines.push("> Note: at least one catalog memento was not found in the local store.");
    lines.push(`> oldFound=${diff.oldFound}, newFound=${diff.newFound}`);
    lines.push("");
  }
  if (diff.identical) {
    lines.push("No contract changes between these two proofHashes. Migration is a no-op.");
    lines.push("");
    return lines.join("\n");
  }

  lines.push(`## Summary`);
  lines.push("");
  lines.push(`- Added: ${diff.added.length}`);
  lines.push(`- Removed: ${diff.removed.length}`);
  lines.push(`- Modified: ${diff.modified.length}`);
  lines.push(`- Impacted callsites in this project: ${impacts.impacted.length}`);
  lines.push("");

  if (diff.added.length > 0) {
    lines.push(`## Added (${diff.added.length})`);
    lines.push("");
    for (const d of diff.added) {
      lines.push(`- \`${d.propertyHash}\`${d.name ? ` (${d.name})` : ""}`);
    }
    lines.push("");
  }

  if (diff.removed.length > 0) {
    lines.push(`## Removed (${diff.removed.length})`);
    lines.push("");
    for (const d of diff.removed) {
      lines.push(`- \`${d.propertyHash}\`${d.name ? ` (${d.name})` : ""}`);
    }
    lines.push("");
  }

  if (diff.modified.length > 0) {
    lines.push(`## Modified (${diff.modified.length})`);
    lines.push("");
    for (const m of diff.modified) {
      lines.push(`- \`${m.name}\`: \`${m.oldPropertyHash}\` → \`${m.newPropertyHash}\``);
    }
    lines.push("");
  }

  if (impacts.impacted.length > 0) {
    lines.push(`## Impacted callsites (${impacts.impacted.length})`);
    lines.push("");
    lines.push(`These project invariants reference a removed or modified propertyHash. Each must be revisited before the upgrade lands.`);
    lines.push("");
    for (const c of impacts.impacted) {
      const loc = `${c.callsite.filePath}:${c.callsite.startLine}`;
      const hint =
        c.reason === "removed"
          ? "removed upstream"
          : `modified upstream → \`${c.newPropertyHash}\``;
      lines.push(`- \`${c.invariantId}\` (${c.reason}): ${loc} — ${hint}`);
      lines.push(`  - Originating bug: ${c.originatingBug}`);
    }
    lines.push("");
  }

  lines.push(`## Method`);
  lines.push("");
  lines.push(`Catalog diff: produced by the \`migrate\` workflow's diff-catalogs stage. Two declarations are considered the same when their propertyHashes are equal; "modified" pairs match by symbolic name across versions.`);
  lines.push("");
  lines.push(`Callsite impact match strategy: \`${impacts.matchStrategy}\`. v1 reports a project invariant as impacted when its own propertyHash appears in the Removed or Modified set. Composition-walking via inputCids is a follow-up.`);
  lines.push("");
  return lines.join("\n");
}
