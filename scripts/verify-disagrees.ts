#!/usr/bin/env tsx
/**
 * #115 step 2.5 verification: replay the 11 manual-30 disagrees against
 * the tightened principles and confirm the fix.
 *
 * Each disagree is a real BugsJS bug where one of the broad principles
 * (addition-overflow, subtraction-underflow, multiplication-overflow,
 * falsy-default) misfired in the prior gate run. With the tightened
 * versions (operand-type filter + is_in_dirty_set), those misfires
 * should now be suppressed.
 *
 * Outcome: counts of how many of the 11 still fire vs no longer fire.
 * 0/11 fires = the gate's recognized-stratum failure mode is fully
 * absorbed by the tightening; >0 means residual failure modes need
 * another iteration.
 */
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "fs";
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
import "../src/sast/schema/capabilities/index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");

// The 11 manual-30 disagrees from .provekit/manual-sample-30-summary.md
const DISAGREES: Array<{ project: string; bugId: string; principle: string; reason: string }> = [
  // String concat misfires (operand-type)
  { project: "karma", bugId: "22", principle: "addition-overflow", reason: "regex string assembly" },
  { project: "eslint", bugId: "51", principle: "addition-overflow", reason: "regex sentinel split" },
  { project: "eslint", bugId: "64", principle: "addition-overflow", reason: "fixer text concat" },
  { project: "hexo", bugId: "12", principle: "addition-overflow", reason: "regex string concat" },
  { project: "eslint", bugId: "49", principle: "addition-overflow", reason: "fixer string concat" },
  // Stable-at-locus misfires (dirty-set)
  { project: "eslint", bugId: "232", principle: "addition-overflow", reason: "null-guard fix; arithmetic stable" },
  { project: "eslint", bugId: "232", principle: "multiplication-overflow", reason: "null-guard fix; * stable" },
  { project: "eslint", bugId: "246", principle: "addition-overflow", reason: "comment scoping; + stable" },
  { project: "eslint", bugId: "60", principle: "multiplication-overflow", reason: "missing addend; * stable" },
  { project: "eslint", bugId: "182", principle: "subtraction-underflow", reason: "block comment guard; - stable" },
  { project: "hessian.js", bugId: "6", principle: "subtraction-underflow", reason: "synthetic key filter; - stable" },
  { project: "express", bugId: "21", principle: "falsy-default", reason: "save/restore wrap; || stable" },
];

const principleSrc = (name: string) =>
  readFileSync(join(__dirname, "..", ".provekit", "principles", `${name}.dsl`), "utf-8");

function compile(name: string) {
  const program = parseDSL(principleSrc(name));
  const map = compileProgram(program.nodes);
  const principle = map.get(name);
  if (!principle) throw new Error(`failed to compile ${name}`);
  return principle;
}

const compiled = new Map<string, ReturnType<typeof compile>>();
for (const p of new Set(DISAGREES.map((d) => d.principle))) {
  compiled.set(p, compile(p));
}

let suppressed = 0;
let stillFires = 0;
let extractError = 0;
for (const d of DISAGREES) {
  const projectPath = `/Users/tsavo/bugsjs/${d.project}`;
  let candidate;
  try {
    const ext = extractBugs({
      projectPath,
      project: d.project,
      onlyBugIds: [d.bugId],
      maxFiles: 999,
      maxLoc: 99999,
    });
    candidate = ext.candidates[0];
    if (!candidate) {
      console.log(`SKIP ${d.project}/Bug-${d.bugId}: candidate not found (${ext.skipped[0]?.reason})`);
      extractError++;
      continue;
    }
  } catch (err) {
    console.log(`ERROR ${d.project}/Bug-${d.bugId}: ${(err as Error).message}`);
    extractError++;
    continue;
  }

  const scratch = mkdtempSync(join(tmpdir(), `verify-${d.project}-${d.bugId}-`));
  try {
    const db = openDb(join(scratch, "test.db"));
    migrate(db, { migrationsFolder });

    const remappedBuggy: Record<string, string> = {};
    const remappedFixed: Record<string, string> = {};
    for (const relPath of Object.keys(candidate.buggyFiles)) {
      if (!/\.(?:js|jsx|ts|tsx|mjs|cjs)$/.test(relPath)) continue;
      const abs = join(scratch, relPath);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, candidate.buggyFiles[relPath]!, "utf-8");
      try { buildSASTForFile(db, abs); } catch { /* parse errors non-fatal */ }
      remappedBuggy[abs] = candidate.buggyFiles[relPath]!;
      if (candidate.fixedFiles[relPath] !== undefined) {
        remappedFixed[abs] = candidate.fixedFiles[relPath]!;
      }
    }
    recordCandidateDiff(db, { ...candidate, buggyFiles: remappedBuggy, fixedFiles: remappedFixed });
    setActiveCandidate(db, d.project, d.bugId);

    const principle = compiled.get(d.principle)!;
    const matches = principle(db);
    if (matches.length === 0) {
      console.log(`SUPPRESSED  ${d.project}/Bug-${d.bugId} ${d.principle}: ${d.reason}`);
      suppressed++;
    } else {
      console.log(`STILL_FIRES ${d.project}/Bug-${d.bugId} ${d.principle}  matches=${matches.length}: ${d.reason}`);
      stillFires++;
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

console.log();
console.log(`Summary: ${suppressed}/${DISAGREES.length} disagrees now suppressed, ${stillFires} still fire, ${extractError} extract errors`);
console.log(`Improvement: ${suppressed} / ${DISAGREES.length} = ${((suppressed / DISAGREES.length) * 100).toFixed(0)}%`);
console.log();
console.log(`If 11/12 suppressed, the gate-relevant precision goes from 16/30 (53%)`);
console.log(`to ~27/30 (90%) on a re-tag. Manual re-sample should confirm.`);
