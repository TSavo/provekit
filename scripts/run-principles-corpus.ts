#!/usr/bin/env tsx
/**
 * Generic principle-runner for the BugsJS corpus. Wraps mine-or-chain's
 * scaffolding so any subset of principles can be evaluated against any
 * subset of projects, with per-project flushing (the lesson from the
 * full-corpus run that buffered everything to /dev/null until kill).
 *
 * Used by:
 *   - #115 step 4: tightening on train projects {eslint, express, karma}
 *   - #115 step 5: held-out measurement on test projects
 *   - Future precision/recall measurements as the library grows
 *
 * Output: NDJSON streamed to disk per candidate. Each line is
 *   {project, bugId, principle, matches: [{atNodeId, file, line, col, textPreview}]}
 * Missing rows mean "principle didn't fire on this candidate" (the
 * absence carries information for precision/recall computation).
 *
 * Diff context: any principle that uses diff-aware relations (e.g.
 * `was_replaced_by_addition`) reads from `diff_context_active`. The
 * runner sets the active candidate before each principle eval and
 * clears after. Static-only principles ignore this and work either way.
 *
 * Usage:
 *   tsx scripts/run-principles-corpus.ts \
 *     --principles or-chain-extended-by-fix,division-by-zero \
 *     --projects eslint,express \
 *     --out .provekit/results-train.ndjson \
 *     [--max-bugs 10]
 */
import { mkdtempSync, mkdirSync, writeFileSync, appendFileSync, readFileSync, rmSync, existsSync } from "fs";
import { dirname, join, basename } from "path";
import { tmpdir } from "os";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import {
  recordCandidateDiff,
  setActiveCandidate,
  clearActiveDiffContext,
} from "../src/fix/harvest/diff.js";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { parseDSL } from "../src/dsl/parser.js";
import { compileProgram, type CompiledPrincipleQuery } from "../src/dsl/compiler.js";
import "../src/dsl/relations.js";
import { eq, and, inArray } from "drizzle-orm";
import { nodes, files } from "../src/sast/schema/index.js";
import { prePostDiff } from "../src/db/schema/preDiff.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");
const principlesDir = join(__dirname, "..", ".provekit", "principles");

interface Args {
  principles: string[];
  projects: string[];
  out: string;
  maxBugs: number | undefined;
  bugsjsRoot: string;
}

function parseArgs(): Args {
  const argv = process.argv.slice(2);
  const out: Partial<Args> = { bugsjsRoot: "/Users/tsavo/bugsjs" };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[i + 1];
    if (a === "--principles") { out.principles = v!.split(","); i++; }
    else if (a === "--projects") { out.projects = v!.split(","); i++; }
    else if (a === "--out") { out.out = v; i++; }
    else if (a === "--max-bugs") { out.maxBugs = parseInt(v!, 10); i++; }
    else if (a === "--bugsjs-root") { out.bugsjsRoot = v; i++; }
    else throw new Error(`unknown flag: ${a}`);
  }
  if (!out.principles || !out.projects || !out.out) {
    throw new Error("required flags: --principles, --projects, --out");
  }
  return out as Args;
}

function loadAndCompile(principleNames: string[]): Map<string, CompiledPrincipleQuery> {
  const compiled = new Map<string, CompiledPrincipleQuery>();
  for (const name of principleNames) {
    const path = join(principlesDir, `${name}.dsl`);
    if (!existsSync(path)) throw new Error(`principle file not found: ${path}`);
    const program = parseDSL(readFileSync(path, "utf-8"));
    const map = compileProgram(program.nodes);
    for (const [pname, q] of map) {
      compiled.set(pname, q);
    }
  }
  return compiled;
}

function isJsFile(path: string): boolean {
  return /\.(?:js|jsx|ts|tsx|mjs|cjs)$/.test(path);
}

const args = parseArgs();
const compiled = loadAndCompile(args.principles);
console.log(`Loaded principles: ${[...compiled.keys()].join(", ")}`);
console.log(`Output stream: ${args.out}`);

// Truncate output file at start
writeFileSync(args.out, "", "utf-8");

const totals = { processed: 0, errors: 0, candidatesWithAnyMatch: 0, totalMatches: 0 };
const perProject = new Map<string, { processed: number; matched: number; matches: number; errors: number }>();

for (const project of args.projects) {
  const projectPath = join(args.bugsjsRoot, project);
  if (!existsSync(projectPath)) {
    console.log(`SKIP ${project}: not found at ${projectPath}`);
    continue;
  }
  const ps = { processed: 0, matched: 0, matches: 0, errors: 0 };
  perProject.set(project, ps);

  let extracted;
  try {
    extracted = extractBugs({
      projectPath,
      project,
      maxFiles: 2,
      maxLoc: 50,
      maxBugs: args.maxBugs,
    });
  } catch (err) {
    console.log(`FAIL ${project}: extract: ${(err as Error).message}`);
    ps.errors += 1;
    continue;
  }

  console.log(`\n=== ${project}: ${extracted.candidates.length} candidates (${extracted.skipped.length} filtered) ===`);

  for (const candidate of extracted.candidates) {
    const scratch = mkdtempSync(join(tmpdir(), `runner-${project}-${candidate.source.bugId}-`));
    try {
      const dbPath = join(scratch, "test.db");
      const db = openDb(dbPath);
      migrate(db, { migrationsFolder });

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

      recordCandidateDiff(db, {
        ...candidate,
        buggyFiles: remappedBuggy,
        fixedFiles: remappedFixed,
      });
      setActiveCandidate(db, project, candidate.source.bugId);

      let candidateAnyMatch = false;
      for (const [principleName, principle] of compiled) {
        const matches = principle(db);
        if (matches.length > 0) {
          candidateAnyMatch = true;
          ps.matches += matches.length;
          totals.totalMatches += matches.length;

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
          const out = matches.map((m) => {
            const nr = byId.get(m.atNodeId);
            const preview = nr
              ? (db
                  .select({ p: prePostDiff.preTextPreview })
                  .from(prePostDiff)
                  .where(
                    and(
                      eq(prePostDiff.preStart, nr.start),
                      eq(prePostDiff.filePath, nr.path),
                    ),
                  )
                  .limit(1)
                  .all()[0]?.p ?? "")
              : "";
            return {
              atNodeId: m.atNodeId,
              file: nr ? nr.path.replace(scratch + "/", "") : null,
              line: nr?.line ?? null,
              col: nr?.col ?? null,
              textPreview: preview,
            };
          });
          appendFileSync(
            args.out,
            JSON.stringify({
              project,
              bugId: candidate.source.bugId,
              principle: principleName,
              matches: out,
            }) + "\n",
          );
        }
      }
      if (candidateAnyMatch) ps.matched += 1;
      ps.processed += 1;
      totals.processed += 1;
    } catch (err) {
      ps.errors += 1;
      totals.errors += 1;
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  }
  console.log(
    `  processed=${ps.processed} matched=${ps.matched} total_matches=${ps.matches} errors=${ps.errors}`,
  );
}

console.log("\n=== Summary ===");
for (const [project, ps] of perProject) {
  console.log(
    `${project.padEnd(15)} processed=${String(ps.processed).padStart(4)} matched=${String(ps.matched).padStart(4)} matches=${String(ps.matches).padStart(4)} errors=${String(ps.errors).padStart(3)}`,
  );
}
console.log(
  `${"TOTAL".padEnd(15)} processed=${String(totals.processed).padStart(4)} candidates_with_any_match=${String([...perProject.values()].reduce((s, p) => s + p.matched, 0)).padStart(4)} total_matches=${String(totals.totalMatches).padStart(4)} errors=${String(totals.errors).padStart(3)}`,
);
console.log(`\nNDJSON: ${args.out}`);
