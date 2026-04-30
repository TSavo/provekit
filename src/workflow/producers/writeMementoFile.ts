/**
 * WriteMementoFile Action — write a signed memento envelope to disk as
 * pretty-printed JSON.
 *
 * Used by the mint workflow as the side-effecting tail of the chain.
 * The Stage produces the envelope (cacheable, content-addressable);
 * this Action serializes it to a path. The audit memento records the
 * absolute path and the content sha256, not the envelope's raw bytes.
 *
 * Stdout-only emission (the `--out` not supplied case in cli.mint.ts)
 * is NOT this Action's concern; it stays in the CLI shim. Actions
 * produce resources — a stdout write does not yield a resource handle
 * the audit DAG can refer to.
 *
 * Spec: docs/specs/2026-04-29-stages-vs-actions.md
 */

import { writeFileSync, mkdirSync } from "fs";
import { dirname, isAbsolute } from "path";
import { createHash } from "crypto";
import type { Action } from "../types.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";

export const WRITE_MEMENTO_FILE_CAPABILITY = "write-memento-file";

export interface WriteMementoFileInput {
  envelope: ClaimEnvelope;
  /** Absolute path to write to. */
  path: string;
}

export interface WriteMementoFileResource {
  path: string;
  /** Content-address of what was written. Lets the audit memento record EXACTLY what landed. */
  contentSha256: string;
  bytesWritten: number;
  /** The envelope's CID — surfaced for callers that want to verify path↔CID mapping. */
  envelopeCid: string;
}

export interface MakeWriteMementoFileActionDeps {
  /** Override producer identity. Default: "writeMementoFile@v1". */
  producerVersion?: string;
}

export function makeWriteMementoFileAction(
  deps: MakeWriteMementoFileActionDeps = {},
): Action<WriteMementoFileInput, WriteMementoFileResource> {
  const producedBy = deps.producerVersion ?? "writeMementoFile@v1";

  return {
    name: "writeMementoFile",
    producedBy,

    serializeInput(input) {
      // The envelope's CID is the content fingerprint — hashing the
      // CID is sufficient for the audit memento and avoids embedding
      // the entire envelope in the audit row.
      return {
        path: input.path,
        envelopeCid: input.envelope.cid,
      };
    },

    describeResource(resource) {
      return `wrote ${resource.bytesWritten} bytes (cid: ${resource.envelopeCid}) to ${resource.path} → sha256:${resource.contentSha256}`;
    },

    async run(input) {
      if (!isAbsolute(input.path)) {
        throw new Error(
          `writeMementoFile.path must be absolute, got "${input.path}"`,
        );
      }
      mkdirSync(dirname(input.path), { recursive: true });
      const json = JSON.stringify(input.envelope, null, 2) + "\n";
      writeFileSync(input.path, json, "utf-8");
      return {
        path: input.path,
        contentSha256: createHash("sha256").update(json, "utf-8").digest("hex"),
        bytesWritten: Buffer.byteLength(json, "utf-8"),
        envelopeCid: input.envelope.cid,
      };
    },
  };
}
