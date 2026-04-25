/**
 * D3: Learning layer — library updates post-apply.
 *
 * After D2 successfully applies a bundle, D3 promotes its learnings into the
 * principle library so the system is strictly stronger afterward.
 *
 * MVP simplifications documented inline:
 * - Confidence-update loop (false-negative detection) is deferred. The spec
 *   notes that full match-set comparison belongs to a separate `provekit review`
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
  /**
   * Pitch-leak 3 layer 1: alternative AST shapes for the same bug class that
   * were also written to .provekit/principles/. Empty when C6 returned only
   * the canonical shape (or returned no principles at all).
   */
  alternateShapesWritten: {
    name: string;
    bugClassId: string;
    dslPath: string;
    jsonPath: string;
  }[];
  /** substrate bundles only — the capabilityName that was registered */
  capabilityRegistered: string | null;
  /** MVP: always empty — deferred to `provekit review` CLI */
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
 * Reject principle names that could escape the `.provekit/principles/`
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
  /** Root of the repo being analysed. Principle files go to <projectRoot>/.provekit/principles/. Defaults to process.cwd(). */
  projectRoot?: string;
}): Promise<LearnResult> {
  // Fast-path: nothing to do when apply did not succeed.
  if (!args.applyResult.applied) {
    return {
      principleAdded: null,
      principleFilesWritten: null,
      alternateShapesWritten: [],
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
    alternateShapesWritten: [],
    capabilityRegistered: null,
    confidenceUpdates: [],
    pendingFixesEnqueued: 0,
    auditLogged: false,
  };

  // -------------------------------------------------------------------------
  // 1. Principle library update
  //
  // Pitch-leak 3 layer 1: a single C6 call may produce 1-3 alternative shapes
  // for the same bug class. We persist ALL of them — the canonical principle
  // and each alternate shape — to .provekit/principles/. They share the same
  // `bug_class_id` in their JSON metadata, allowing downstream tooling to
  // group shapes when reasoning about bug-class coverage.
  //
  // `result.principleAdded` and `result.principleFilesWritten` reflect the
  // canonical (primary) shape only, for backward compatibility with callers
  // that read those fields. Alternate shapes are reported via
  // `result.alternateShapesWritten`.
  // -------------------------------------------------------------------------
  const principle = args.bundle.artifacts.principle;
  const alternateShapes = args.bundle.artifacts.alternateShapes ?? [];
  const allShapes = principle !== null ? [principle, ...alternateShapes] : [];

  if (allShapes.length > 0) {
    const principlesDir = join(projectRoot, ".provekit", "principles");
    mkdirSync(principlesDir, { recursive: true });

    for (let i = 0; i < allShapes.length; i++) {
      const shape = allShapes[i];
      const isPrimary = i === 0;
      assertSafePrincipleName(shape.name);

      const dslPath = join(principlesDir, `${shape.name}.dsl`);
      const jsonPath = join(principlesDir, `${shape.name}.json`);

      writeFileSync(dslPath, shape.dslSource, "utf8");
      writeFileSync(
        jsonPath,
        JSON.stringify(
          {
            id: shape.name,
            name: shape.name,
            // Pitch-leak 3 layer 1: bug_class_id is shared across all shapes
            // of the same bug class. Canonical and alternates have distinct
            // `id`/`name` but identical `bug_class_id`.
            bug_class_id: shape.bugClassId,
            description: shape.teachingExample.explanation,
            smtTemplate: shape.smtTemplate,
            teachingExample: shape.teachingExample,
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
          name: shape.name,
          dslPath: relDsl,
          jsonPath: relJson,
          confidenceTier: "advisory",
          addedBundleId: args.bundle.bundleId,
          addedAt: Date.now(),
          falseNegativeCount: 0,
          successfulApplicationCount: 0,
        })
        .run();

      if (isPrimary) {
        result.principleAdded = shape.name;
        result.principleFilesWritten = { dslPath, jsonPath };
      } else {
        result.alternateShapesWritten.push({
          name: shape.name,
          bugClassId: shape.bugClassId,
          dslPath,
          jsonPath,
        });
      }
    }
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
  // this bug but didn't) belongs to a separate `provekit review` CLI command.
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
