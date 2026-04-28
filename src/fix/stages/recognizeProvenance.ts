/**
 * B3 mechanical-mode (C6m) provenance helper.
 *
 * When B3 recognizes a bug and the loop completes, C6m appends a provenance
 * entry to the matched LibraryPrinciple's JSON file on disk. This records
 * that the customer's bug was an instance of an existing principle (not a
 * novel discovery) so the harvest pipeline can compute cluster sizes.
 *
 * The append is best-effort: a failure to write the file does NOT abort the
 * fix loop. The principle still applies; only the audit trail is incomplete.
 */

import { readFileSync, writeFileSync, existsSync } from "fs";
import { basename } from "path";
import type { BugProvenance, LibraryPrinciple } from "../types.js";
import { findPrinciplesDir } from "./recognize.js";
import { enumeratePrincipleFiles } from "../../principleEnumeration.js";
import { createNoopLogger, type FixLoopLogger } from "../logger.js";

export interface AppendProvenanceArgs {
  principleId: string;
  entry: BugProvenance;
  logger?: FixLoopLogger;
  /** Override directory (test injection). Defaults to findPrinciplesDir(). */
  dir?: string;
}

export function appendLibraryProvenance(args: AppendProvenanceArgs): void {
  const logger = args.logger ?? createNoopLogger();
  const dir = args.dir ?? findPrinciplesDir();

  // Task #134: principle library is partitioned. Locate the principle's
  // JSON in whichever partition it actually lives. Walks every partition
  // (loadAllPartitions=true) since a B3-recognized principle could be
  // universal/ or any per-language partition.
  const filename = `${args.principleId}.json`;
  const { jsonPaths } = enumeratePrincipleFiles(dir, {
    loadAllPartitions: true,
  });
  const path = jsonPaths.find((p) => basename(p) === filename);

  if (!path || !existsSync(path)) {
    logger.detail(`[C6m] WARN: principle file ${filename} not found under ${dir}; skipping provenance append`);
    return;
  }

  let principle: LibraryPrinciple;
  try {
    principle = JSON.parse(readFileSync(path, "utf-8")) as LibraryPrinciple;
  } catch (err) {
    logger.detail(
      `[C6m] WARN: failed to parse ${path}: ${err instanceof Error ? err.message : String(err)}`,
    );
    return;
  }

  const existing: BugProvenance[] = (() => {
    if (!principle.provenance) return [];
    if (Array.isArray(principle.provenance)) return principle.provenance;
    return [principle.provenance];
  })();
  existing.push(args.entry);
  principle.provenance = existing;

  try {
    writeFileSync(path, JSON.stringify(principle, null, 2) + "\n", "utf-8");
    logger.detail(`[C6m] appended provenance to ${args.principleId} (${existing.length} entries)`);
  } catch (err) {
    logger.detail(
      `[C6m] WARN: failed to write ${path}: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}
