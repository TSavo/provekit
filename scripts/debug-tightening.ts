#!/usr/bin/env tsx
/**
 * Smoke for #115 step 2.5: validates the two principle-tightening levers
 * fire correctly on synthetic versions of the manual-30 disagree fixtures.
 *
 *   1. is_in_dirty_set: arithmetic principle fires ONLY on `+` actually
 *      changed by the fix, not on stable `+` near the diff
 *   2. result_sort != "String": addition-overflow does NOT fire on string
 *      concatenation, even when the concat IS in the dirty set
 *
 * The synthetic fixtures mirror the real disagrees:
 *   - eslint Bug-232: bug is null-guard; existing arithmetic at locus is unchanged
 *   - karma Bug-22: bug adds string concat to a regex assembly
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
import { parseDSL } from "../src/dsl/parser.js";
import { compileProgram } from "../src/dsl/compiler.js";
import "../src/dsl/relations.js";
import "../src/sast/schema/capabilities/index.js";
import type { HarvestCandidate } from "../src/fix/harvest/extractBugs.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");

const principles: Record<string, string> = {
  "addition-overflow": readFileSync(join(__dirname, "..", ".provekit", "principles", "addition-overflow.dsl"), "utf-8"),
  "subtraction-underflow": readFileSync(join(__dirname, "..", ".provekit", "principles", "subtraction-underflow.dsl"), "utf-8"),
  "multiplication-overflow": readFileSync(join(__dirname, "..", ".provekit", "principles", "multiplication-overflow.dsl"), "utf-8"),
};

function compile(name: string) {
  const program = parseDSL(principles[name]!);
  const map = compileProgram(program.nodes);
  const principle = map.get(name);
  if (!principle) throw new Error(`failed to compile ${name}`);
  return principle;
}

interface TestCase {
  name: string;
  buggy: string;
  fixed: string;
  principle: string;
  expectMatches: number;
  description: string;
}

const cases: TestCase[] = [
  {
    name: "stable arithmetic at locus (bug-232 shape)",
    buggy: `function f(arr, parent) {
  if (arr[0].loc.start.line === parent.loc.start.line && arr[0].loc.end.line !== parent.loc.start.line) {
    return arr.length + 1;
  }
}`,
    fixed: `function f(arr, parent) {
  if (arr[0] && arr[0].loc.start.line === parent.loc.start.line && arr[0].loc.end.line !== parent.loc.start.line) {
    return arr.length + 1;
  }
}`,
    principle: "addition-overflow",
    expectMatches: 0,
    description: "addition is unchanged across fix; principle should not fire",
  },
  {
    name: "modified arithmetic (in dirty set)",
    buggy: `function f(n) {
  return n + 1;
}`,
    fixed: `function f(n) {
  return n + 2;
}`,
    principle: "addition-overflow",
    expectMatches: 1,
    description: "literal change makes the + node modified; should fire",
  },
  {
    name: "string concat (added but should not fire as addition-overflow)",
    buggy: `function urlRe(host) {
  return new RegExp("(?:https?:\\\\/\\\\/)?\\\\/?" + "(base/|absolute)");
}`,
    fixed: `function urlRe(host) {
  return new RegExp("(?:https?:\\\\/\\\\/" + host + ")?\\\\/?" + "(base/|absolute)");
}`,
    principle: "addition-overflow",
    expectMatches: 0,
    description: "+ in dirty set but result_sort=String; should not fire",
  },
  {
    name: "stable subtraction (bug-182 shape)",
    buggy: `function f(group) {
  const slice = group.length - 1;
  return slice;
}`,
    fixed: `function f(group) {
  const slice = group.length - 1;
  if (group.startsWith("/")) return null;
  return slice;
}`,
    principle: "subtraction-underflow",
    expectMatches: 0,
    description: "subtraction unchanged; principle should not fire",
  },
  {
    name: "stable multiplication (bug-60 shape)",
    buggy: `function indent(node, opts) {
  const inner = opts.size * opts.depth;
  return inner;
}`,
    fixed: `function indent(node, opts) {
  const inner = opts.size * opts.depth;
  return getNodeIndent(node).goodChar + inner;
}`,
    principle: "multiplication-overflow",
    expectMatches: 0,
    description: "multiplication unchanged; principle should not fire",
  },
];

let pass = 0;
let fail = 0;
for (const tc of cases) {
  const scratch = mkdtempSync(join(tmpdir(), `tightening-${tc.principle}-`));
  try {
    const dbPath = join(scratch, "test.db");
    const db = openDb(dbPath);
    migrate(db, { migrationsFolder });

    const file = join(scratch, "src", "x.ts");
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, tc.buggy, "utf-8");
    buildSASTForFile(db, file);

    const candidate: HarvestCandidate = {
      source: { project: "synth", bugId: "1", baseSha: "x", fixSha: "y", testSha: null, originalSha: null },
      buggyFiles: { [file]: tc.buggy },
      fixedFiles: { [file]: tc.fixed },
      diff: "", upstreamFixMessage: "", testFiles: {},
      stats: { filesChanged: 1, insertions: 0, deletions: 0 },
    };
    recordCandidateDiff(db, candidate);
    setActiveCandidate(db, "synth", "1");

    const principle = compile(tc.principle);
    const matches = principle(db);

    const ok = matches.length === tc.expectMatches;
    if (ok) {
      console.log(`PASS  ${tc.name}  matches=${matches.length}`);
      pass++;
    } else {
      console.log(`FAIL  ${tc.name}  expected=${tc.expectMatches} got=${matches.length}`);
      console.log(`  description: ${tc.description}`);
      console.log(`  principle: ${tc.principle}`);
      fail++;
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

console.log();
console.log(`${pass}/${pass + fail} passed`);
process.exit(fail > 0 ? 1 : 0);
