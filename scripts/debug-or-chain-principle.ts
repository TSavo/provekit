#!/usr/bin/env tsx
/**
 * End-to-end smoke for hard-bug 1 Day 3: build SAST from a synthetic
 * buggy file, record the diff against its fix, set the active diff
 * context, run the `or-chain-extended-by-fix` principle, assert it
 * fires on the inner OR. Then clear the active context and assert it
 * stops firing: the dormant-without-context contract.
 */
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { dirname, join } from "path";
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
import { parseDSL } from "../src/dsl/parser.js";
import { compileProgram } from "../src/dsl/compiler.js";
import "../src/dsl/relations.js"; // self-registers built-ins
import type { HarvestCandidate } from "../src/fix/harvest/extractBugs.js";
import { readFileSync } from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const migrationsFolder = join(__dirname, "..", "drizzle");

const scratch = mkdtempSync(join(tmpdir(), "provekit-day3-smoke-"));
const dbPath = join(scratch, "test.db");
const db = openDb(dbPath);
migrate(db, { migrationsFolder });

// 1. Write the buggy file to disk and build SAST against it. (The mining
//    pipeline does the same per-candidate.)
const bugFile = join(scratch, "src", "check.ts");
mkdirSync(dirname(bugFile), { recursive: true });
const buggySource = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`;
const fixedSource = `function check(t: string): boolean {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`;
writeFileSync(bugFile, buggySource, "utf-8");
buildSASTForFile(db, bugFile);

// 2. Record the diff between buggy and fixed sources. Use the absolute
//    file path as the key so it matches the files row's path. The
//    HarvestCandidate's file paths are conventionally absolute in the
//    real pipeline (writeFixturesToDir copies into the scratch project root).
const candidate: HarvestCandidate = {
  source: {
    project: "synthetic",
    bugId: "1",
    baseSha: "deadbeef",
    fixSha: "f1xc0de",
    testSha: null,
    originalSha: null,
  },
  buggyFiles: { [bugFile]: buggySource },
  fixedFiles: { [bugFile]: fixedSource },
  diff: "",
  upstreamFixMessage: "",
  testFiles: {},
  stats: { filesChanged: 1, insertions: 1, deletions: 0 },
};
recordCandidateDiff(db, candidate);
setActiveCandidate(db, "synthetic", "1");

// 3. Compile the principle and run it.
const dslSource = readFileSync(
  join(__dirname, "..", ".provekit", "principles", "or-chain-extended-by-fix.dsl"),
  "utf-8",
);
const program = parseDSL(dslSource);
const compiled = compileProgram(program.nodes);
const principle = compiled.get("or-chain-extended-by-fix");
if (!principle) {
  console.error("FAIL: principle not compiled");
  process.exit(1);
}

if (process.env["DEBUG"]) {
  console.log("Generated SQL:");
  console.log((principle as any).__sql);
  console.log();
}

const matches = principle(db);
console.log(`Active context: matches = ${matches.length}`);
for (const m of matches) {
  console.log(`  match: ${JSON.stringify(m)}`);
}
if (matches.length === 0) {
  console.log("FAIL: expected at least one match on the buggy 2-clause OR");
  process.exit(1);
}

// 4. Clear active context: relation must now report false → no matches.
clearActiveDiffContext(db);
const noMatches = principle(db);
console.log(`Cleared context: matches = ${noMatches.length}`);
if (noMatches.length !== 0) {
  console.log("FAIL: relation should be dormant without active context");
  process.exit(1);
}

// 5. Negative cases: build fresh scratch DBs so the file_id doesn't collide.
//    These verify the false-positive control: relation must not fire when
//    the buggy OR isn't structurally enclosed by an added post node.
function checkNegativeCase(name: string, buggy: string, fixed: string): boolean {
  const ns = mkdtempSync(join(tmpdir(), "provekit-day3-neg-"));
  const ndbPath = join(ns, "test.db");
  const ndb = openDb(ndbPath);
  migrate(ndb, { migrationsFolder });
  const nbug = join(ns, "src", "check.ts");
  mkdirSync(dirname(nbug), { recursive: true });
  writeFileSync(nbug, buggy, "utf-8");
  buildSASTForFile(ndb, nbug);
  recordCandidateDiff(ndb, {
    source: {
      project: "synthetic", bugId: "neg", baseSha: "x", fixSha: "y",
      testSha: null, originalSha: null,
    },
    buggyFiles: { [nbug]: buggy },
    fixedFiles: { [nbug]: fixed },
    diff: "", upstreamFixMessage: "", testFiles: {},
    stats: { filesChanged: 1, insertions: 0, deletions: 0 },
  });
  setActiveCandidate(ndb, "synthetic", "neg");
  const m = principle(ndb);
  rmSync(ns, { recursive: true, force: true });
  if (m.length !== 0) {
    console.log(`FAIL: ${name}: principle should not fire (got ${m.length})`);
    return false;
  }
  console.log(`PASS: ${name}: no spurious match (${m.length})`);
  return true;
}

let allPass = true;
allPass = checkNegativeCase(
  "OR unchanged across unrelated fix",
  `function check(t: string): boolean {
  console.log("checking");
  return t === "Foo" || t === "Bar";
}`,
  `function check(t: string): boolean {
  console.warn("checking");
  return t === "Foo" || t === "Bar";
}`,
) && allPass;
allPass = checkNegativeCase(
  "OR completely replaced",
  `function check(t: string): boolean {
  return t === "Foo" || t === "Bar";
}`,
  `function check(t: string): boolean {
  return ["Foo", "Bar", "Baz"].includes(t);
}`,
) && allPass;

rmSync(scratch, { recursive: true, force: true });
console.log();
if (!allPass) {
  console.log("FAIL: negative cases regressed");
  process.exit(1);
}
console.log(`PASS: or-chain-extended-by-fix: positive + dormant + 2 negative cases`);
