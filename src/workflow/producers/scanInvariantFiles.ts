/**
 * ScanInvariantFiles Stage — walk a project tree for *.invariant.ts
 * files and return their (path, contentHash, resolvedModulePath)
 * triples for downstream consumption by verify-project-invariants.
 *
 * Migration of src/cli.attest.ts's findInvariantFiles() helper. Per
 * the migration brief this is a Stage (not an Action) — same compromise
 * as other filesystem-walking Stages in the codebase: pure given the
 * named projectRoot, with the cache footgun that adding/changing a
 * .invariant.ts file mid-run invalidates the cached witness only on
 * the next miss. Acceptable: attest is run by users who expect the
 * walk to reflect current disk state.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import { existsSync, readdirSync, readFileSync, statSync } from "fs";
import { createHash } from "crypto";
import { join, relative } from "path";
import type { Stage } from "../types.js";
import type { InvariantFileSource } from "./verifyProjectInvariants.js";

export const SCAN_INVARIANT_FILES_CAPABILITY = "scan-invariant-files";

const SKIP_DIRS = new Set([
  "node_modules",
  "dist",
  "lib",
  "__fixtures__",
]);

const INVARIANT_SUFFIXES = [
  ".invariant.ts",
  ".invariant.mjs",
  ".invariant.js",
];

export interface ScanInvariantFilesInput {
  /** Absolute path to walk for .invariant.ts files. */
  scanRoot: string;
  /**
   * Project root used to compute the relative `path` field on each
   * discovered file. Often equal to or an ancestor of scanRoot.
   */
  projectRoot: string;
}

export interface ScanInvariantFilesOutput {
  files: InvariantFileSource[];
}

export interface MakeScanInvariantFilesStageDeps {
  /** Override producer identity. Default: "scanInvariantFiles@v1". */
  producerVersion?: string;
}

export function makeScanInvariantFilesStage(
  deps: MakeScanInvariantFilesStageDeps = {},
): Stage<ScanInvariantFilesInput, ScanInvariantFilesOutput> {
  const producedBy = deps.producerVersion ?? "scanInvariantFiles@v1";

  return {
    name: "scanInvariantFiles",
    producedBy,

    serializeInput(input) {
      return {
        scanRoot: input.scanRoot,
        projectRoot: input.projectRoot,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ScanInvariantFilesOutput;
    },

    async run(input) {
      return runScanInvariantFiles(input);
    },
  };
}

export async function runScanInvariantFiles(
  input: ScanInvariantFilesInput,
): Promise<ScanInvariantFilesOutput> {
  const found: InvariantFileSource[] = [];
  if (!existsSync(input.scanRoot)) {
    return { files: found };
  }

  function walk(dir: string): void {
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.startsWith(".") || SKIP_DIRS.has(entry)) continue;
      const full = join(dir, entry);
      let stats;
      try {
        stats = statSync(full);
      } catch {
        continue;
      }
      if (stats.isDirectory()) {
        walk(full);
        continue;
      }
      if (INVARIANT_SUFFIXES.some((suffix) => entry.endsWith(suffix))) {
        const content = readFileSync(full, "utf-8");
        found.push({
          path: relative(input.projectRoot, full),
          contentHash: sha256Hex(content),
          resolvedModulePath: full,
        });
      }
    }
  }

  walk(input.scanRoot);
  // Stable order — relative path drives the project root memento's
  // inputCids ordering downstream.
  found.sort((a, b) => a.path.localeCompare(b.path));
  return { files: found };
}

function sha256Hex(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}
