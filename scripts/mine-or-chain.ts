#!/usr/bin/env tsx
/**
 * Hard-bug 1 acceptance test: mine the BugsJS corpus with
 * `or-chain-extended-by-fix` (the first diff-aware principle) and
 * tabulate per-candidate firings.
 *
 * Per advisor (2026-04-27): the headline metric of hard-bug 1 was
 * "did precision actually move from the 1-2/10 baseline on the
 * enum-disjunction cluster?" Without that read, the substrate is
 * unverified for its stated purpose. This script provides the read.
 *
 * For each candidate (Bug-N..Bug-N-fix pair) in each project:
 *   - Build SAST from the buggy source (only files matched by .ts/.js)
 *   - Record pre/post diff into pre_post_diff
 *   - Set active diff context
 *   - Compile and run `or-chain-extended-by-fix`
 *   - Tabulate: matched | empty (no OR-bearing falsy_default node)
 *
 * Output:
 *   - One line per project summarizing counts
 *   - JSON dump of all matches (project/bug/file/line/text) for manual TP/FP labeling
 *
 * The TP/FP labeling step is manual (read the diff; is the matched
 * BinaryExpression actually an enum-disjunction that the fix extended?).
 * Aim for ~30 matches across the corpus so we can pick 10 for inspection
 * — that's enough to give a stable precision read at the per-shape level.
 */
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from "fs";
import { dirname, join } from "path";
import { tmpdir } from "os";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import {
  recordCandidateDiff,
  setActiveCandidate,
} from "../src/fix/harvest/diff.js";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { parseDSL } from "../src/dsl/parser.js";
import { compileProgram } from "../src/dsl/compiler.js";
import "../src/dsl/relations.js";
import { eq, and, inArray } from "drizzle-orm";
import { nodes, files } from "../src/sast/schema/index.js";
import { prePostDiff } from "../src/db/schema/preDiff.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");
const dslSource = readFileSync(
  join(__dirname, "..", ".provekit", "principles", "or-chain-extended-by-fix.dsl"),
  "utf-8",
);
const program = parseDSL(dslSource);
const compiled = compileProgram(program.nodes);
const principle = compiled.get("or-chain-extended-by-fix")!;

const BUGSJS_ROOT = process.env["BUGSJS_ROOT"] ?? "/Users/tsavo/bugsjs";
const PROJECTS = (process.env["BUGSJS_PROJECTS"] ?? "bower,eslint,express,hessian.js,hexo,karma,pencilblue,shields").split(",");
const MAX_BUGS_PER_PROJECT = process.env["MAX_BUGS"]
  ? parseInt(process.env["MAX_BUGS"], 10)
  : undefined;

interface MatchRecord {
  project: string;
  bugId: string;
  file: string;
  line: number;
  col: number;
  textPreview: string;
}

interface ProjectStats {
  candidatesTotal: number;
  candidatesProcessed: number;
  candidatesWithMatch: number;
  errorCount: number;
  matchCount: number;
}

const allMatches: MatchRecord[] = [];
const stats = new Map<string, ProjectStats>();

function isJsFile(path: string): boolean {
  return /\.(?:js|jsx|ts|tsx|mjs|cjs)$/.test(path);
}

for (const project of PROJECTS) {
  const projectPath = join(BUGSJS_ROOT, project);
  if (!existsSync(projectPath)) {
    console.log(`SKIP ${project}: not found at ${projectPath}`);
    continue;
  }
  const ps: ProjectStats = {
    candidatesTotal: 0,
    candidatesProcessed: 0,
    candidatesWithMatch: 0,
    errorCount: 0,
    matchCount: 0,
  };
  stats.set(project, ps);

  let extracted;
  try {
    extracted = extractBugs({
      projectPath,
      project,
      maxFiles: 2,
      maxLoc: 50,
      maxBugs: MAX_BUGS_PER_PROJECT,
    });
  } catch (err) {
    console.log(`FAIL ${project}: extract error: ${(err as Error).message}`);
    ps.errorCount += 1;
    continue;
  }

  ps.candidatesTotal = extracted.candidates.length;
  console.log(`\n=== ${project}: ${extracted.candidates.length} candidates (${extracted.skipped.length} skipped by filter) ===`);

  for (const candidate of extracted.candidates) {
    const scratch = mkdtempSync(join(tmpdir(), `mine-${project}-${candidate.source.bugId}-`));
    try {
      const dbPath = join(scratch, "test.db");
      const db = openDb(dbPath);
      migrate(db, { migrationsFolder });

      // Materialize the buggy files into the scratch dir, build SAST.
      const candidateForDiff = { ...candidate, buggyFiles: { ...candidate.buggyFiles }, fixedFiles: { ...candidate.fixedFiles } };
      // Rewrite keys to absolute paths in scratch so the files row's path
      // matches what recordCandidateDiff stores. SAST nodes' file_id joins
      // pre_post_diff via files.path.
      const remappedBuggy: Record<string, string> = {};
      const remappedFixed: Record<string, string> = {};
      for (const relPath of Object.keys(candidate.buggyFiles)) {
        if (!isJsFile(relPath)) continue;
        const abs = join(scratch, relPath);
        mkdirSync(dirname(abs), { recursive: true });
        writeFileSync(abs, candidate.buggyFiles[relPath]!, "utf-8");
        try { buildSASTForFile(db, abs); } catch { /* parse errors non-fatal */ }
        remappedBuggy[abs] = candidate.buggyFiles[relPath]!;
        if (candidate.fixedFiles[relPath] !== undefined) {
          remappedFixed[abs] = candidate.fixedFiles[relPath]!;
        }
      }
      candidateForDiff.buggyFiles = remappedBuggy;
      candidateForDiff.fixedFiles = remappedFixed;

      recordCandidateDiff(db, candidateForDiff);
      setActiveCandidate(db, project, candidate.source.bugId);

      const matches = principle(db);
      ps.candidatesProcessed += 1;
      if (matches.length > 0) {
        ps.candidatesWithMatch += 1;
        ps.matchCount += matches.length;
        const matchIds = matches.map((m) => m.atNodeId);
        const nodeRows = db
          .select({
            id: nodes.id,
            line: nodes.sourceLine,
            col: nodes.sourceCol,
            start: nodes.sourceStart,
            path: files.path,
          })
          .from(nodes)
          .innerJoin(files, eq(files.id, nodes.fileId))
          .where(inArray(nodes.id, matchIds))
          .all();
        const byId = new Map(nodeRows.map((r) => [r.id, r]));
        for (const m of matches) {
          const nr = byId.get(m.atNodeId);
          if (!nr) continue;
          const previewRows = db
            .select({ preview: prePostDiff.preTextPreview })
            .from(prePostDiff)
            .where(
              and(
                eq(prePostDiff.preStart, nr.start),
                eq(prePostDiff.filePath, nr.path),
                eq(prePostDiff.changeKind, "unchanged"),
              ),
            )
            .limit(1)
            .all();
          allMatches.push({
            project,
            bugId: candidate.source.bugId,
            file: nr.path.replace(scratch + "/", ""),
            line: nr.line,
            col: nr.col,
            textPreview: previewRows[0]?.preview ?? "",
          });
        }
      }
    } catch (err) {
      ps.errorCount += 1;
      if (process.env["DEBUG"]) {
        console.log(`  ERR ${project}/Bug-${candidate.source.bugId}: ${(err as Error).message}`);
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  }
  console.log(
    `  processed=${ps.candidatesProcessed} matched_candidates=${ps.candidatesWithMatch} total_matches=${ps.matchCount} errors=${ps.errorCount}`,
  );
}

console.log("\n=== Corpus summary ===");
let totalMatched = 0, totalProcessed = 0, totalMatches = 0, totalErrors = 0;
for (const [project, ps] of stats) {
  console.log(
    `${project.padEnd(15)} processed=${String(ps.candidatesProcessed).padStart(4)} matched_candidates=${String(ps.candidatesWithMatch).padStart(4)} total_matches=${String(ps.matchCount).padStart(4)} errors=${String(ps.errorCount).padStart(3)}`,
  );
  totalMatched += ps.candidatesWithMatch;
  totalProcessed += ps.candidatesProcessed;
  totalMatches += ps.matchCount;
  totalErrors += ps.errorCount;
}
console.log(
  `${"TOTAL".padEnd(15)} processed=${String(totalProcessed).padStart(4)} matched_candidates=${String(totalMatched).padStart(4)} total_matches=${String(totalMatches).padStart(4)} errors=${String(totalErrors).padStart(3)}`,
);

const outPath = join(__dirname, "..", ".provekit", "or-chain-mining-results.json");
writeFileSync(
  outPath,
  JSON.stringify({ summary: Object.fromEntries(stats), matches: allMatches }, null, 2),
  "utf-8",
);
console.log(`\nMatches dumped to: ${outPath}`);
console.log(`(read it to label TP/FP and compute precision)`);
