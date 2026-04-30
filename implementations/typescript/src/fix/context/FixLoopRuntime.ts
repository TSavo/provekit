/**
 * Stateful infrastructure threaded through every stage of the fix loop.
 *
 * Separated from FixLoopContext (which is the immutable artifact bag)
 * because these are stateful by nature: a Db handle owns a SQLite
 * connection, an LLMProvider mutates internal call history, a logger
 * appends. Keeping them out of the immutable context lets us claim the
 * context honestly: nothing already-recorded ever changes.
 *
 * Stages take BOTH (ctx, rt). Read upstream artifacts from ctx; call
 * services through rt.
 */

import type { Db } from "../../db/index.js";
import type { LLMProvider } from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import type { OverlayHandle } from "../types.js";

export interface FixLoopRuntime {
  /** Per-project SAST + harvest sqlite handle. Read-shared across stages. */
  readonly db: Db;
  /** LLM seam for stage prompts. The agent property is consulted for capture-the-change. */
  readonly llm: LLMProvider;
  /** Stage entry/exit + LLM-call telemetry sink. */
  readonly logger: FixLoopLogger;
  /** Run-scoped artifact directory: .provekit/contexts/<run-id>/. Per-stage JSON lands here. */
  readonly runDir: string;
  /** Project root the fix is operating against (the user's repo, not provekit). */
  readonly projectRoot: string;

  // Optional injectable test runners — keep at runtime layer because they
  // are stateful (spawn child processes); not part of the artifact trail.
  readonly vitestRunner?: (overlay: OverlayHandle) => { exitCode: number; stdout: string; stderr: string };
  readonly c5TestRunner?: (overlay: OverlayHandle, testFilePath: string, mainRepoRoot: string) => { exitCode: number; stdout: string; stderr: string };
}
