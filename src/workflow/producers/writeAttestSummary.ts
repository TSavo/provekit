/**
 * WriteAttestSummary Action — write the verify-project-invariants output
 * to a JSON file (the `attest-summary.json` produced by `--out <dir>`
 * in cli.attest.ts).
 *
 * Side-effecting: filesystem write. The audit memento records the path
 * + the project root CID + the null-root count so auditors can confirm
 * what shipped without re-running the verification.
 *
 * Spec: protocol/specs/2026-04-29-stages-vs-actions.md
 */

import { writeFileSync, mkdirSync } from "fs";
import { dirname, isAbsolute } from "path";
import { createHash } from "crypto";
import type { Action } from "../types.js";
import type { VerifyProjectInvariantsStageOutput } from "./verifyProjectInvariants.js";

export const WRITE_ATTEST_SUMMARY_CAPABILITY = "write-attest-summary";

export interface WriteAttestSummaryInput {
  summary: VerifyProjectInvariantsStageOutput;
  /** Project name + version, written into the summary file alongside the verdicts. */
  projectName: string;
  projectVersion: string;
  /** Absolute path to write the summary JSON to. */
  outPath: string;
}

export interface WriteAttestSummaryResource {
  path: string;
  contentSha256: string;
  bytesWritten: number;
  /** Project root CID surfaced for the audit memento's witness. */
  projectRootCid: string;
  /** Null root count surfaced for fast auditing without re-reading the file. */
  nullRootCount: number;
}

export interface MakeWriteAttestSummaryActionDeps {
  /** Override producer identity. Default: "writeAttestSummary@v1". */
  producerVersion?: string;
}

export function makeWriteAttestSummaryAction(
  deps: MakeWriteAttestSummaryActionDeps = {},
): Action<WriteAttestSummaryInput, WriteAttestSummaryResource> {
  const producedBy = deps.producerVersion ?? "writeAttestSummary@v1";

  return {
    name: "writeAttestSummary",
    producedBy,

    serializeInput(input) {
      return {
        path: input.outPath,
        projectName: input.projectName,
        projectVersion: input.projectVersion,
        projectRootCid: input.summary.projectRootCid,
        declarationCount: input.summary.declarations.length,
        nullRootCount: input.summary.nullRoots.length,
      };
    },

    describeResource(resource) {
      return `wrote attest summary (${resource.bytesWritten} bytes, projectRoot=${resource.projectRootCid}, nullRoots=${resource.nullRootCount}) to ${resource.path}`;
    },

    async run(input) {
      if (!isAbsolute(input.outPath)) {
        throw new Error(
          `writeAttestSummary.outPath must be absolute, got "${input.outPath}"`,
        );
      }
      mkdirSync(dirname(input.outPath), { recursive: true });
      const payload = {
        projectName: input.projectName,
        projectVersion: input.projectVersion,
        projectRootCid: input.summary.projectRootCid,
        declarations: input.summary.declarations,
        nullRoots: input.summary.nullRoots,
      };
      const json = JSON.stringify(payload, null, 2) + "\n";
      writeFileSync(input.outPath, json, "utf-8");
      return {
        path: input.outPath,
        contentSha256: createHash("sha256").update(json, "utf-8").digest("hex"),
        bytesWritten: Buffer.byteLength(json, "utf-8"),
        projectRootCid: input.summary.projectRootCid,
        nullRootCount: input.summary.nullRoots.length,
      };
    },
  };
}
