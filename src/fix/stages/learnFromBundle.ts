/**
 * D3: Stage shim — delegates to src/fix/learn.ts.
 *
 * The orchestrator calls this file via the stages/ convention. The real
 * implementation lives in learn.ts so it can be tested independently and
 * imported without pulling in the full stage wiring.
 */

import type { FixBundle, ApplyResult } from "../types.js";
import type { Db } from "../../db/index.js";
import { learnFromBundle as _learnFromBundle } from "../learn.js";

export async function learnFromBundle(args: {
  bundle: FixBundle;
  applyResult: ApplyResult;
  db: Db;
}): Promise<void> {
  // Orchestrator does not use the LearnResult — it only cares that D3
  // completes without throwing. Return value is intentionally dropped.
  await _learnFromBundle(args);
}
