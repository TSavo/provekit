// D3 stub — landing zone for learning/knowledge-base update after successful apply.
import type { FixBundle, ApplyResult } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function learnFromBundle(args: {
  bundle: FixBundle;
  applyResult: ApplyResult;
  db: Db;
}): Promise<void> {
  void args;
  throw new NotImplementedError(
    "D3",
    "learnFromBundle (D3) not yet implemented — B5 orchestrator will route around it when D3 lands",
  );
}
