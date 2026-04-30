/**
 * Phase 2-A: recognition mode (no LLM).
 *
 * For each HarvestCandidate, materialize its buggy files, build SAST in a
 * scratch DB, evaluate every principle in the library against that DB, and
 * record any principle whose match falls in one of the candidate's changed
 * files. A recognized candidate's bug is already covered by an existing
 * principle: we don't need discovery mode for it. The harvest pipeline can
 * append provenance to the matched principle and skip the expensive path.
 *
 * This is the same mechanical operation the production B3 stage runs at C1
 * for fix loops. The harvest pipeline calls it in batch.
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, existsSync } from "fs";
import { dirname, join } from "path";
import { tmpdir } from "os";
import { eq, and, inArray } from "drizzle-orm";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { fileURLToPath } from "url";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { evaluatePrinciple } from "../../dsl/evaluator.js";
import { files, nodes } from "../../sast/schema/nodes.js";
import { principleMatches } from "../../db/schema/principleMatches.js";
import { prePostDiff } from "../../db/schema/preDiff.js";
import type { HarvestCandidate } from "./extractBugs.js";
import { recordCandidateDiff, setActiveCandidate } from "./diff.js";
import { enumeratePrincipleFiles } from "../../principleEnumeration.js";

/**
 * Principles that intentionally bind their `at` to a node WITHOUT change
 * (e.g., `or-chain-extended-by-fix` binds `at $or` where $or pairs as
 * `unchanged` in the diff — the inner OR survives, the OUTER one is added).
 * The recognition harness's mining-context dirty-set post-filter must not
 * drop those matches.
 *
 * Convention: any principle whose name contains `extended-by-fix` or
 * starts with `diff-` is treated as self-managing its diff-awareness.
 */
function isDiffAwarePrincipleName(name: string): boolean {
  if (name.startsWith("diff-")) return true;
  if (name.includes("extended-by-fix")) return true;
  if (name.includes("replaced-by-fix")) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RecognizedMatch {
  /** Principle name as declared in the DSL `principle <name> { ... }`. */
  principleName: string;
  /** Path of the buggy file the match landed in, relative to candidate root. */
  filePath: string;
  /** 1-based source line of the matched root node. */
  line: number;
}

export interface RecognitionResult {
  /** Matches found across all changed buggy files. */
  matches: RecognizedMatch[];
  /** True if at least one principle matched in a changed (non-test) file. */
  recognized: boolean;
  /** Number of buggy files that were built into SAST (test files skipped). */
  filesIndexed: number;
  /** Number of principle DSL files attempted. */
  principlesEvaluated: number;
  /** Number of principle DSL files that threw at parse/compile/eval time. */
  principleErrors: number;
}

export interface RecognizeOptions {
  /** Path to .provekit/principles/ directory. Defaults to project's. */
  principlesDir?: string;
  /** Optional parent for the scratch dir; tests scope this to per-test parents. */
  scratchParent?: string;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Materialize the candidate's buggy files into a scratch tree, build SAST,
 * evaluate every principle, and return the matches that landed in those files.
 *
 * Side-effect-free outside its scratch dir; cleans up on success or failure.
 */
export function recognizeCandidate(
  candidate: HarvestCandidate,
  options: RecognizeOptions = {},
): RecognitionResult {
  const principlesDir = options.principlesDir ?? defaultPrinciplesDir();
  const scratchParent = options.scratchParent ?? tmpdir();
  const scratchDir = mkdtempSync(join(scratchParent, "provekit-harvest-recognize-"));

  let db: ReturnType<typeof openDb> | null = null;
  try {
    // 1. Materialize buggy files. Track abs↔relPath so we can pass the
    // remapped paths to recordCandidateDiff (the diff writer keys
    // pre_post_diff rows by the file path stored in `files.path`, which
    // is what nodes.file_id joins on at DSL-relation eval time).
    const productionPaths: string[] = [];
    const remappedBuggy: Record<string, string> = {};
    const remappedFixed: Record<string, string> = {};
    for (const [relPath, content] of Object.entries(candidate.buggyFiles)) {
      if (isTestPath(relPath)) continue;
      const abs = join(scratchDir, "src", relPath);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content, "utf-8");
      productionPaths.push(abs);
      remappedBuggy[abs] = content;
      const fixed = candidate.fixedFiles[relPath];
      if (fixed !== undefined) remappedFixed[abs] = fixed;
    }

    if (productionPaths.length === 0) {
      // Pure test-only candidate (shouldn't happen — extractBugs filters these
      // already). Defensive return.
      return { matches: [], recognized: false, filesIndexed: 0, principlesEvaluated: 0, principleErrors: 0 };
    }

    // 2. Open scratch DB + migrations.
    const dbPath = join(scratchDir, "scratch.db");
    db = openDb(dbPath);
    migrate(db, { migrationsFolder: resolveMigrationsDir() });

    // 3. Build SAST for each file. Track path → file_id for later joining.
    const indexedAbsPaths: string[] = [];
    for (const abs of productionPaths) {
      try {
        buildSASTForFile(db, abs);
        indexedAbsPaths.push(abs);
      } catch {
        // Swallow per-file build errors — recognition is opportunistic.
        // A file the parser can't handle simply doesn't contribute matches.
      }
    }

    // 3.5. Record the pre/post diff and set the active diff context so
    // diff-aware DSL relations (`is_in_dirty_set`, `was_replaced_by_addition`)
    // can fire. Without this, principles using those relations correctly
    // return false everywhere — which would silently zero out the
    // recognition rate of every principle that depends on diff signal.
    try {
      recordCandidateDiff(db, {
        ...candidate,
        buggyFiles: remappedBuggy,
        fixedFiles: remappedFixed,
      });
      setActiveCandidate(db, candidate.source.project, candidate.source.bugId);
    } catch {
      // Diff recording is best-effort — recognition should still attempt
      // even if the diff couldn't be classified for some reason.
    }

    // 4. Load principle DSL files and evaluate each. Failures are per-DSL
    // skipped, not fatal — a malformed library entry shouldn't poison the
    // whole harvest run.
    let principlesEvaluated = 0;
    let principleErrors = 0;
    if (existsSync(principlesDir)) {
      // Partition-aware (task #134): every partition's DSL is evaluated
      // because harvest runs cross-corpus and we want any applicable
      // principle to fire. loadAllPartitions=true mirrors the B3
      // recognize stage decision for the same reason.
      const { dslPaths } = enumeratePrincipleFiles(principlesDir, {
        loadAllPartitions: true,
      });
      for (const dslPath of dslPaths) {
        principlesEvaluated++;
        let dslSource: string;
        try {
          dslSource = readFileSync(dslPath, "utf-8");
        } catch {
          principleErrors++;
          continue;
        }
        try {
          evaluatePrinciple(db, dslSource);
        } catch {
          principleErrors++;
        }
      }
    }

    // 5. Read principleMatches and map to relative paths. Two filters apply
    // to count a match as recognition:
    //   (a) coarse line-level: the match's source_line falls within a line
    //       range the candidate's diff touched. Rejects matches in files
    //       the diff doesn't reach at all.
    //   (b) node-level dirty set: the match's `at` node has change_kind !=
    //       'unchanged' in pre_post_diff. Rejects matches on stable code
    //       that happens to live on a changed line (e.g., addition-overflow
    //       firing on `parentElements[0].loc.start.line === parent.loc.start.line`
    //       at line 729 because the surrounding if-statement got a null-guard).
    //       Filter (b) is exempted for principles that intentionally bind
    //       to unchanged nodes — see `isDiffAwarePrincipleName`.
    //
    // The node-level filter is the architectural fix for #115's
    // recognized-stratum over-firing: rather than every DSL author having
    // to add `require ... is_in_dirty_set($matched)`, the harness applies
    // the constraint uniformly. DSL stays clean.
    const dirtyLinesByPath = parseDiffDirtyLines(candidate.diff);

    const matchRows = db
      .select({
        principleName: principleMatches.principleName,
        fileId: principleMatches.fileId,
        rootNodeId: principleMatches.rootMatchNodeId,
      })
      .from(principleMatches)
      .all();

    const matches: RecognizedMatch[] = [];
    if (matchRows.length > 0) {
      const fileIds = Array.from(new Set(matchRows.map((r) => r.fileId)));
      const fileRows = db
        .select({ id: files.id, path: files.path })
        .from(files)
        .where(inArray(files.id, fileIds))
        .all();
      const pathById = new Map<number, string>();
      for (const r of fileRows) pathById.set(r.id, r.path);

      for (const m of matchRows) {
        const absPath = pathById.get(m.fileId);
        if (!absPath) continue;
        const relPath = relativeToScratch(absPath, scratchDir);
        if (relPath === null) continue;
        const line = lineForNode(db, m.rootNodeId);
        const dirtyLines = dirtyLinesByPath.get(relPath);
        if (!dirtyLines || !lineInRanges(line, dirtyLines)) continue;

        // Node-level dirty-set filter (mining-context architectural fix).
        // Skip principles that intentionally fire on unchanged nodes.
        if (!isDiffAwarePrincipleName(m.principleName)) {
          const isDirty = isNodeDirty(db, m.rootNodeId);
          if (!isDirty) continue;
        }

        matches.push({ principleName: m.principleName, filePath: relPath, line });
      }
    }

    return {
      matches,
      recognized: matches.length > 0,
      filesIndexed: indexedAbsPaths.length,
      principlesEvaluated,
      principleErrors,
    };
  } finally {
    try {
      if (db) db.$client.close();
    } catch { /* ignore */ }
    try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * True iff the SAST node identified by `nodeId` corresponds to a
 * pre_post_diff row whose change_kind is anything but `unchanged` —
 * i.e., the fix actually touched this node. Joins via (file_path,
 * source_start, kind) which uniquely identifies the node since both
 * tables key on getFullStart() and the kind name.
 *
 * Returns FALSE when:
 *   - The node has a pre_post_diff row with change_kind = 'unchanged'
 *   - No pre_post_diff row exists for this node (defensive — recognition
 *     should not silently include matches whose diff classification we
 *     couldn't compute)
 */
function isNodeDirty(db: ReturnType<typeof openDb>, nodeId: string): boolean {
  const rows = db
    .select({ kind: prePostDiff.changeKind })
    .from(prePostDiff)
    .innerJoin(files, eq(files.path, prePostDiff.filePath))
    .innerJoin(nodes, eq(nodes.fileId, files.id))
    .where(
      and(
        eq(nodes.id, nodeId),
        eq(prePostDiff.preStart, nodes.sourceStart),
        eq(prePostDiff.preKind, nodes.kind),
      ),
    )
    .all();
  if (rows.length === 0) return false;
  return rows.some((r) => r.kind !== "unchanged");
}

/**
 * Parse a unified diff into per-file dirty-line ranges. Each entry is the
 * line range in the BUGGY (pre-fix) file that the diff touched, expressed
 * as a list of [startLine, endLine] inclusive 1-based ranges.
 *
 * For recognition we want "did the principle fire near the bug?" so we use
 * the OLD-file line ranges from the hunk header (`@@ -<start>,<len> +... @@`).
 * A locus match falls inside one of these ranges iff its source_line is
 * within ±LINE_NEIGHBORHOOD of any dirty line. Strict equality is too tight
 * (the principle's root node may sit a couple of lines away from the
 * exact changed line); LINE_NEIGHBORHOOD = 3 forgives that without
 * collapsing the constraint.
 */
const LINE_NEIGHBORHOOD = 3;

export function parseDiffDirtyLines(diff: string): Map<string, Array<[number, number]>> {
  const out = new Map<string, Array<[number, number]>>();
  const lines = diff.split("\n");
  let currentPath: string | null = null;
  for (const line of lines) {
    // Track the current file via "diff --git a/<path> b/<path>" or "--- a/<path>".
    const gitMatch = /^diff --git a\/(.+?) b\/(.+)$/.exec(line);
    if (gitMatch) {
      currentPath = gitMatch[1] ?? null;
      continue;
    }
    const fromMatch = /^---\s+a\/(.+)$/.exec(line);
    if (fromMatch) {
      currentPath = fromMatch[1] ?? currentPath;
      continue;
    }
    const hunkMatch = /^@@ -(\d+)(?:,(\d+))? \+\d+(?:,\d+)? @@/.exec(line);
    if (hunkMatch && currentPath) {
      const start = parseInt(hunkMatch[1]!, 10);
      const len = hunkMatch[2] !== undefined ? parseInt(hunkMatch[2], 10) : 1;
      const end = start + Math.max(len - 1, 0);
      // Expand by LINE_NEIGHBORHOOD on each side to forgive principles whose
      // match root sits a few lines off the exact changed range.
      const expandedStart = Math.max(1, start - LINE_NEIGHBORHOOD);
      const expandedEnd = end + LINE_NEIGHBORHOOD;
      let arr = out.get(currentPath);
      if (!arr) {
        arr = [];
        out.set(currentPath, arr);
      }
      arr.push([expandedStart, expandedEnd]);
    }
  }
  return out;
}

function lineInRanges(line: number, ranges: Array<[number, number]>): boolean {
  for (const [start, end] of ranges) {
    if (line >= start && line <= end) return true;
  }
  return false;
}

function isTestPath(p: string): boolean {
  const segments = p.split("/");
  for (const seg of segments) {
    if (seg === "test" || seg === "tests" || seg === "__tests__") return true;
  }
  return /\.(test|spec)\.[^/]+$/.test(p);
}

function relativeToScratch(absPath: string, scratchDir: string): string | null {
  const prefix = join(scratchDir, "src") + "/";
  if (!absPath.startsWith(prefix)) return null;
  return absPath.slice(prefix.length);
}

function lineForNode(db: ReturnType<typeof openDb>, nodeId: string): number {
  const rows = db.$client
    .prepare(`SELECT source_line FROM nodes WHERE id = ?`)
    .all(nodeId) as { source_line: number }[];
  return rows[0]?.source_line ?? 0;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function defaultPrinciplesDir(): string {
  // src/fix/harvest/recognize.ts → projectRoot/.provekit/principles
  return join(__dirname, "..", "..", "..", ".provekit", "principles");
}

function resolveMigrationsDir(): string {
  return join(__dirname, "..", "..", "..", "drizzle");
}
