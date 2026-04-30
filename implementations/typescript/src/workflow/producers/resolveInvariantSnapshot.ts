/**
 * resolveInvariantSnapshot Stage — gather every invariant's CID + body
 * at a given git ref (or the working tree).
 *
 * The diff workflow's input primitive. Two snapshots, taken at "from"
 * and "to", are set-differenced to produce the forensic punch list.
 *
 * Ref shapes:
 *   - "WORKING_TREE" — read the live .provekit/invariants/ directory
 *   - any other string — treated as a git ref; we run
 *     `git ls-tree <ref> .provekit/invariants/` to list the files at
 *     that commit, then `git show <ref>:<path>` to fetch each body
 *
 * Pure given (projectRoot, ref). Cache key is (projectRoot, ref); the
 * runner can skip re-doing the work for unchanged refs.
 *
 * No LLM. Pure git plumbing + file IO.
 */

import { execFileSync } from "child_process";
import { existsSync, readFileSync, readdirSync } from "fs";
import { join } from "path";
import type { Stage } from "../types.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";

export const RESOLVE_INVARIANT_SNAPSHOT_CAPABILITY = "resolve-invariant-snapshot";

export interface ResolveInvariantSnapshotInput {
  projectRoot: string;
  /** Git ref (e.g. "HEAD", "main", "abc123") or the literal "WORKING_TREE". */
  ref: string;
}

export interface InvariantSnapshotEntry {
  id: string;
  invariant: StoredInvariant;
}

export interface ResolveInvariantSnapshotOutput {
  ref: string;
  /** Each invariant present at this ref. Keyed-by-array preserves stable iteration. */
  entries: InvariantSnapshotEntry[];
  /** Quick-lookup index built from entries; populated for caller convenience. */
  byId: Record<string, StoredInvariant>;
}

export interface MakeResolveInvariantSnapshotStageDeps {
  producerVersion?: string;
}

export function makeResolveInvariantSnapshotStage(
  deps: MakeResolveInvariantSnapshotStageDeps = {},
): Stage<ResolveInvariantSnapshotInput, ResolveInvariantSnapshotOutput> {
  const producedBy = deps.producerVersion ?? "resolveInvariantSnapshot@v1";

  return {
    name: "resolveInvariantSnapshot",
    producedBy,

    serializeInput(input) {
      return { projectRoot: input.projectRoot, ref: input.ref };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ResolveInvariantSnapshotOutput;
    },

    async run(input) {
      const entries = input.ref === "WORKING_TREE"
        ? readWorkingTreeSnapshot(input.projectRoot)
        : readGitRefSnapshot(input.projectRoot, input.ref);
      const byId: Record<string, StoredInvariant> = {};
      for (const e of entries) byId[e.id] = e.invariant;
      return { ref: input.ref, entries, byId };
    },
  };
}

function readWorkingTreeSnapshot(projectRoot: string): InvariantSnapshotEntry[] {
  const dir = join(projectRoot, ".provekit", "invariants");
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((n) => n.endsWith(".json"))
    .map((name) => {
      const id = name.replace(/\.json$/, "");
      const invariant = JSON.parse(readFileSync(join(dir, name), "utf-8")) as StoredInvariant;
      return { id, invariant };
    });
}

function readGitRefSnapshot(projectRoot: string, ref: string): InvariantSnapshotEntry[] {
  // List files in .provekit/invariants/ at the ref. ls-tree's -r recurses;
  // --name-only emits paths only.
  let listing: string;
  try {
    listing = execFileSync(
      "git",
      ["ls-tree", "-r", "--name-only", ref, ".provekit/invariants/"],
      { cwd: projectRoot, encoding: "utf-8", stdio: ["ignore", "pipe", "pipe"] },
    );
  } catch {
    // Ref doesn't exist OR .provekit/invariants/ wasn't tracked at that
    // ref. Either way, the snapshot is empty.
    return [];
  }
  const entries: InvariantSnapshotEntry[] = [];
  for (const path of listing.split("\n")) {
    const trimmed = path.trim();
    if (!trimmed.endsWith(".json")) continue;
    const id = trimmed.split("/").pop()!.replace(/\.json$/, "");
    let body: string;
    try {
      body = execFileSync(
        "git",
        ["show", `${ref}:${trimmed}`],
        { cwd: projectRoot, encoding: "utf-8", stdio: ["ignore", "pipe", "pipe"] },
      );
    } catch {
      // File listed but unreadable at that ref. Skip; the diff will see
      // it as missing in both snapshots which is the right answer.
      continue;
    }
    try {
      entries.push({ id, invariant: JSON.parse(body) as StoredInvariant });
    } catch {
      // Malformed JSON at this ref; skip rather than crash the snapshot.
      continue;
    }
  }
  return entries;
}
