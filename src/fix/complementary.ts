/**
 * C4: Complementary-change helpers.
 *
 * Exposes three building blocks consumed by generateComplementary (stages/generateComplementary.ts):
 *   1. discoverComplementarySites — SAST queries to find candidate sites
 *   2. proposeChangeForSite      — LLM call per site to propose a patch
 *   3. verifySiteChange          — oracle #3: Z3 re-verification under the overlay
 *
 * Discovery strategies (in priority order):
 *   A. principle_match  → adjacent_site_fix    (same principle fires at another node)
 *   B. calls_table      → caller_update        (callers of the fixed function)
 *   C. data_flow_table  → data_flow_guard      (upstream nodes feeding the bug site)
 *   D. llm_reflection   → observability | startup_assert  (SKIPPED in MVP — too speculative)
 *
 * Oracle #3 runs PER SITE before a change is accepted. Only changes with
 * verifiedAgainstOverlay: true are returned. Rejected changes are rolled back
 * (file restored + overlay re-indexed) to prevent poisoning subsequent sites.
 *
 * Overlay state is CUMULATIVE: accepted patches from earlier sites are visible to
 * later site verifications.
 */

import { readFileSync, writeFileSync, existsSync, realpathSync } from "fs"; // realpathSync used in computeRelPath
import { join, relative, dirname } from "path";
import { execFileSync } from "child_process";
import { eq } from "drizzle-orm";
import { runAgentInOverlay, getChangedFiles, getUntrackedFiles } from "./captureChange.js";
import { principleMatches, principleMatchCaptures } from "../db/schema/principleMatches.js";
import { nodeCalls } from "../sast/schema/capabilities/calls.js";
import { nodeBinding } from "../sast/schema/capabilities/index.js";
import { dataFlow } from "../sast/schema/dataFlow.js";
import { nodes, files as filesTable } from "../sast/schema/nodes.js";
import { evaluatePrinciple } from "../dsl/evaluator.js";
import { buildSASTForFile, reindexFile } from "../sast/builder.js";
import { applyPatchToOverlay, reindexOverlay } from "./overlay.js";
import { parseProposedFixes } from "./candidateGen.js";
import type { Db } from "../db/index.js";
import type {
  BugLocus,
  FixCandidate,
  InvariantClaim,
  LLMProvider,
  OverlayHandle,
  CodePatch,
  ComplementarySiteKind,
} from "./types.js";

// ---------------------------------------------------------------------------
// Internal: ComplementarySite (pre-LLM candidate)
// ---------------------------------------------------------------------------

export interface ComplementarySite {
  kind: ComplementarySiteKind;
  nodeId: string;
  fileId: number;
  /** Absolute path to the file containing this site (from main DB). */
  filePath: string;
  /**
   * Path of the file relative to the repository root (= same relative path
   * in the overlay worktree). Set during discovery from the locus's repo root.
   * If null, oracle #3 falls back to content-based checks.
   */
  fileRelPath: string | null;
  discoveredVia: "principle_match" | "calls_table" | "data_flow_table" | "llm_reflection";
  reason: string; // passed to LLM as context
}

// ---------------------------------------------------------------------------
// Priority ordering
// ---------------------------------------------------------------------------

export function priorityOf(kind: ComplementarySiteKind): number {
  switch (kind) {
    case "caller_update":    return 0;
    case "adjacent_site_fix": return 1;
    case "data_flow_guard":  return 2;
    case "observability":    return 3;
    case "startup_assert":   return 4;
  }
}

// ---------------------------------------------------------------------------
// Helper: resolve file path from fileId in main DB
// ---------------------------------------------------------------------------

function resolveFilePath(db: Db, fileId: number): string | null {
  const row = db
    .select({ path: filesTable.path })
    .from(filesTable)
    .where(eq(filesTable.id, fileId))
    .get();
  return row?.path ?? null;
}

// ---------------------------------------------------------------------------
// Helper: ensure a file is indexed in the overlay's scratch DB.
// relPath is the file path relative to the repo root (same in both
// the original repo and the overlay worktree).
// ---------------------------------------------------------------------------

function ensureFileIndexedInOverlay(overlay: OverlayHandle, relPath: string | null): void {
  if (!relPath) return;

  // Use the raw (non-realpath) overlay file path to match what reindexOverlay uses.
  // Both ensureFileIndexedInOverlay and reindexOverlay must use the same path
  // representation so the DB lookup succeeds.
  const overlayFilePath = join(overlay.worktreePath, relPath);
  if (!existsSync(overlayFilePath)) return;

  // Build SAST for this file in the overlay's scratch DB.
  // Use buildSASTForFile (not reindexFile) to avoid deleting if already present.
  // If the file is already indexed with matching content hash, this is a no-op.
  try {
    buildSASTForFile(overlay.sastDb, overlayFilePath);
  } catch {
    // Ignore — best effort. The file may not be parseable.
  }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/**
 * Discover candidate complementary sites via SAST queries.
 *
 * Runs three strategies (A, B, C). Dedupes by nodeId (highest-priority kind
 * wins). Caps total at maxSites.
 *
 * Strategy D (llm_reflection) is intentionally skipped in MVP — no concrete
 * grounding for observability/startup_assert without a principle anchor.
 */
export async function discoverComplementarySites(args: {
  fix: FixCandidate;
  locus: BugLocus;
  db: Db;
  maxSites: number;
  invariant?: InvariantClaim;
}): Promise<ComplementarySite[]> {
  const { locus, db, maxSites, invariant } = args;

  // Determine the repo root from the locus file so we can compute relative paths
  // that are valid in both the main repo and the overlay worktree.
  let repoRoot: string | null = null;
  try {
    const locusDir = dirname(locus.file);
    repoRoot = execFileSync(
      "git",
      ["rev-parse", "--show-toplevel"],
      { cwd: locusDir, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
  } catch {
    repoRoot = null;
  }

  // Resolve the repo root symlinks for reliable relative path computation.
  // On macOS, /var is a symlink to /private/var; realpathSync normalizes both.
  const repoRootReal = repoRoot ? (() => {
    try { return realpathSync(repoRoot); } catch { return repoRoot; }
  })() : null;

  const computeRelPath = (absFilePath: string): string | null => {
    if (!repoRootReal) return null;
    try {
      const absReal = (() => {
        try { return realpathSync(absFilePath); } catch { return absFilePath; }
      })();
      const rel = relative(repoRootReal, absReal);
      if (rel.startsWith("..")) return null;
      return rel;
    } catch {
      return null;
    }
  };

  // Collect from each strategy, then dedupe + sort + cap.
  const byNodeId = new Map<string, ComplementarySite>();

  // Helper: add site if not already seen (first insertion wins — priority is applied after).
  const addSite = (site: ComplementarySite): void => {
    if (!byNodeId.has(site.nodeId)) {
      byNodeId.set(site.nodeId, site);
    } else {
      // Keep higher-priority kind.
      const existing = byNodeId.get(site.nodeId)!;
      if (priorityOf(site.kind) < priorityOf(existing.kind)) {
        byNodeId.set(site.nodeId, site);
      }
    }
  };

  // -------------------------------------------------------------------------
  // Strategy A: principle_match → adjacent_site_fix
  // Requires invariant.principleId to know which principle to search.
  // -------------------------------------------------------------------------
  if (invariant?.principleId) {
    try {
      const principleId = invariant.principleId;
      const allMatches = db
        .select({
          rootMatchNodeId: principleMatches.rootMatchNodeId,
          fileId: principleMatches.fileId,
          message: principleMatches.message,
        })
        .from(principleMatches)
        .where(eq(principleMatches.principleName, principleId))
        .all();

      for (const m of allMatches) {
        if (m.rootMatchNodeId === locus.primaryNode) continue;
        if (m.rootMatchNodeId === locus.containingFunction) continue;

        const filePath = resolveFilePath(db, m.fileId);
        if (!filePath) continue;

        addSite({
          kind: "adjacent_site_fix",
          nodeId: m.rootMatchNodeId,
          fileId: m.fileId,
          filePath,
          fileRelPath: computeRelPath(filePath),
          discoveredVia: "principle_match",
          reason: `Principle '${principleId}' also fires at ${filePath}: ${m.message}`,
        });
      }
    } catch {
      // Ignore DB errors — proceed with other strategies.
    }
  }

  // -------------------------------------------------------------------------
  // Strategy B: calls_table → caller_update
  // Find callers of the function that contains the bug site.
  // -------------------------------------------------------------------------
  try {
    const containingFnId = locus.containingFunction;

    // Get the function's binding name (if available).
    const bindingRow = db
      .select({ name: nodeBinding.name })
      .from(nodeBinding)
      .where(eq(nodeBinding.nodeId, containingFnId))
      .get();

    // Query node_calls for rows whose callee_node or callee_name matches.
    const callersByNode = db
      .select({
        nodeId: nodeCalls.nodeId,
      })
      .from(nodeCalls)
      .where(eq(nodeCalls.calleeNode, containingFnId))
      .all();

    const callersByName = bindingRow
      ? db
          .select({
            nodeId: nodeCalls.nodeId,
          })
          .from(nodeCalls)
          .where(eq(nodeCalls.calleeName, bindingRow.name))
          .all()
      : [];

    const allCallerNodeIds = new Set<string>();
    for (const r of [...callersByNode, ...callersByName]) {
      allCallerNodeIds.add(r.nodeId);
    }

    // For each call-site node, get its fileId + path.
    for (const callerNodeId of allCallerNodeIds) {
      const nodeRow = db
        .select({ fileId: nodes.fileId })
        .from(nodes)
        .where(eq(nodes.id, callerNodeId))
        .get();
      if (!nodeRow) continue;

      const filePath = resolveFilePath(db, nodeRow.fileId);
      if (!filePath) continue;

      addSite({
        kind: "caller_update",
        nodeId: callerNodeId,
        fileId: nodeRow.fileId,
        filePath,
        fileRelPath: computeRelPath(filePath),
        discoveredVia: "calls_table",
        reason: `Call site in ${filePath} calls fixed function '${bindingRow?.name ?? containingFnId}' and may need error handling.`,
      });
    }
  } catch {
    // Ignore DB errors.
  }

  // -------------------------------------------------------------------------
  // Strategy C: data_flow_table → data_flow_guard
  // Find upstream nodes (from_node) that flow into the bug's primaryNode.
  // -------------------------------------------------------------------------
  try {
    const primaryNodeId = locus.primaryNode;

    const upstreamRows = db
      .select({ fromNode: dataFlow.fromNode })
      .from(dataFlow)
      .where(eq(dataFlow.toNode, primaryNodeId))
      .all();

    for (const r of upstreamRows) {
      if (r.fromNode === primaryNodeId) continue;
      if (r.fromNode === locus.containingFunction) continue;

      const nodeRow = db
        .select({ fileId: nodes.fileId })
        .from(nodes)
        .where(eq(nodes.id, r.fromNode))
        .get();
      if (!nodeRow) continue;

      const filePath = resolveFilePath(db, nodeRow.fileId);
      if (!filePath) continue;

      addSite({
        kind: "data_flow_guard",
        nodeId: r.fromNode,
        fileId: nodeRow.fileId,
        filePath,
        fileRelPath: computeRelPath(filePath),
        discoveredVia: "data_flow_table",
        reason: `Node in ${filePath} feeds data into the bug site (${primaryNodeId}) without a validation guard.`,
      });
    }
  } catch {
    // Ignore DB errors.
  }

  // Strategy D (llm_reflection) is intentionally skipped in MVP.

  // -------------------------------------------------------------------------
  // Sort by priority and cap at maxSites.
  // -------------------------------------------------------------------------
  const sorted = [...byNodeId.values()].sort(
    (a, b) => priorityOf(a.kind) - priorityOf(b.kind),
  );

  return sorted.slice(0, maxSites);
}

// ---------------------------------------------------------------------------
// Propose
// ---------------------------------------------------------------------------

export interface ProposedSiteChange {
  patch: CodePatch;
  rationale: string;
}

/**
 * Ask the LLM to propose a complementary change for the given site.
 *
 * Returns null if the LLM explicitly declines (empty patch or explicit skip).
 * Throws on parse errors.
 */
export async function proposeChangeForSite(
  site: ComplementarySite,
  fix: FixCandidate,
  locus: BugLocus,
  invariant: InvariantClaim | undefined,
  llm: LLMProvider,
): Promise<ProposedSiteChange | null> {
  // Read source context from the site's file for the prompt.
  let siteSourceContext = "(source not available)";
  try {
    if (existsSync(site.filePath)) {
      siteSourceContext = readFileSync(site.filePath, "utf-8");
    }
  } catch {
    // ignore — use reason as fallback context
    siteSourceContext = site.reason;
  }

  const prompt = `You are a code-repair expert. A bug was just fixed at one site in a codebase.
Your task is to propose a complementary change at an adjacent site that prevents the same bug class.

== Primary fix ==
Fixed at node: ${locus.primaryNode} (containing function: ${locus.containingFunction})
File: ${locus.file}:${locus.line}
Patch applied:
${JSON.stringify(fix.patch, null, 2)}

== Adjacent site ==
Site kind: ${site.kind}
Site node ID: ${site.nodeId}
File: ${site.filePath}
Discovery reason: ${site.reason}
Source:
\`\`\`
${siteSourceContext.slice(0, 2000)}
\`\`\`
${invariant ? `\nInvariant violated at primary: ${invariant.description}` : ""}

Propose a complementary CodePatch for the adjacent site (file: ${site.filePath}).
If no change is needed at this site, respond with:
{"skip": true, "reason": "..."}

Otherwise respond with ONLY a JSON object (no markdown fences, no extra text):
{
  "candidates": [
    {
      "rationale": "one sentence: why this patch helps at this adjacent site",
      "confidence": 0.8,
      "patch": {
        "description": "what the patch does",
        "fileEdits": [
          {
            "file": "relative/path/to/file.ts",
            "newContent": "FULL file contents after the patch"
          }
        ]
      }
    }
  ]
}

Rules:
- Each candidate must have rationale (string), confidence (0..1 number), and patch.
- patch.fileEdits is an array; each entry has file (relative path) and newContent (full file content).
- Rank candidates by confidence descending.
- Do NOT output anything outside the JSON object or skip response.`;

  const rawResponse = await llm.complete({ prompt });

  // Check for explicit skip.
  let cleaned = rawResponse.trim();
  if (cleaned.startsWith("```")) {
    cleaned = cleaned.replace(/^```[a-z]*\n?/, "").replace(/```\s*$/, "").trim();
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(cleaned);
  } catch {
    throw new Error(
      `proposeChangeForSite: LLM response is not valid JSON: ${cleaned.slice(0, 200)}`,
    );
  }

  if (
    typeof parsed === "object" &&
    parsed !== null &&
    (parsed as Record<string, unknown>)["skip"] === true
  ) {
    return null;
  }

  // Parse as candidates array (reuse parseProposedFixes logic).
  const fixes = parseProposedFixes(rawResponse);
  if (fixes.length === 0) return null;

  // Take the highest-confidence candidate.
  const best = fixes.sort((a, b) => b.confidence - a.confidence)[0]!;
  if (best.patch.fileEdits.length === 0) return null;

  return {
    patch: best.patch,
    rationale: best.rationale,
  };
}

// ---------------------------------------------------------------------------
// Agent path: propose changes for all sites via runAgentInOverlay
// ---------------------------------------------------------------------------

/**
 * Agent-path equivalent of proposeChangeForSite for a single site.
 *
 * Strategy:
 *   1. Snapshot all files currently modified vs HEAD (git diff + untracked) BEFORE this agent call.
 *   2. Run the agent to edit the site file in place.
 *   3. After the agent returns, identify which files changed NEW in this agent run
 *      (post-agent content differs from pre-site baseline content).
 *   4. Revert those new changes to pre-site baseline so verifySiteChange can apply
 *      the patch from scratch.
 *   5. Return a ProposedSiteChange with the filtered patch.
 */
export async function proposeChangeForSiteViaAgent(
  site: ComplementarySite,
  fix: FixCandidate,
  locus: BugLocus,
  invariant: InvariantClaim | undefined,
  llm: LLMProvider,
  overlay: OverlayHandle,
): Promise<ProposedSiteChange | null> {
  // Step 1: Snapshot all files currently changed vs HEAD before this agent call.
  // This captures accepted patches from earlier sites so we can diff them out.
  const isOverlayInternal = (f: string) => f.startsWith(".provekit/") || f === ".provekit";
  const preTracked = getChangedFiles(overlay.worktreePath).filter((f) => !isOverlayInternal(f));
  const preUntracked = getUntrackedFiles(overlay.worktreePath).filter((f) => !isOverlayInternal(f));
  const preAllFiles = [...new Set([...preTracked, ...preUntracked])];

  // Read the current on-disk content of each pre-existing change for comparison.
  const baselineContents = new Map<string, string>();
  for (const f of preAllFiles) {
    const absPath = join(overlay.worktreePath, f);
    try {
      baselineContents.set(f, readFileSync(absPath, "utf-8"));
    } catch {
      baselineContents.set(f, "");
    }
  }

  // Build the agent prompt.
  let siteSourceContext = "(source not available)";
  try {
    if (site.fileRelPath) {
      const sitePath = join(overlay.worktreePath, site.fileRelPath);
      if (existsSync(sitePath)) {
        siteSourceContext = readFileSync(sitePath, "utf-8");
      } else if (existsSync(site.filePath)) {
        siteSourceContext = readFileSync(site.filePath, "utf-8");
      }
    } else if (existsSync(site.filePath)) {
      siteSourceContext = readFileSync(site.filePath, "utf-8");
    }
  } catch {
    siteSourceContext = site.reason;
  }

  const prompt = `You are a code-repair expert. A bug was just fixed at one site in a codebase.
Apply a complementary change at the adjacent site described below to prevent the same bug class.

== Primary fix ==
Fixed at node: ${locus.primaryNode} (containing function: ${locus.containingFunction})
File: ${locus.file}:${locus.line}
Patch applied:
${JSON.stringify(fix.patch, null, 2)}

== Adjacent site to fix ==
Site kind: ${site.kind}
Site node ID: ${site.nodeId}
File: ${site.fileRelPath ?? site.filePath}
Discovery reason: ${site.reason}
Source:
\`\`\`
${siteSourceContext.slice(0, 2000)}
\`\`\`
${invariant ? `\nInvariant violated at primary: ${invariant.description}` : ""}

Apply the complementary fix directly to the file using your tools.
If no change is needed at this site, write a file named .provekit/c4-skip.txt containing "skip".
Otherwise, edit the file at ${site.fileRelPath ?? site.filePath} to apply the complementary fix.`;

  // Step 2: Run the agent.
  let agentResult: Awaited<ReturnType<typeof runAgentInOverlay>>;
  try {
    agentResult = await runAgentInOverlay({
      overlay,
      llm,
      prompt,
      allowedTools: ["Read", "Edit", "Write", "Bash", "Glob", "Grep"],
    });
  } catch (err) {
    // Agent threw (e.g. no agent method). Caller should handle.
    throw err;
  }

  // Check for explicit skip signal.
  const skipPath = join(overlay.worktreePath, ".provekit", "c4-skip.txt");
  if (existsSync(skipPath)) {
    // Clean up skip file.
    try { writeFileSync(skipPath, "", "utf-8"); } catch { /* ignore */ }
    return null;
  }

  // Step 3: Identify which files changed NEW in this agent run.
  const postTracked = getChangedFiles(overlay.worktreePath).filter((f) => !isOverlayInternal(f));
  const postUntracked = getUntrackedFiles(overlay.worktreePath).filter((f) => !isOverlayInternal(f));
  const postAllFiles = [...new Set([...postTracked, ...postUntracked])];

  const newFileEdits: { file: string; newContent: string }[] = [];
  for (const f of postAllFiles) {
    const absPath = join(overlay.worktreePath, f);
    let postContent: string;
    try {
      postContent = readFileSync(absPath, "utf-8");
    } catch {
      continue;
    }

    const baselineContent = baselineContents.get(f);
    if (baselineContent === undefined || postContent !== baselineContent) {
      // This file was not in baseline OR content changed since baseline.
      newFileEdits.push({ file: f, newContent: postContent });
    }
  }

  if (newFileEdits.length === 0) {
    return null;
  }

  // Step 4: Revert those files to pre-site baseline so verifySiteChange can apply normally.
  for (const edit of newFileEdits) {
    const absPath = join(overlay.worktreePath, edit.file);
    const baseline = baselineContents.get(edit.file);
    if (baseline !== undefined) {
      // File existed in baseline (from a prior accepted site) — restore to that content.
      writeFileSync(absPath, baseline, "utf-8");
    } else {
      // File was not in the baseline snapshot. It could be:
      //   (a) A committed tracked file that wasn't changed before this agent run.
      //       → Restore from git HEAD.
      //   (b) Truly new file added by this agent.
      //       → Delete it.
      // Try to restore from HEAD; if that fails (file doesn't exist at HEAD), delete.
      try {
        const headContent = execFileSync(
          "git",
          ["show", `HEAD:${edit.file}`],
          { cwd: overlay.worktreePath, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
        );
        writeFileSync(absPath, headContent, "utf-8");
      } catch {
        // File doesn't exist at HEAD — truly new — delete it.
        try {
          const { unlinkSync } = require("fs") as typeof import("fs");
          if (existsSync(absPath)) unlinkSync(absPath);
        } catch { /* best effort */ }
      }
    }
  }

  // Step 5: Return the filtered patch.
  return {
    patch: {
      fileEdits: newFileEdits,
      description: `complementary fix at ${site.fileRelPath ?? site.filePath} (agent)`,
    },
    rationale: agentResult.rationale || site.reason,
  };
}

// ---------------------------------------------------------------------------
// Oracle #3: verify site change
// ---------------------------------------------------------------------------

export interface SiteVerificationResult {
  verifiedAgainstOverlay: boolean;
  verdict: "sat" | "unsat" | "unknown" | "error" | "bug_site_removed";
  z3RunMs: number;
}

/**
 * Oracle #3: verify that the proposed site change closes the bug at this site.
 *
 * Strategy:
 *   1. Ensure the site's file is indexed in the overlay scratch DB (pre-patch baseline).
 *   2. Count principle matches in the site's file BEFORE the patch (M_before).
 *   3. Snapshot current overlay file contents for rollback.
 *   4. Apply the proposed patch to the overlay (cumulative state).
 *   5. Re-index the modified files in the overlay scratch DB.
 *   6. Count principle matches in the site's file AFTER the patch (M_after).
 *   7. If M_after < M_before → bug_site_removed.
 *   8. Else: look at the patched file content directly to see if the binding
 *      source_expr strings are gone (novel path fallback).
 *   9. On failure: restore snapshot + re-index.
 *
 * Note on node IDs: the overlay scratch DB has its own auto-incremented fileIds,
 * so node IDs from the main DB (site.nodeId) cannot be used to filter overlay
 * scratch DB results. Instead, we filter by file path and compare match counts.
 *
 * Returns verifiedAgainstOverlay: true only for "unsat" or "bug_site_removed".
 * "unknown" is treated as failure per spec decision #6.
 */
export async function verifySiteChange(
  proposed: ProposedSiteChange,
  site: ComplementarySite,
  overlay: OverlayHandle,
  invariant: InvariantClaim | undefined,
): Promise<SiteVerificationResult> {
  const z3Start = Date.now();

  // Step 1: Ensure the site's file is indexed in the overlay scratch DB BEFORE the patch.
  // This is necessary for adjacent files that weren't indexed during openOverlay.
  ensureFileIndexedInOverlay(overlay, site.fileRelPath);

  // Step 2: Count principle matches in the site's file BEFORE the patch.
  // Clear existing principle_matches in the overlay scratch DB first to avoid
  // duplicate rows from prior evaluatePrinciple calls in this C4 session.
  const relPath = site.fileRelPath;
  let beforeMatchCount = 0;
  let dslSource: string | null = null;
  if (invariant?.principleId) {
    const dslPath = join(
      overlay.worktreePath,
      ".provekit",
      "principles",
      `${invariant.principleId}.dsl`,
    );
    if (existsSync(dslPath)) {
      try {
        dslSource = readFileSync(dslPath, "utf-8");
        // Clear principle_matches to avoid duplicates from prior evaluations.
        clearOverlayPrincipleMatches(overlay);
        evaluatePrinciple(overlay.sastDb, dslSource);
        beforeMatchCount = countMatchesForFile(overlay, relPath);
      } catch {
        beforeMatchCount = 0;
        dslSource = null;
      }
    }
  }

  // Step 3: Snapshot current overlay file contents for rollback.
  const snapshots = new Map<string, string | null>();
  for (const edit of proposed.patch.fileEdits) {
    const absPath = join(overlay.worktreePath, edit.file);
    if (existsSync(absPath)) {
      try {
        snapshots.set(edit.file, readFileSync(absPath, "utf-8"));
      } catch {
        snapshots.set(edit.file, null);
      }
    } else {
      snapshots.set(edit.file, null);
    }
  }

  // Step 4: Apply patch.
  applyPatchToOverlay(overlay, proposed.patch);

  // Step 5: Re-index modified files.
  await reindexOverlay(overlay);

  // Step 6: Check if site's bug is closed.
  let verdict: "sat" | "unsat" | "unknown" | "error" | "bug_site_removed";
  try {
    verdict = await checkSiteVerdict(
      site,
      overlay,
      invariant,
      dslSource,
      relPath,
      beforeMatchCount,
    );
  } catch {
    verdict = "error";
  }

  const z3RunMs = Date.now() - z3Start;
  const verifiedAgainstOverlay = verdict === "bug_site_removed" || verdict === "unsat";

  // Step 7: On failure, restore snapshot + re-index to keep overlay clean.
  if (!verifiedAgainstOverlay) {
    for (const [relFilePath, content] of snapshots) {
      const absPath = join(overlay.worktreePath, relFilePath);
      if (content !== null) {
        try {
          writeFileSync(absPath, content, "utf-8");
        } catch {
          // Best effort rollback.
        }
      }
    }
    // Re-index restored files.
    try {
      await reindexOverlay(overlay, [...snapshots.keys()]);
    } catch {
      // Best effort.
    }
  }

  return { verifiedAgainstOverlay, verdict, z3RunMs };
}

/**
 * Count principle matches associated with a specific overlay file.
 *
 * Looks up the file in the overlay scratch DB by its absolute path (the
 * worktree copy), then queries principle_matches count for that file_id.
 *
 * Returns 0 if the file isn't indexed yet or has no matches.
 */
function countMatchesForFile(
  overlay: OverlayHandle,
  relPath: string | null,
): number {
  if (!relPath) return 0;
  // Use raw (non-realpath) path to match how reindexOverlay/buildSASTForFile stores paths.
  const overlayFilePath = join(overlay.worktreePath, relPath);

  // Look up the file_id in the overlay scratch DB.
  const fileRow = overlay.sastDb
    .select({ id: filesTable.id })
    .from(filesTable)
    .where(eq(filesTable.path, overlayFilePath))
    .get();
  if (!fileRow) return 0;

  // Count principle_matches rows for this fileId.
  const matchRows = overlay.sastDb
    .select({ id: principleMatches.id })
    .from(principleMatches)
    .where(eq(principleMatches.fileId, fileRow.id))
    .all();

  return matchRows.length;
}

/**
 * Clear all principle_matches from the overlay scratch DB.
 * Used before calling evaluatePrinciple to avoid duplicate rows from
 * prior evaluations in the same C4 session.
 *
 * Safe because the overlay scratch DB is not the main DB.
 */
function clearOverlayPrincipleMatches(overlay: OverlayHandle): void {
  try {
    overlay.sastDb.delete(principleMatchCaptures).run();
    overlay.sastDb.delete(principleMatches).run();
  } catch {
    // Ignore — worst case: duplicate rows, which are harmless for counting.
  }
}

/**
 * Determine the oracle #3 verdict for a site.
 *
 * Principle path: re-evaluate principle against overlay scratch DB.
 *   - Count file-level matches before (beforeMatchCount) and after (M_after).
 *   - If M_before ≥ 1 and M_after < M_before → bug_site_removed (pass).
 *   - If M_after >= M_before → sat (fail — fix didn't close it).
 *   - If M_before == 0 → site wasn't indexed or principle didn't fire: unknown.
 *
 * Novel path: check if invariant binding source_expr strings are gone
 *   from the patched file → bug_site_removed.
 *
 * Fallback: unknown.
 */
async function checkSiteVerdict(
  site: ComplementarySite,
  overlay: OverlayHandle,
  invariant: InvariantClaim | undefined,
  dslSource: string | null,
  relPath: string | null,
  beforeMatchCount: number,
): Promise<"sat" | "unsat" | "unknown" | "error" | "bug_site_removed"> {
  // Principle path.
  if (dslSource !== null) {
    try {
      // Clear existing principle_matches before re-evaluating to avoid duplicates.
      clearOverlayPrincipleMatches(overlay);
      evaluatePrinciple(overlay.sastDb, dslSource);
      const afterMatchCount = countMatchesForFile(overlay, relPath);

      if (beforeMatchCount >= 1 && afterMatchCount < beforeMatchCount) {
        return "bug_site_removed";
      }
      if (afterMatchCount >= beforeMatchCount && beforeMatchCount >= 1) {
        return "sat";
      }
      if (beforeMatchCount === 0) {
        // The site file wasn't indexed or principle didn't fire before the patch.
        // Fall through to novel/source_expr check.
      }
    } catch {
      return "unknown";
    }
  }

  // Novel / fallback path: check if invariant binding source_expr strings
  // are absent from the patched file.
  if (invariant?.bindings && invariant.bindings.length > 0 && relPath) {
    const patchedPath = join(overlay.worktreePath, relPath);
    if (existsSync(patchedPath)) {
      try {
        const content = readFileSync(patchedPath, "utf-8");
        const allGone = invariant.bindings.every((b) => !content.includes(b.source_expr));
        if (allGone) {
          return "bug_site_removed";
        }
        // Some bindings still present.
        return "sat";
      } catch {
        return "unknown";
      }
    }
  }

  // Also check all modified files for binding source_expr absence.
  if (invariant?.bindings && invariant.bindings.length > 0) {
    const modifiedContents: string[] = [];
    for (const rel of overlay.modifiedFiles) {
      const absPath = join(overlay.worktreePath, rel);
      if (existsSync(absPath)) {
        try {
          modifiedContents.push(readFileSync(absPath, "utf-8"));
        } catch {
          // ignore
        }
      }
    }
    if (modifiedContents.length > 0) {
      const allGone = invariant.bindings.every(
        (b) => !modifiedContents.some((content) => content.includes(b.source_expr)),
      );
      if (allGone) {
        return "bug_site_removed";
      }
      return "sat";
    }
  }

  // No principle DSL, no bindings — cannot verify.
  return "unknown";
}
