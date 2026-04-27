/**
 * The artifact produced by C4 (generateComplementary).
 *
 * C4 reads the FixCandidate and the InvariantReport, then searches for
 * other code sites in the project that match the same bug-class shape
 * and would need the same fix. The artifact records what was found and
 * the search semantics used (so downstream and audits can see whether
 * the search was narrow or broad).
 */

import type { CodePatch } from "../types.js";

export interface ComplementarySite {
  /** Source location of the matching site. */
  readonly file: string;
  readonly line: number;
  readonly function?: string;
  /** The patch C4 proposes for this site. */
  readonly patch: CodePatch;
  /** Why this site matches the bug class. */
  readonly rationale: string;
}

export interface ComplementaryReport {
  /** Sites C4 found that share the bug-class shape. May be empty. */
  readonly sites: ReadonlyArray<ComplementarySite>;

  /**
   * What invariant clause C4 used to define "matching shape." Cited
   * downstream so reviewers can see whether the search generalized
   * appropriately or stayed too local.
   */
  readonly searchPredicate: string;

  /** Confidence each site is actually a TP (not a false structural match). */
  readonly perSiteConfidence: ReadonlyArray<"high" | "medium" | "low">;
}
