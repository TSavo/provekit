/**
 * captureChange.ts — helpers for running an LLM agent in an overlay worktree
 * and reconstructing a CodePatch from the git diff after it completes.
 *
 * Used by the C3 agent path in candidateGen.ts.
 */

import { readFileSync } from "fs";
import { join, relative } from "path";
import { realpathSync } from "fs";
import { execFileSync } from "child_process";
import type { OverlayHandle, CodePatch, LLMProvider, AgentRequestOptions } from "./types.js";
import { OverlayBypassError } from "./types.js";
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

  // -------------------------------------------------------------------------
  // Layer 2: post-agent path enforcement.
  //
  // Inspect every tool use to detect file accesses outside the overlay.
  // Edit/Write/Read are hard-fail: any absolute path outside the overlay root
  // is a confirmed bypass. Bash is warn-and-log: false-positive rate is too
  // high to hard-fail (the overlay itself lives under /var/folders on macOS,
  // /usr/bin/node is legit, etc.). The dogfood proof was an Edit, not a Bash,
  // so this threshold is correct.
  //
  // NOTE: This is detection, not prevention. By the time we get here the agent
  // has already run. Throwing stops the patch from being recorded but does NOT
  // undo any filesystem mutations. Callers should treat a bypass as a poisoned
  // overlay and close it.
  // -------------------------------------------------------------------------
  const overlayRootReal = (() => {
    try { return realpathSync(cwd); } catch { return cwd; }
  })();

  // Two-pass: collect all path-bearing tool uses first, then decide which
  // bypasses to throw on. Self-correction is common — the agent hallucinates
  // an absolute path like `/home/user/.provekit/foo`, sees the failure (or
  // its own pwd), then re-writes to the real overlay path. The first pass
  // finds bypass events; the second pass tolerates Write bypasses if the
  // same relative tail was also written inside the overlay (the agent
  // self-corrected). Read/Edit bypasses still throw — those touch real
  // files outside the overlay and indicate genuine confinement failure.
  type ToolPath = { tool: string; rawPath: string; overlayRel: string | null };
  const bypassEvents: ToolPath[] = [];
  const inOverlayPaths = new Set<string>();

  for (const tu of result.toolUses) {
    const inp = (tu.input ?? {}) as Record<string, unknown>;

    if (tu.name === "Edit" || tu.name === "Write" || tu.name === "Read") {
      const rawPath = typeof inp["file_path"] === "string" ? inp["file_path"] : null;
      if (!rawPath) continue;

      if (rawPath.startsWith("/")) {
        const realRaw = (() => { try { return realpathSync(rawPath); } catch { return rawPath; } })();
        const rel = relative(overlayRootReal, realRaw);
        if (rel.startsWith("..")) {
          // Find the .provekit/-rooted suffix if present (the canonical agent
          // output shape). We compare on this suffix to detect self-correction.
          const provekitMatch = rawPath.match(/(\.provekit\/.+)$/);
          const overlayRel = provekitMatch ? provekitMatch[1]! : null;
          bypassEvents.push({ tool: tu.name, rawPath, overlayRel });
          continue;
        }
        // Path normalized to inside the overlay — record its in-overlay tail.
        const provekitMatch = rawPath.match(/(\.provekit\/.+)$/);
        if (provekitMatch) inOverlayPaths.add(provekitMatch[1]!);
      } else {
        // Relative path — agent kept things in the overlay's cwd. Record.
        const provekitMatch = rawPath.match(/(\.provekit\/.+)$/);
        if (provekitMatch) inOverlayPaths.add(provekitMatch[1]!);
      }
    } else if (tu.name === "Bash") {
      const cmd = typeof inp["command"] === "string" ? inp["command"] : null;
      if (cmd) {
        // Extract absolute-path tokens from the command.
        const absTokens = cmd.match(/(?<!['"\/\w])\/[^\s'";\|&>]+/g) ?? [];
        for (const token of absTokens) {
          const realToken = (() => { try { return realpathSync(token); } catch { return token; } })();
          const rel = relative(overlayRootReal, realToken);
          if (rel.startsWith("..")) {
            // Warn and log — do not hard-fail Bash (too many false positives).
            logger.error(
              `overlay-bypass-warn: Bash command references path outside overlay (not failing hard — Bash has high false-positive rate)`,
              { tool: "Bash", path: token, command: cmd, overlayRoot: cwd, stage: stageName },
            );
            // No throw for Bash — see comment above.
          }
        }
      }
    }
  }

  // Second pass: decide which bypass events warrant a throw.
  for (const ev of bypassEvents) {
    const selfCorrected =
      ev.tool === "Write" &&
      ev.overlayRel !== null &&
      inOverlayPaths.has(ev.overlayRel);

    if (selfCorrected) {
      // Agent hallucinated an absolute path then re-wrote to the correct
      // overlay path. Tolerate: log a warning, keep going. Common with
      // open toolsets where the agent picks /home/user/... before pwd.
      logger.error(
        `overlay-bypass-warn: ${ev.tool} on ${ev.rawPath} but same relative tail was also written inside overlay (self-corrected)`,
        { tool: ev.tool, path: ev.rawPath, overlayRoot: cwd, stage: stageName },
      );
      continue;
    }

    // Hard bypass: Read/Edit always throws (those touch real existing
    // files outside the overlay), or Write with no in-overlay counterpart.
    logger.error(
      `overlay-bypass: ${ev.tool} on path outside overlay`,
      { tool: ev.tool, path: ev.rawPath, overlayRoot: cwd, stage: stageName },
    );
    throw new OverlayBypassError(ev.tool, ev.rawPath, cwd);
  }

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
