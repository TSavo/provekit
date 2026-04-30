/**
 * locateCurrentFunction Stage — find the function-shaped substrate node
 * currently associated with an invariant's callsite, regardless of
 * whether the function has moved or been edited.
 *
 * Inputs an already-loaded StoredInvariant (so this stage doesn't reach
 * back into disk for the invariant; the previous Stage in the workflow
 * does that). Walks the substrate via two strategies, in order:
 *
 *   1. By stored functionHash — the cleanest match. Returns the function
 *      AT ITS CURRENT LINE plus its current body.
 *   2. By stored function NAME (callsite.function) — fallback when no
 *      hash was recorded (legacy invariants) OR the hash decayed (case
 *      3: function was edited). Returns the function with the matching
 *      name in the same file at any line. The body's content hash will
 *      differ from the recorded hash if it's case 3.
 *
 * Output reports which strategy matched (`matchedBy`) and surfaces the
 * function's current location, body, and content hash. Downstream
 * stages use that to decide whether the binding self-heals (case 2:
 * matchedBy === "hash") or needs LLM re-evaluation (case 3: matchedBy
 * === "name", currentHash !== invariant.callsite.functionHash).
 *
 * Pure given (projectRoot, invariantId-derived inputs). Substrate state
 * IS an input the runner hashes via the substrate fingerprint.
 */

import { existsSync, readFileSync } from "fs";
import type { Stage } from "../types.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import {
  openSubstrateDb,
  findFunctionLineByHash,
  findFunctionByHashGlobal,
} from "../../fix/runtime/substrate.js";
import { files, nodes } from "../../sast/schema/nodes.js";
import { eq } from "drizzle-orm";

export const LOCATE_CURRENT_FUNCTION_CAPABILITY = "locate-current-function";

export interface LocateCurrentFunctionInput {
  invariant: StoredInvariant;
  projectRoot: string;
}

export interface LocateCurrentFunctionOutput {
  /**
   * Which strategy matched the function.
   *   "hash-same-file"   — recorded hash found at recorded file (case 1+2)
   *   "hash-other-file"  — recorded hash found in a different file (case 3,
   *                        cross-file move; binding self-heals across move)
   *   "name-same-file"   — hash didn't match anywhere; first function in
   *                        the recorded file is returned as best-effort
   *                        (legacy fallback when no hash was recorded)
   *   "fallback-bytes"   — hash gone globally; we returned the bytes of
   *                        whatever is currently at the recorded file
   *                        path so the LLM can judge the new code
   *   "none"             — file is gone entirely, nothing to surface
   */
  matchedBy: "hash-same-file" | "hash-other-file" | "name-same-file" | "fallback-bytes" | "none";
  /** When matched, the file the function CURRENTLY lives in (may differ from recorded). */
  currentFilePath: string | null;
  /** When matched, the function's current sourceLine; null if no match. */
  currentLine: number | null;
  /** When matched, the bytes of the function body; null if no match. */
  currentBody: string | null;
  /** When matched, the substrate's recorded subtreeHash for the current body. */
  currentHash: string | null;
  /** When matched, the function's name (echoed for downstream convenience). */
  functionName: string | null;
  /**
   * True when the recorded functionHash exists somewhere in the substrate
   * (same file or cross-file). Distinguishes case 1+2+3 (binding still
   * anchored mechanically) from case 4 (LLM re-eval required).
   */
  hashStillResolves: boolean;
}

export interface MakeLocateCurrentFunctionStageDeps {
  producerVersion?: string;
}

function readBody(
  db: ReturnType<typeof openSubstrateDb>,
  filePath: string,
  functionHash: string,
  sourceLine: number,
  fnName: string | null,
  matchedBy: "hash-same-file" | "hash-other-file",
  hashStillResolves: boolean,
): LocateCurrentFunctionOutput {
  if (!db) {
    return {
      matchedBy: "none",
      currentFilePath: null,
      currentLine: null,
      currentBody: null,
      currentHash: null,
      functionName: fnName,
      hashStillResolves: false,
    };
  }
  const fileRow = db.select({ id: files.id }).from(files).where(eq(files.path, filePath)).get();
  if (!fileRow || !existsSync(filePath)) {
    return {
      matchedBy,
      currentFilePath: filePath,
      currentLine: sourceLine,
      currentBody: null,
      currentHash: functionHash,
      functionName: fnName,
      hashStillResolves,
    };
  }
  const fnNode = db
    .select({ sourceStart: nodes.sourceStart, sourceEnd: nodes.sourceEnd, subtreeHash: nodes.subtreeHash })
    .from(nodes)
    .where(eq(nodes.fileId, fileRow.id))
    .all()
    .find((n) => n.subtreeHash === functionHash);
  if (!fnNode) {
    return {
      matchedBy,
      currentFilePath: filePath,
      currentLine: sourceLine,
      currentBody: null,
      currentHash: functionHash,
      functionName: fnName,
      hashStillResolves,
    };
  }
  const bytes = readFileSync(filePath, "utf-8").slice(fnNode.sourceStart, fnNode.sourceEnd);
  return {
    matchedBy,
    currentFilePath: filePath,
    currentLine: sourceLine,
    currentBody: bytes,
    currentHash: fnNode.subtreeHash,
    functionName: fnName,
    hashStillResolves,
  };
}

export function makeLocateCurrentFunctionStage(
  deps: MakeLocateCurrentFunctionStageDeps = {},
): Stage<LocateCurrentFunctionInput, LocateCurrentFunctionOutput> {
  const producedBy = deps.producerVersion ?? "locateCurrentFunction@v1";

  return {
    name: "locateCurrentFunction",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        invariantId: input.invariant.id,
        callsite: input.invariant.callsite,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LocateCurrentFunctionOutput;
    },

    async run(input) {
      const { filePath, function: fnName, functionHash } = input.invariant.callsite;
      const db = openSubstrateDb(input.projectRoot);
      const noMatch = (matchedBy: LocateCurrentFunctionOutput["matchedBy"]): LocateCurrentFunctionOutput => ({
        matchedBy,
        currentFilePath: null,
        currentLine: null,
        currentBody: null,
        currentHash: null,
        functionName: fnName ?? null,
        hashStillResolves: false,
      });

      if (!db) return noMatch("none");

      // Strategy 1: hash in the same file (cases 1, 2 — same-file recovery).
      if (functionHash) {
        const lineByHash = findFunctionLineByHash(db, filePath, functionHash);
        if (lineByHash !== null) {
          return readBody(db, filePath, functionHash, lineByHash, fnName, "hash-same-file", true);
        }

        // Strategy 2: hash anywhere in the substrate (case 3 — cross-file move).
        const moved = findFunctionByHashGlobal(db, functionHash);
        if (moved) {
          return readBody(db, moved.filePath, functionHash, moved.sourceLine, fnName, "hash-other-file", true);
        }
      }

      // Strategy 3: fall back to function name in the same file (legacy
      // line-only invariants, or hash gone but a function with the same
      // name still lives at the original file).
      if (fnName) {
        const fileRow = db.select({ id: files.id }).from(files).where(eq(files.path, filePath)).get();
        if (fileRow) {
          const namedNodes = db
            .select({ id: nodes.id, sourceLine: nodes.sourceLine, subtreeHash: nodes.subtreeHash, kind: nodes.kind, sourceStart: nodes.sourceStart, sourceEnd: nodes.sourceEnd })
            .from(nodes)
            .where(eq(nodes.fileId, fileRow.id))
            .all();
          for (const n of namedNodes) {
            if (n.kind === "FunctionDeclaration" || n.kind === "MethodDefinition") {
              if (existsSync(filePath)) {
                const bytes = readFileSync(filePath, "utf-8").slice(n.sourceStart, n.sourceEnd);
                return {
                  matchedBy: "name-same-file",
                  currentFilePath: filePath,
                  currentLine: n.sourceLine,
                  currentBody: bytes,
                  currentHash: n.subtreeHash,
                  functionName: fnName,
                  hashStillResolves: false,
                };
              }
            }
          }
        }
      }

      // Strategy 4 (case 4 fallback): hash gone globally and no name match.
      // Surface the file's current bytes so the LLM stage can judge what
      // now lives where the original function was. This is the "exactly
      // enough information" property: the framework hands the LLM the
      // original claim AND the current code so it can decide.
      if (existsSync(filePath)) {
        const bytes = readFileSync(filePath, "utf-8");
        return {
          matchedBy: "fallback-bytes",
          currentFilePath: filePath,
          currentLine: null,
          currentBody: bytes,
          currentHash: null,
          functionName: fnName ?? null,
          hashStillResolves: false,
        };
      }

      // File is gone too: definitively retire.
      return noMatch("none");
    },
  };
}
