/**
 * WriteInvariantFile action — write `.invariant.ts` source to disk.
 *
 * Used by workflows that mutate the on-disk catalog (weaken, strengthen,
 * retire). The file write is side-effecting, so it ships as an Action
 * rather than a Stage: the audit memento records the path written and
 * a digest of the content, and the caller's downstream Stages (e.g.
 * verification of the new contract) compose against the formula
 * produced by formulate-via-lifter — NOT against the file path.
 *
 * Mode semantics:
 *   - "overwrite": replace the file's contents with `surfaceText`.
 *   - "append":   append `surfaceText` to the existing file (creating
 *                 it if missing). retire uses this to splice in a
 *                 deprecation marker without touching the rest.
 *
 * The Action's resource is the absolute path written + a sha256 of
 * the content. describeResource serializes both; the live filesystem
 * handle is implicit (the path).
 *
 * Spec: docs/specs/2026-04-29-stages-vs-actions.md
 */

import { mkdirSync, writeFileSync, readFileSync, existsSync } from "fs";
import { dirname, isAbsolute } from "path";
import { createHash } from "crypto";
import type { Action } from "../types.js";

export const WRITE_INVARIANT_FILE_CAPABILITY = "write-invariant-file";

export interface WriteInvariantFileActionInput {
  /** Absolute path to the target `.invariant.ts` file. */
  path: string;
  /** Source text to write. */
  surfaceText: string;
  /**
   * "overwrite" replaces the file's contents; "append" adds to the
   * existing contents (creating the file if it doesn't exist).
   */
  mode: "overwrite" | "append";
}

export interface WriteInvariantFileResource {
  /** The absolute path written. */
  path: string;
  /** sha256 of the final file contents after the write. */
  contentSha256: string;
  /** Number of bytes written by THIS invocation (not total file size). */
  bytesWritten: number;
  /** The mode that was used. */
  mode: "overwrite" | "append";
}

export interface MakeWriteInvariantFileActionDeps {
  /** Override producer identity. Default: "writeInvariantFile@v1". */
  producerVersion?: string;
}

export function makeWriteInvariantFileAction(
  deps: MakeWriteInvariantFileActionDeps = {},
): Action<WriteInvariantFileActionInput, WriteInvariantFileResource> {
  const producedBy = deps.producerVersion ?? "writeInvariantFile@v1";

  return {
    name: "writeInvariantFile",
    producedBy,

    serializeInput(input) {
      // Hash a digest of the content rather than the content itself so
      // audit rows stay small for large surface texts. The runner's
      // _auditSalt makes the row unique per invocation regardless.
      return {
        path: input.path,
        mode: input.mode,
        contentSha256: sha256(input.surfaceText),
      };
    },

    describeResource(resource) {
      return `wrote ${resource.bytesWritten} bytes (${resource.mode}) to ${resource.path} → sha256:${resource.contentSha256}`;
    },

    async run(input) {
      if (!isAbsolute(input.path)) {
        throw new Error(
          `writeInvariantFile.path must be absolute, got "${input.path}"`,
        );
      }
      mkdirSync(dirname(input.path), { recursive: true });

      const bytesWritten = Buffer.byteLength(input.surfaceText, "utf-8");
      let finalContents: string;
      if (input.mode === "append" && existsSync(input.path)) {
        const existing = readFileSync(input.path, "utf-8");
        finalContents = existing + input.surfaceText;
        writeFileSync(input.path, finalContents, "utf-8");
      } else {
        finalContents = input.surfaceText;
        writeFileSync(input.path, finalContents, "utf-8");
      }

      return {
        path: input.path,
        contentSha256: sha256(finalContents),
        bytesWritten,
        mode: input.mode,
      };
    },
  };
}

function sha256(s: string): string {
  return createHash("sha256").update(s, "utf-8").digest("hex");
}
