/**
 * captureChange.ts — helpers for running an LLM agent in an overlay worktree
 * and reconstructing a CodePatch from the git diff after it completes.
 *
 * Used by the C3 agent path in candidateGen.ts.
 */

import { readFileSync } from "fs";
import { join } from "path";
import { execFileSync } from "child_process";
import type { OverlayHandle, CodePatch, LLMProvider, AgentRequestOptions } from "./types.js";

/**
 * Run an LLM agent in the overlay's worktree. After the agent returns,
 * capture the change as a CodePatch by reading the git diff + new files.
 */
export async function runAgentInOverlay(args: {
  overlay: OverlayHandle;
  llm: LLMProvider;
  prompt: string;
  allowedTools?: string[];
  model?: AgentRequestOptions["model"];
  maxTurns?: number;
}): Promise<{
  patch: CodePatch;
  rationale: string;
  turnsUsed: number;
}> {
  if (!args.llm.agent) {
    throw new Error(
      "runAgentInOverlay: LLM provider does not implement agent() — use JSON-patch path instead",
    );
  }

  const cwd = args.overlay.worktreePath;

  const result = await args.llm.agent(args.prompt, {
    cwd,
    allowedTools: args.allowedTools ?? ["Read", "Edit", "Write", "Bash", "Glob", "Grep"],
    model: args.model,
    maxTurns: args.maxTurns ?? 20,
  });

  // Reconstruct CodePatch: modified tracked files from git diff + new untracked files.
  // Exclude .provekit/ — it contains the scratch SAST DB which must not be overwritten.
  const isOverlayInternal = (f: string) => f.startsWith(".provekit/") || f === ".provekit";
  const modifiedFiles = getChangedFiles(cwd).filter((f) => !isOverlayInternal(f));
  const newFiles = getUntrackedFiles(cwd).filter((f) => !isOverlayInternal(f));
  const allFiles = [...new Set([...modifiedFiles, ...newFiles])];

  const patch = reconstructCodePatch(args.overlay, allFiles);
  return {
    patch,
    rationale: result.text,
    turnsUsed: result.turnsUsed,
  };
}

/**
 * Build a CodePatch from the list of files that changed in the overlay.
 * Reads the current (post-agent) content of each file.
 */
function reconstructCodePatch(overlay: OverlayHandle, filesChanged: string[]): CodePatch {
  const fileEdits = filesChanged.map((file) => ({
    file,
    newContent: (() => {
      try {
        return readFileSync(join(overlay.worktreePath, file), "utf-8");
      } catch {
        // File was deleted or unreadable — return empty string.
        return "";
      }
    })(),
  }));

  return {
    fileEdits,
    description: `captured via agent (${filesChanged.length} file${filesChanged.length === 1 ? "" : "s"} changed)`,
  };
}

/**
 * Get tracked files that have been modified in the worktree (git diff --name-only).
 */
export function getChangedFiles(cwd: string): string[] {
  try {
    return execFileSync("git", ["diff", "--name-only"], { cwd, encoding: "utf-8" })
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

/**
 * Get untracked files the agent may have created (git ls-files --others --exclude-standard).
 * Filters out .gitignored paths.
 */
export function getUntrackedFiles(cwd: string): string[] {
  try {
    return execFileSync(
      "git",
      ["ls-files", "--others", "--exclude-standard"],
      { cwd, encoding: "utf-8" },
    )
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}
