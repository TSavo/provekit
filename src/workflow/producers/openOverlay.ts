/**
 * OpenOverlay action — bug-fix workflow's scratch-worktree creation (C2).
 *
 * Wraps openOverlay() in an Action<I, R>. Side-effecting by nature:
 *   - creates a scratch directory on disk (mkdtemp)
 *   - runs `git worktree add --detach` against the host repo
 *   - opens + migrates a fresh sqlite SAST DB
 *   - copies the principles directory in
 *   - pre-indexes the locus file
 *
 * Action contract:
 *   Actions are impure, run-every-time units of work. The runner never
 *   looks up a cached result — it always invokes run() and writes an
 *   audit-only memento (kind: action-invocation) recording what was
 *   produced. The OverlayHandle is a RESOURCE (live filesystem +
 *   sqlite handle), not a CLAIM, so it must not enter the proof DAG
 *   as a content-addressable reference.
 *
 *   serializeInput returns only the content-defining fields for the
 *   audit memento. The runner injects an internal _auditSalt (UUID)
 *   so each invocation produces a distinct propertyHash and audit row
 *   — this is the runner's responsibility, not the action's.
 *
 *   describeResource captures metadata (worktree path, baseRef) for
 *   the audit memento's witness without holding a reference to the
 *   live resource.
 *
 * Spec: docs/specs/2026-04-29-stages-vs-actions.md
 */

import { openOverlay as openOverlayImpl } from "../../fix/stages/openOverlay.js";
import type { BugLocus, OverlayHandle } from "../../fix/types.js";
import type { Db } from "../../db/index.js";
import type { Action } from "../types.js";

export const OPEN_OVERLAY_CAPABILITY = "openOverlay";

export interface OpenOverlayActionInput {
  locus: BugLocus;
}

export interface MakeOpenOverlayActionDeps {
  db: Db;
  /** Override producer identity. Default: "openOverlay@v1". */
  producerVersion?: string;
}

export function makeOpenOverlayAction(
  deps: MakeOpenOverlayActionDeps,
): Action<OpenOverlayActionInput, OverlayHandle> {
  const producedBy = deps.producerVersion ?? "openOverlay@v1";

  return {
    name: "openOverlay",
    producedBy,

    serializeInput(input) {
      // Return only the content-defining fields for the audit memento.
      // The runner adds the _auditSalt internally so each invocation
      // produces a distinct propertyHash — no salt needed here.
      return { locus: input.locus };
    },

    describeResource(handle) {
      return `worktree at ${handle.worktreePath} from ${handle.baseRef}`;
    },

    async run(input) {
      return openOverlayImpl({
        locus: input.locus,
        db: deps.db,
      });
    },
  };
}
