// D2 stub — landing zone for bundle application (PR creation or direct apply).
import type { FixBundle, ApplyResult } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function applyBundle(args: {
  bundle: FixBundle;
  options: { autoApply: boolean; prDraftMode: boolean };
  db: Db;
}): Promise<ApplyResult> {
  void args;
  throw new NotImplementedError(
    "D2",
    "applyBundle (D2) not yet implemented — B5 orchestrator will route around it when D2 lands",
  );
}
