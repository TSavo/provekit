/**
 * D3: Learning layer — library updates post-apply.
 *
 * After D2 successfully applies a bundle, D3 promotes its learnings into the
 * principle library so the system is strictly stronger afterward.
 *
 * MVP simplifications documented inline:
 * - Confidence-update loop (false-negative detection) is deferred. The spec
 *   notes that full match-set comparison belongs to a separate `neurallog review`
 *   CLI command. Here we only set confidenceUpdates = [].
 * - Capability registration logging is minimal: we record the name so callers
 *   can verify it, but no additional registry mutation is needed — D2 already
 *   committed the capability files to disk.
 */

import { mkdirSync, writeFileSync } from "fs";
import { join, dirname, relative, sep } from "path";
import type { FixBundle, ApplyResult } from "./types.js";
import type { Db } from "../db/index.js";
import { principlesLibrary } from "../db/schema/principlesLibrary.js";
import { enqueuePendingFix } from "./bundlePersistence.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface LearnResult {
  principleAdded: string | null;
  principleFilesWritten: { dslPath: string; jsonPath: string } | null;
  /** substrate bundles only — the capabilityName that was registered */
  capabilityRegistered: string | null;
  /** MVP: always empty — deferred to `neurallog review` CLI */
  confidenceUpdates: {
    principleName: string;
    prevTier: string;
    newTier: string;
    reason: string;
  }[];
  pendingFixesEnqueued: number;
  auditLogged: boolean;
}

// ---------------------------------------------------------------------------
// Name safety guard
// ---------------------------------------------------------------------------

/**
 * Reject principle names that could escape the `.neurallog/principles/`
 * directory via path traversal.
 */
function assertSafePrincipleName(name: string): void {
  if (
    name.includes("/") ||
    name.includes("\\") ||
    name.includes("..") ||
    name.split(sep).some((part) => part === "..")
  ) {
    throw new Error(
      `Principle name is unsafe for use as a filename: ${JSON.stringify(name)}`,
    );
  }
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export async function learnFromBundle(args: {
  bundle: FixBundle;
  applyResult: ApplyResult;
  db: Db;
  /** Root of the repo being analysed. Principle files go to <projectRoot>/.neurallog/principles/. Defaults to process.cwd(). */
  projectRoot?: string;
}): Promise<LearnResult> {
  // Fast-path: nothing to do when apply did not succeed.
  if (!args.applyResult.applied) {
    return {
      principleAdded: null,
      principleFilesWritten: null,
      capabilityRegistered: null,
      confidenceUpdates: [],
      pendingFixesEnqueued: 0,
      auditLogged: false,
    };
  }

  const projectRoot = args.projectRoot ?? process.cwd();

  const result: LearnResult = {
    principleAdded: null,
    principleFilesWritten: null,
    capabilityRegistered: null,
    confidenceUpdates: [],
    pendingFixesEnqueued: 0,
    auditLogged: false,
  };

  // -------------------------------------------------------------------------
  // 1. Principle library update
  // -------------------------------------------------------------------------
  const principle = args.bundle.artifacts.principle;

  if (principle !== null) {
    assertSafePrincipleName(principle.name);

    const principlesDir = join(projectRoot, ".neurallog", "principles");
    mkdirSync(principlesDir, { recursive: true });

    const dslPath = join(principlesDir, `${principle.name}.dsl`);
    const jsonPath = join(principlesDir, `${principle.name}.json`);

    writeFileSync(dslPath, principle.dslSource, "utf8");
    writeFileSync(
      jsonPath,
      JSON.stringify(
        {
          id: principle.name,
          name: principle.name,
          description: principle.teachingExample.explanation,
          smtTemplate: principle.smtTemplate,
          teachingExample: principle.teachingExample,
          validated: true,
          confidence: "advisory",
        },
        null,
        2,
      ),
      "utf8",
    );

    const relDsl = relative(projectRoot, dslPath);
    const relJson = relative(projectRoot, jsonPath);

    args.db
      .insert(principlesLibrary)
      .values({
        name: principle.name,
        dslPath: relDsl,
        jsonPath: relJson,
        confidenceTier: "advisory",
        addedBundleId: args.bundle.bundleId,
        addedAt: Date.now(),
        falseNegativeCount: 0,
        successfulApplicationCount: 0,
      })
      .run();

    result.principleAdded = principle.name;
    result.principleFilesWritten = { dslPath, jsonPath };
  }

  // -------------------------------------------------------------------------
  // 2. Capability registration (substrate bundles)
  //
  // Capability files are already on disk thanks to D2. D3's job is to note
  // that the capability is now available so downstream tooling can pick it up
  // on the next process start via the schema/index re-exports written by D2.
  // -------------------------------------------------------------------------
  if (
    args.bundle.bundleType === "substrate" &&
    args.bundle.artifacts.capabilitySpec !== null
  ) {
    result.capabilityRegistered =
      args.bundle.artifacts.capabilitySpec.capabilityName;
  }

  // -------------------------------------------------------------------------
  // 3. Confidence updates — deferred (MVP)
  //
  // Full match-set comparison (which existing principles should have caught
  // this bug but didn't) belongs to a separate `neurallog review` CLI command.
  // We log the intent but do not compute anything here.
  // -------------------------------------------------------------------------
  result.confidenceUpdates = [];

  // -------------------------------------------------------------------------
  // 4. Enqueue latent sites as pending fixes
  // -------------------------------------------------------------------------
  const latentSites = principle?.latentSiteMatches ?? [];
  for (const site of latentSites) {
    enqueuePendingFix(args.db, {
      sourceBundleId: args.bundle.bundleId,
      siteNodeId: site.nodeId,
      siteFile: site.file,
      siteLine: site.line,
      reason: `latent site of principle '${principle!.name}' — not included in this bundle`,
      priority: 5,
    });
    result.pendingFixesEnqueued++;
  }

  // -------------------------------------------------------------------------
  // 5. Audit record
  //
  // LLM calls and the bundle's own audit trail are already persisted by D1/D2.
  // Mark auditLogged = true to signal that we've completed our work.
  // -------------------------------------------------------------------------
  result.auditLogged = true;

  return result;
}
