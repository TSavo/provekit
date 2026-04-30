/**
 * Stage execution helper for the fix loop.
 *
 * Wraps every stage with a uniform shape:
 *   1. Log the stage start.
 *   2. Run the stage's produce function.
 *   3. Persist the artifact to disk under the run-scoped artifact dir.
 *   4. Extend the context immutably with the new artifact.
 *   5. Log the stage end + duration.
 *   6. Append an audit entry.
 *
 * The persistence step is what makes the run dir a literal audit trail:
 * after a run, .provekit/contexts/<run-id>/ contains every stage's
 * artifact as JSON, which means any stage can be replayed by re-loading
 * its inputs from disk.
 */

import { mkdirSync, writeFileSync } from "fs";
import { join } from "path";
import type { FixLoopContext } from "./FixLoopContext.js";
import { extendContext } from "./FixLoopContext.js";
import type { FixLoopRuntime } from "./FixLoopRuntime.js";

export interface AuditEntry {
  stage: string;
  kind: "start" | "complete" | "error";
  detail: string;
  timestamp: number;
}

export interface RunStageResult<TKey extends keyof FixLoopContext> {
  readonly ctx: FixLoopContext;
  readonly artifact: NonNullable<FixLoopContext[TKey]>;
  readonly durationMs: number;
}

/**
 * Run a stage with uniform observability + persistence. Throws on
 * stage error after appending an "error" audit entry — the caller's
 * try/catch decides whether to abort the run or recover.
 */
export async function runStage<TKey extends Exclude<keyof FixLoopContext, "runId" | "bugReport">>(
  ctx: FixLoopContext,
  rt: FixLoopRuntime,
  audit: AuditEntry[],
  stageName: string,
  stageKey: TKey,
  produce: (ctx: FixLoopContext, rt: FixLoopRuntime) => Promise<NonNullable<FixLoopContext[TKey]>>,
): Promise<RunStageResult<TKey>> {
  const t0 = Date.now();
  audit.push({ stage: stageName, kind: "start", detail: stageKey, timestamp: t0 });
  rt.logger.stage(stageName);

  let artifact: NonNullable<FixLoopContext[TKey]>;
  try {
    artifact = await produce(ctx, rt);
  } catch (err) {
    audit.push({
      stage: stageName,
      kind: "error",
      detail: (err as Error).message,
      timestamp: Date.now(),
    });
    throw err;
  }

  // Persist as a stable JSON artifact under the run dir. Stage key is the
  // filename so the disk layout mirrors the context shape exactly.
  try {
    mkdirSync(rt.runDir, { recursive: true });
    const artifactPath = join(rt.runDir, `${stageKey}.json`);
    writeFileSync(
      artifactPath,
      JSON.stringify(artifact, jsonReplacerForUnserializable, 2),
      "utf-8",
    );
  } catch (err) {
    // Persistence failure is non-fatal — the in-memory artifact is still
    // available to downstream stages; we just lose the audit-trail copy.
    rt.logger.error(
      `runStage: failed to persist ${stageKey} artifact: ${(err as Error).message}`,
    );
  }

  const durationMs = Date.now() - t0;
  audit.push({ stage: stageName, kind: "complete", detail: stageKey, timestamp: Date.now() });
  rt.logger.info(`  ${stageName} stage OK in ${durationMs}ms`);

  const nextCtx = extendContext(ctx, stageKey, artifact);
  return { ctx: nextCtx, artifact, durationMs };
}

/**
 * JSON replacer that drops non-serializable fields (functions, sqlite
 * handles, ts-morph nodes) instead of throwing. The artifact persistence
 * is a best-effort audit trail; stages can carry richer in-memory state
 * than what survives a JSON round trip.
 */
function jsonReplacerForUnserializable(_key: string, value: unknown): unknown {
  if (typeof value === "function") return "[Function]";
  if (value && typeof value === "object" && "constructor" in value) {
    const ctor = (value as { constructor?: { name?: string } }).constructor;
    const ctorName = ctor?.name ?? "";
    if (ctorName === "Database" || ctorName === "Statement") return `[${ctorName}]`;
    if (ctorName === "SourceFile" || ctorName === "Node") return `[${ctorName}]`;
  }
  return value;
}
