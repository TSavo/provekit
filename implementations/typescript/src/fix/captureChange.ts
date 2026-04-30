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
import { createNoopLogger } from "./logger.js";
import type { FixLoopLogger } from "./logger.js";

/**
 * Run an LLM agent in the overlay's worktree. After the agent returns,
 * capture the change as a CodePatch by reading the git diff + new files.
 */
export async function runAgentInOverlay(args: {
  overlay: OverlayHandle;
  llm: LLMProvider;
  prompt: string;
  stage?: string;
  allowedTools?: string[];
  model?: AgentRequestOptions["model"];
  maxTurns?: number;
  logger?: FixLoopLogger;
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

  const logger = args.logger ?? createNoopLogger();
  const stageName = args.stage ?? "agent";
  const cwd = args.overlay.worktreePath;

  // No artificial caps: the LLM gets all tools and as many turns as it
  // needs. The claude-agent-sdk has a default of 20 turns which cuts off
  // legitimate exploration (we observed C3 successfully navigating from a
  // wrong investigateReport locus to the real code path at turn 24, then
  // dying). 1000 is effectively no cap — any realistic stage finishes well
  // before it. The agent's output contract is enforced by the parsed result
  // (toolUses + final patch), not by clamping the turn budget.
  const result = await args.llm.agent(args.prompt, {
    cwd,
    allowedTools: args.allowedTools ?? [".*"],
    model: args.model,
    maxTurns: args.maxTurns ?? 1000,
  });

  // Emit structured log events for every block — full payloads, no truncation.
  for (const tu of result.toolUses) {
    logger.toolUse(stageName, tu.name, tu.input);
    if (tu.result !== undefined) {
      logger.toolResult(stageName, tu.id, tu.result);
    }
  }
  for (const tb of result.thinkingBlocks) {
    logger.thinking(stageName, tb.content);
  }
  for (const txt of result.textBlocks) {
    logger.response(stageName, "claude-agent", txt.content);
  }

  // No agent-behavior gating. The agent has bypassPermissions and full
  // tool access by explicit user directive. The mechanical gates that
  // matter live downstream: Z3 for invariant satisfiability, mutation
  // verification for the regression test, full-suite parity for the
  // bundle. Heuristic path-policing here just creates false negatives
  // when the agent picks a hallucinated path, gets ENOENT, and moves on.
  //
  // The overlay is a throwaway git worktree — anything the agent writes
  // OUTSIDE it lands in a real path under the user's home only if the
  // agent picks that exact path AND the user's filesystem layout matches.
  // Even then, the user has explicitly opted into bypassPermissions for
  // this run; that's a policy decision at session level, not a per-tool
  // veto.
  //
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
