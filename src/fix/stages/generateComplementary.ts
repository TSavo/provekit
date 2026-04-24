/**
 * C4: Complementary-change generator.
 *
 * Discovers adjacent sites via SAST queries, proposes patches via LLM, and
 * verifies each via oracle #3 (principle re-evaluation under the overlay).
 *
 * Contract:
 * - Every returned ComplementaryChange has verifiedAgainstOverlay: true.
 * - Rejected patches are rolled back (file restore + re-index) before the next site.
 * - Overlay state is cumulative: each accepted patch is visible to later verifications.
 * - Main db is READ-ONLY. Writes go to overlay.sastDb.
 * - "unknown" Z3 verdict is failure (spec decision #6).
 *
 * See complementary.ts for helpers: discoverComplementarySites,
 * proposeChangeForSite, verifySiteChange.
 */

import type {
  FixCandidate,
  BugLocus,
  OverlayHandle,
  ComplementaryChange,
  LLMProvider,
  InvariantClaim,
} from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import type { Db } from "../../db/index.js";
import {
  discoverComplementarySites,
  proposeChangeForSite,
  proposeChangeForSiteViaAgent,
  verifySiteChange,
  priorityOf,
} from "../complementary.js";

export async function generateComplementary(args: {
  fix: FixCandidate;
  locus: BugLocus;
  overlay: OverlayHandle;
  db: Db;
  llm: LLMProvider;
  maxSites: number;
  invariant?: InvariantClaim;
  logger?: FixLoopLogger;
}): Promise<ComplementaryChange[]> {
  const { fix, locus, overlay, db, llm, maxSites, invariant } = args;

  // 1. Discover candidate sites via SAST queries (read-only from main db).
  const sites = await discoverComplementarySites({
    fix,
    locus,
    db,
    maxSites,
    invariant,
  });

  // 2. Propose + verify each site sequentially (cumulative overlay state).
  const accepted: ComplementaryChange[] = [];

  for (const site of sites) {
    // 2a. LLM proposes a patch for this site (agent path or JSON path).
    let proposed;
    try {
      if (llm.agent) {
        // Agent path: Claude edits files directly; we capture via git diff.
        proposed = await proposeChangeForSiteViaAgent(site, fix, locus, invariant, llm, overlay);
      } else {
        // JSON path: LLM returns a JSON candidates array.
        proposed = await proposeChangeForSite(site, fix, locus, invariant, llm);
      }
    } catch {
      // LLM error — skip this site.
      continue;
    }
    if (!proposed) continue; // LLM declined.

    // 2b. Oracle #3: verify the proposed patch under the cumulative overlay.
    const verified = await verifySiteChange(proposed, site, overlay, invariant);

    if (verified.verifiedAgainstOverlay) {
      accepted.push({
        kind: site.kind,
        targetNodeId: site.nodeId,
        patch: proposed.patch,
        rationale: proposed.rationale,
        verifiedAgainstOverlay: true,
        overlayZ3Verdict: verified.verdict,
        priority: priorityOf(site.kind),
        audit: {
          siteKind: site.kind,
          discoveredVia: site.discoveredVia,
          z3RunMs: verified.z3RunMs,
        },
      });
    }
    // On rejection, verifySiteChange already rolled back the overlay state.
  }

  // 3. Sort by priority ascending (callers first, observability last).
  return accepted.sort((a, b) => a.priority - b.priority);
}
