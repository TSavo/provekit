/**
 * OpenOverlay stage — bug-fix workflow's scratch-worktree creation (C2).
 *
 * Wraps openOverlay() in a Stage<I, O>. Heavily side-effecting:
 *   - creates a scratch directory on disk (mkdtemp)
 *   - runs `git worktree add --detach` against the host repo
 *   - opens + migrates a fresh sqlite SAST DB
 *   - copies the principles directory in
 *   - pre-indexes the locus file
 *
 * Cache contract — DELIBERATELY DEFEATED:
 *   The Stage exists for capability-dispatch uniformity (workflows
 *   address openOverlay the same way they address recognize / formulate
 *   / etc.) but caching it would be wrong. Each call MUST produce a
 *   fresh worktree on disk; a "cache hit" returning the path of a
 *   prior overlay would either crash (path removed by closeOverlay) or
 *   silently corrupt the next run by reusing the prior overlay's
 *   modified state.
 *
 *   We defeat the cache by mixing a per-call salt (crypto.randomUUID)
 *   into serializeInput so every call gets a fresh propertyHash and
 *   thus a fresh memento row. Re-running yields a new worktree every
 *   time — same as calling openOverlay() directly. The memento that
 *   gets written is effectively a log entry, not a reusable witness;
 *   downstream consumers reach into the result directly rather than
 *   through cache lookups.
 *
 *   The OverlayHandle returned holds a live Db (sastDb) and an open
 *   filesystem worktree — neither survives JSON round-trip. This is
 *   why the witness is essentially write-only: serializeOutput emits
 *   only the content-defining fields (worktreePath, baseRef,
 *   sastDbPath); deserializeOutput reconstructs a stub with closed=true
 *   and an unusable sastDb to make accidental cache-hit consumption
 *   fail loudly. We never expect that path to fire because the salt
 *   defeats hits.
 *
 * Construction-time deps: db (the MAIN repo's SAST DB, used by
 * openOverlay() to resolve file ids when pre-indexing). Per-call
 * input: locus.
 */

import { openOverlay as openOverlayImpl } from "../../fix/stages/openOverlay.js";
import type { BugLocus, OverlayHandle } from "../../fix/types.js";
import type { Db } from "../../db/index.js";
import type { Stage } from "../types.js";

export const OPEN_OVERLAY_CAPABILITY = "openOverlay";

export interface OpenOverlayStageInput {
  locus: BugLocus;
}

export interface MakeOpenOverlayStageDeps {
  db: Db;
  /** Override producer identity. Default: "openOverlay@v1". */
  producerVersion?: string;
}

export function makeOpenOverlayStage(
  deps: MakeOpenOverlayStageDeps,
): Stage<OpenOverlayStageInput, OverlayHandle> {
  const producedBy = deps.producerVersion ?? "openOverlay@v1";

  return {
    name: "openOverlay",
    producedBy,

    serializeInput(input) {
      // Per-call salt: defeat cache. Each call gets a fresh propertyHash
      // so the runner always falls through to run() and produces a fresh
      // worktree on disk. See file header for rationale.
      return {
        locus: input.locus,
        _cacheBuster: cryptoRandomUUID(),
      };
    },

    serializeOutput(output) {
      // Witness keeps the content-defining fields only. Live runtime
      // resources (sastDb handle, modifiedFiles set) are not
      // serializable — and per the cache contract, deserialization is
      // not expected to fire in practice.
      return JSON.stringify({
        worktreePath: output.worktreePath,
        sastDbPath: output.sastDbPath,
        baseRef: output.baseRef,
      });
    },

    deserializeOutput(_witness): OverlayHandle {
      // Cache hits should never happen for this stage (see header). If
      // one does, returning a stub with closed=true makes downstream
      // consumption fail loudly rather than silently corrupting state.
      throw new Error(
        "openOverlay Stage: cache reconstruction not supported — " +
          "this stage is intentionally cache-defeated; check serializeInput salt",
      );
    },

    async run(input) {
      return openOverlayImpl({
        locus: input.locus,
        db: deps.db,
      });
    },
  };
}

/**
 * Indirection so tests can stub the salt and assert salt-driven hash
 * variation deterministically. Production reads `crypto.randomUUID()`.
 */
function cryptoRandomUUID(): string {
  return globalThis.crypto.randomUUID();
}
