/**
 * Bug-tag-pair extractor — Phase 1 of the BugsJS harvest pipeline.
 *
 * BugsJS forks each tag every bug five ways:
 *   Bug-N           — the buggy code, base for diffs
 *   Bug-N-fix       — Bug-N + the production-code fix (no test)
 *   Bug-N-test      — Bug-N + the regression test (no fix)
 *   Bug-N-full      — Bug-N + fix + test
 *   Bug-N-original  — the upstream historical commit at the moment of fix
 *                     (carries full project state + the upstream commit message)
 *
 * For harvest the load-bearing pairing is `Bug-N..Bug-N-fix`. That diff is
 * the production-only fix the upstream maintainer landed, with no test
 * pollution. Tests are read separately from `Bug-N..Bug-N-test`.
 *
 * Filters at this stage are mechanical: skip diffs that touch more than
 * `maxFiles` files or whose changed files together exceed `maxLoc` lines,
 * and skip diffs that touch ONLY test files (no production principle to
 * harvest). Skipped bugs are surfaced in the result with a reason so the
 * filter rates can be inspected without running the full harvest.
 */

import { execFileSync } from "child_process";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface HarvestCandidateSource {
  project: string;
  bugId: string;            // numeric string, e.g. "1", "27"
  baseSha: string;          // resolved Bug-N tag SHA
  fixSha: string;           // resolved Bug-N-fix tag SHA
  testSha: string | null;   // resolved Bug-N-test tag SHA (null if no test tag)
  originalSha: string | null; // resolved Bug-N-original tag SHA (null if missing)
}

export interface HarvestCandidate {
  source: HarvestCandidateSource;
  /** Buggy file contents at Bug-N, keyed by path. Only files that change in the fix diff. */
  buggyFiles: Record<string, string>;
  /** Fixed file contents at Bug-N-fix, keyed by path. Only files that change in the fix diff. */
  fixedFiles: Record<string, string>;
  /** Unified diff of Bug-N..Bug-N-fix. */
  diff: string;
  /** Commit message of Bug-N-original (the upstream fix commit). Empty if unavailable. */
  upstreamFixMessage: string;
  /** Test files added/changed in Bug-N..Bug-N-test, keyed by path. Empty if no test tag. */
  testFiles: Record<string, string>;
  /** Statistics inferred from the diff. */
  stats: { filesChanged: number; insertions: number; deletions: number };
}

export interface ExtractOptions {
  /** Path to a single BugsJS project clone (e.g. /Users/tsavo/bugsjs/express). */
  projectPath: string;
  /** Project name; defaults to basename(projectPath). */
  project?: string;
  /** Skip bugs whose fix diff touches more than this many files. Default 2. */
  maxFiles?: number;
  /** Skip bugs whose fix diff total +/- lines exceed this. Default 50. */
  maxLoc?: number;
  /** Optional cap on bugs to extract; enumerates in numeric order. */
  maxBugs?: number;
  /** Specific bug IDs to extract (numeric strings). Overrides numeric enumeration. */
  onlyBugIds?: string[];
}

export interface ExtractResult {
  candidates: HarvestCandidate[];
  skipped: { bugId: string; reason: string }[];
  totalBugIds: number;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Walk a BugsJS project clone, extract all Bug-N..Bug-N-fix pairs that pass
 * the cleanliness filters, and return them as HarvestCandidates.
 */
export function extractBugs(opts: ExtractOptions): ExtractResult {
  const projectPath = opts.projectPath;
  const project = opts.project ?? basename(projectPath);
  const maxFiles = opts.maxFiles ?? 2;
  const maxLoc = opts.maxLoc ?? 50;

  const allIds = listBugIds(projectPath);
  let bugIds = opts.onlyBugIds ?? allIds;
  if (opts.maxBugs !== undefined && opts.onlyBugIds === undefined) {
    bugIds = bugIds.slice(0, opts.maxBugs);
  }

  const candidates: HarvestCandidate[] = [];
  const skipped: { bugId: string; reason: string }[] = [];

  for (const bugId of bugIds) {
    const result = extractBug({ projectPath, project, bugId, maxFiles, maxLoc });
    if ("skipped" in result) {
      skipped.push(result.skipped);
    } else {
      candidates.push(result.candidate);
    }
  }

  return { candidates, skipped, totalBugIds: allIds.length };
}

/**
 * Enumerate the bug IDs present in a project clone. A bug ID is the N in
 * Bug-N tags; we require BOTH Bug-N and Bug-N-fix to exist for it to count.
 * Returned in numeric ascending order.
 */
export function listBugIds(projectPath: string): string[] {
  const tags = git(projectPath, ["tag", "-l", "Bug-*"]).trim().split("\n").filter(Boolean);
  const baseIds = new Set<string>();
  const fixIds = new Set<string>();
  for (const tag of tags) {
    // Match "Bug-<id>" exactly (no suffix) for base, "Bug-<id>-fix" for fix.
    const baseMatch = /^Bug-(\d+)$/.exec(tag);
    if (baseMatch && baseMatch[1]) {
      baseIds.add(baseMatch[1]);
      continue;
    }
    const fixMatch = /^Bug-(\d+)-fix$/.exec(tag);
    if (fixMatch && fixMatch[1]) {
      fixIds.add(fixMatch[1]);
    }
  }
  // Both base and fix must exist.
  const intersect: string[] = [];
  for (const id of baseIds) {
    if (fixIds.has(id)) intersect.push(id);
  }
  intersect.sort((a, b) => parseInt(a, 10) - parseInt(b, 10));
  return intersect;
}

// ---------------------------------------------------------------------------
// Internal: extract one bug
// ---------------------------------------------------------------------------

function extractBug(args: {
  projectPath: string;
  project: string;
  bugId: string;
  maxFiles: number;
  maxLoc: number;
}): { candidate: HarvestCandidate } | { skipped: { bugId: string; reason: string } } {
  const { projectPath, project, bugId, maxFiles, maxLoc } = args;

  const baseTag = `Bug-${bugId}`;
  const fixTag = `Bug-${bugId}-fix`;
  const testTag = `Bug-${bugId}-test`;
  const originalTag = `Bug-${bugId}-original`;

  // Resolve required SHAs.
  const baseSha = tryRevParse(projectPath, baseTag);
  const fixSha = tryRevParse(projectPath, fixTag);
  if (!baseSha || !fixSha) {
    return { skipped: { bugId, reason: `missing required tag (base=${!!baseSha}, fix=${!!fixSha})` } };
  }
  const testSha = tryRevParse(projectPath, testTag);
  const originalSha = tryRevParse(projectPath, originalTag);

  // Get the unified diff base..fix.
  const diff = git(projectPath, ["diff", `${baseTag}..${fixTag}`]);
  if (diff.trim().length === 0) {
    return { skipped: { bugId, reason: "empty diff between Bug-N and Bug-N-fix" } };
  }

  // Names of changed files.
  const nameStatus = git(projectPath, ["diff", "--name-status", `${baseTag}..${fixTag}`])
    .trim().split("\n").filter(Boolean);
  const changedPaths: string[] = [];
  for (const line of nameStatus) {
    // Format: "<status>\t<path>" or "R<score>\t<old>\t<new>" for renames.
    const parts = line.split("\t");
    if (parts.length >= 2) {
      // For renames take the new name (last token).
      changedPaths.push(parts[parts.length - 1]!);
    }
  }

  // Filter: max files.
  if (changedPaths.length > maxFiles) {
    return { skipped: { bugId, reason: `changed ${changedPaths.length} files > maxFiles=${maxFiles}` } };
  }

  // Filter: test-only diff (no production principle to harvest).
  const productionPaths = changedPaths.filter((p) => !isTestPath(p));
  if (productionPaths.length === 0) {
    return { skipped: { bugId, reason: "diff touches only test/ files" } };
  }

  // Stats from --numstat.
  const numstat = git(projectPath, ["diff", "--numstat", `${baseTag}..${fixTag}`])
    .trim().split("\n").filter(Boolean);
  let insertions = 0;
  let deletions = 0;
  for (const line of numstat) {
    const parts = line.split("\t");
    // Binary diffs report "-\t-\t<path>"; treat as 0/0.
    const ins = parseInt(parts[0] ?? "0", 10);
    const del = parseInt(parts[1] ?? "0", 10);
    if (Number.isFinite(ins)) insertions += ins;
    if (Number.isFinite(del)) deletions += del;
  }
  const totalLoc = insertions + deletions;
  if (totalLoc > maxLoc) {
    return { skipped: { bugId, reason: `${totalLoc} +/- lines > maxLoc=${maxLoc}` } };
  }

  // Read file contents at base and fix for the changed paths.
  const buggyFiles: Record<string, string> = {};
  const fixedFiles: Record<string, string> = {};
  for (const path of changedPaths) {
    const buggy = tryShow(projectPath, `${baseTag}:${path}`);
    const fixed = tryShow(projectPath, `${fixTag}:${path}`);
    if (buggy !== null) buggyFiles[path] = buggy;
    if (fixed !== null) fixedFiles[path] = fixed;
  }

  // Read upstream fix commit message from Bug-N-original (the historical
  // upstream commit). Falls back to the synthetic Bug-N-fix message if the
  // -original tag is absent.
  const upstreamFixMessage = (() => {
    if (originalSha) {
      try {
        return git(projectPath, ["log", "-1", "--format=%B", originalTag]).trim();
      } catch { /* fall through */ }
    }
    try {
      return git(projectPath, ["log", "-1", "--format=%B", fixTag]).trim();
    } catch {
      return "";
    }
  })();

  // Read test files that changed in Bug-N..Bug-N-test, if a test tag exists.
  const testFiles: Record<string, string> = {};
  if (testSha) {
    const testNameStatus = (() => {
      try {
        return git(projectPath, ["diff", "--name-only", `${baseTag}..${testTag}`])
          .trim().split("\n").filter(Boolean);
      } catch {
        return [];
      }
    })();
    for (const path of testNameStatus) {
      const content = tryShow(projectPath, `${testTag}:${path}`);
      if (content !== null) testFiles[path] = content;
    }
  }

  const candidate: HarvestCandidate = {
    source: { project, bugId, baseSha, fixSha, testSha, originalSha },
    buggyFiles,
    fixedFiles,
    diff,
    upstreamFixMessage,
    testFiles,
    stats: { filesChanged: changedPaths.length, insertions, deletions },
  };
  return { candidate };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function git(cwd: string, args: string[]): string {
  return execFileSync("git", args, {
    cwd,
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
    // Some BugsJS diffs are large; raise the buffer to 32MB.
    maxBuffer: 32 * 1024 * 1024,
  });
}

function tryRevParse(projectPath: string, ref: string): string | null {
  try {
    return git(projectPath, ["rev-parse", "--verify", `${ref}^{commit}`]).trim();
  } catch {
    return null;
  }
}

function tryShow(projectPath: string, ref: string): string | null {
  try {
    return git(projectPath, ["show", ref]);
  } catch {
    return null;
  }
}

function isTestPath(p: string): boolean {
  // Heuristic: if any path segment is exactly "test", "tests", "__tests__",
  // or the filename ends in .test.<ext> / .spec.<ext>, treat as test.
  const segments = p.split("/");
  for (const seg of segments) {
    if (seg === "test" || seg === "tests" || seg === "__tests__") return true;
  }
  return /\.(test|spec)\.[^/]+$/.test(p);
}

function basename(p: string): string {
  const idx = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return idx === -1 ? p : p.slice(idx + 1);
}
