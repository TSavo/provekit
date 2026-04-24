// C2 stub — landing zone for open scratch worktree + reindex implementation.
import type { BugLocus, OverlayHandle } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function openOverlay(args: {
  locus: BugLocus;
  db: Db;
}): Promise<OverlayHandle> {
  void args;
  throw new NotImplementedError(
    "C2",
    "openOverlay (C2) not yet implemented — B5 orchestrator will route around it when C2 lands",
  );
}
