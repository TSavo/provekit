/**
 * WriteInvariantFile action — write a synthesized invariant's surface
 * text to <basename>.invariant.ts on disk.
 *
 * Action contract per `docs/specs/2026-04-29-stages-vs-actions.md`:
 *   - Side-effecting (writes a file)
 *   - Run every time (different filesystem state across runs)
 *   - Audit memento records WHAT was written (filename + content hash);
 *     the resource is the OS file handle the workflow's downstream
 *     stages can reference
 *
 * Scope discipline (per `2026-04-29-correctness-is-a-hash.md` §"What
 * ProvekIt is"): this action mints a write. It does NOT verify the
 * file's content (the lift+canonicalize already happened in the
 * formulate Stage upstream); it does NOT walk into the file's
 * contract-target dependencies (audit work). Output is the file path
 * + the bytes that landed.
 */

import { writeFileSync, existsSync, readFileSync, mkdirSync } from "node:fs";
import { dirname, basename, join, resolve } from "node:path";
import { createHash } from "node:crypto";
import type { Action } from "../types.js";

export const WRITE_INVARIANT_FILE_CAPABILITY = "write-invariant-file";

export interface WriteInvariantFileInput {
  /** Absolute path of the production-code file the invariant is about. */
  targetFile: string;
  /** TypeScript source — the LLM-emitted invariant code, lifted+verified upstream. */
  surfaceText: string;
  /** Append mode: if a *.invariant.ts file already exists, append to it
   *  instead of overwriting. Default false (overwrite). */
  append?: boolean;
}

export interface WriteInvariantFileHandle {
  /** Path of the .invariant.ts file written. */
  invariantFilePath: string;
  /** sha256 of the file's final content. */
  contentHash: string;
  /** Number of bytes written. */
  bytesWritten: number;
  /** True if a previously-existing file was overwritten or appended to. */
  preExisting: boolean;
}

export interface MakeWriteInvariantFileDeps {
  producerVersion?: string;
}

export function makeWriteInvariantFileAction(
  deps: MakeWriteInvariantFileDeps = {},
): Action<WriteInvariantFileInput, WriteInvariantFileHandle> {
  const producedBy = deps.producerVersion ?? "writeInvariantFile@v1";

  return {
    name: "writeInvariantFile",
    producedBy,

    serializeInput(input) {
      return {
        targetFile: input.targetFile,
        surfaceText: input.surfaceText,
        append: input.append ?? false,
      };
    },

    describeResource(handle) {
      return (
        `wrote ${handle.bytesWritten} bytes to ${handle.invariantFilePath} ` +
        `(content sha256: ${handle.contentHash.slice(0, 16)}…; ` +
        `preExisting: ${handle.preExisting})`
      );
    },

    async run(input) {
      return writeInvariantFile(input);
    },
  };
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

export function writeInvariantFile(
  input: WriteInvariantFileInput,
): WriteInvariantFileHandle {
  const target = resolve(input.targetFile);
  const dir = dirname(target);
  const base = basename(target).replace(/\.tsx?$/, "");
  const invariantFilePath = join(dir, `${base}.invariant.ts`);

  const preExisting = existsSync(invariantFilePath);
  const dirExists = existsSync(dir);
  if (!dirExists) mkdirSync(dir, { recursive: true });

  let finalContent: string;
  if (preExisting && input.append) {
    const existing = readFileSync(invariantFilePath, "utf8");
    // Append: simple concatenation with a separator. The framework's
    // collector pattern handles multiple `must()` calls in one file.
    finalContent = existing.replace(/\n*$/, "\n\n") + input.surfaceText;
  } else {
    finalContent = input.surfaceText;
  }

  writeFileSync(invariantFilePath, finalContent);

  return {
    invariantFilePath,
    contentHash: createHash("sha256").update(finalContent).digest("hex"),
    bytesWritten: Buffer.byteLength(finalContent, "utf8"),
    preExisting,
  };
}
