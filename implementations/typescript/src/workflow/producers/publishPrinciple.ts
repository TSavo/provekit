/**
 * Publish-principle action — principalize workflow's terminal mint (P4).
 *
 * Side-effecting: writes a `LibraryPrinciple` JSON file to
 * `.provekit/principles/<id>.json` under the host project root, the
 * canonical location B3 (recognize) loads from. An Action — not a
 * Stage — because the principle library on disk is a shared resource
 * the rest of the framework reads; mutating it is impure and must not
 * be cached.
 *
 * Spec: protocol/specs/2026-04-29-stages-vs-actions.md (Action contract)
 *       src/fix/stages/recognize.ts (LibraryPrinciple loader)
 *
 * The Action's resource is the path to the file written. The audit
 * memento (kind: action-invocation) records the principle name +
 * filesystem path so a forensic walk can reach the published file.
 *
 * Validation contract: when the upstream validate-adversarial stage's
 * verdict is "false-positive", the caller is expected to skip
 * publishing. This Action does NOT inspect the validation result —
 * the workflow YAML expresses the gate by simply not invoking publish
 * when the verdict is dirty. Keeping policy out of the Action keeps
 * the substrate small.
 *
 * Backwards-compat with manual edits: if the target file already
 * exists, the existing JSON is read and merged field-by-field with
 * the new entry. New fields win; existing fields the new entry
 * doesn't carry are preserved. Mirrors the merge behavior in
 * src/fix/harvest/promote.ts.
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";
import type { Action } from "../types.js";
import type { LibraryPrinciple } from "../../fix/types.js";
import type { ShapeCluster } from "./clusterByShape.js";

export const PUBLISH_PRINCIPLE_CAPABILITY = "publish-principle";

export interface PublishPrincipleActionInput {
  /** Host project root containing `.provekit/principles/`. */
  projectRoot: string;
  /** Principle id (filename root). Must match LibraryPrinciple.id. */
  principleName: string;
  /** Bug-class slug. Often equals principleName for canonical shapes. */
  bugClassId: string;
  /**
   * Cluster being lifted. Used for provenance metadata so the resulting
   * file records which propertyHashes contributed to the principle.
   * Null when the workflow's cluster-by-shape stage saw an empty
   * corpus; the Action no-ops in that case, still producing an audit
   * memento that records the no-op outcome.
   */
  cluster: ShapeCluster | null;
  /**
   * Optional human-friendly description; defaults to a generated one
   * derived from the cluster's shape.
   */
  description?: string;
  /** Confidence label. Defaults to "medium" — same default as harvest. */
  confidence?: "high" | "medium" | "low";
}

export interface PublishPrincipleResource {
  /** Absolute path to the written JSON file. Null on no-op. */
  jsonPath: string | null;
  /** The principle id that was published (== filename root). */
  principleId: string;
  /**
   * One of:
   *   "created" — wrote a new principle JSON.
   *   "merged"  — updated an existing principle JSON.
   *   "skipped" — no cluster (empty corpus); nothing written.
   */
  outcome: "created" | "merged" | "skipped";
}

export interface MakePublishPrincipleActionDeps {
  producerVersion?: string;
}

export function makePublishPrincipleAction(
  deps: MakePublishPrincipleActionDeps = {},
): Action<PublishPrincipleActionInput, PublishPrincipleResource> {
  const producedBy = deps.producerVersion ?? "publish-principle@v1";

  return {
    name: "publish-principle",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        principleName: input.principleName,
        bugClassId: input.bugClassId,
        clusterFingerprint: input.cluster?.fingerprint ?? null,
        clusterMembers: input.cluster
          ? [...input.cluster.members].sort()
          : null,
        description: input.description ?? null,
        confidence: input.confidence ?? "medium",
      };
    },

    describeResource(resource) {
      if (resource.outcome === "skipped") {
        return `skipped ${resource.principleId} (empty corpus)`;
      }
      return `${resource.outcome} ${resource.principleId} at ${resource.jsonPath}`;
    },

    async run(input) {
      if (input.cluster === null) {
        return {
          jsonPath: null,
          principleId: input.principleName,
          outcome: "skipped",
        };
      }
      const dir = join(input.projectRoot, ".provekit", "principles");
      mkdirSync(dir, { recursive: true });
      const jsonPath = join(dir, `${input.principleName}.json`);
      const existed = existsSync(jsonPath);

      const description =
        input.description ??
        `Lifted from ${input.cluster.members.length} invariant${
          input.cluster.members.length === 1 ? "" : "s"
        } sharing shape ${input.cluster.fingerprint}.`;

      const fresh: LibraryPrinciple = {
        id: input.principleName,
        bug_class_id: input.bugClassId,
        name: input.principleName,
        description,
        confidence: input.confidence ?? "medium",
        provenance: [
          {
            source: "harvest",
            projectId: "principalize-workflow",
            bugId: input.cluster.fingerprint,
            timestamp: new Date().toISOString(),
          },
        ],
      };

      let merged: LibraryPrinciple = fresh;
      if (existed) {
        try {
          const existing = JSON.parse(
            readFileSync(jsonPath, "utf-8"),
          ) as LibraryPrinciple;
          merged = {
            ...existing,
            ...fresh,
            provenance: mergeProvenance(existing.provenance, fresh.provenance),
          };
        } catch {
          // Corrupt existing file — write fresh; principle authors are
          // expected to inspect the result.
        }
      }

      writeFileSync(
        jsonPath,
        JSON.stringify(merged, null, 2) + "\n",
        "utf-8",
      );

      return {
        jsonPath,
        principleId: input.principleName,
        outcome: existed ? "merged" : "created",
      };
    },
  };
}

function mergeProvenance(
  existing: LibraryPrinciple["provenance"],
  incoming: LibraryPrinciple["provenance"],
): LibraryPrinciple["provenance"] {
  const e = existing ? (Array.isArray(existing) ? existing : [existing]) : [];
  const i = incoming ? (Array.isArray(incoming) ? incoming : [incoming]) : [];
  return [...e, ...i];
}
