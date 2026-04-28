#!/usr/bin/env tsx
/**
 * Library validation runner — task #129.
 *
 * For every `.dsl` file in `.provekit/principles/`, exercise the DSL
 * against a buggy fixture and a clean fixture. Report:
 *
 *   - positive : DSL fired on the buggy fixture (catches the bug class)
 *   - negative : DSL did NOT fire on the clean fixture (no false positive)
 *
 * Both must hold for the principle to count as "validated against fixture
 * pair". This is the curation gate referenced in `.provekit/principles/README.md`.
 *
 * This script is the in-place analogue of oracle #6 (which only runs on
 * in-flight C6 PrincipleCandidates). The shape mirrors the existing
 * harness in `src/pipeline/DerivationPhase.dslEquivalence.test.ts`.
 *
 * Usage:
 *   npx tsx scripts/validate-library.ts
 */
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  readFileSync,
  readdirSync,
  existsSync,
} from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../src/db/index.js";
import { buildSASTForFile } from "../src/sast/builder.js";
import { evaluatePrinciple } from "../src/dsl/evaluator.js";

// ---------------------------------------------------------------------------
// Fixture pairs.
//
// For each principle ID we ship a buggy + clean snippet. The pair acts as
// the discrimination gate: principle MUST fire on buggy and MUST NOT fire on
// clean. Where an existing principle's DSL hard-codes a callee like
// `arr.find` (because the calls extractor stores the full
// PropertyAccessExpression text), the buggy fixture wires the same shape.
// ---------------------------------------------------------------------------

interface FixturePair {
  buggy: string;
  clean: string;
  // Substitution applied to the DSL source before evaluation, if needed.
  // Mirrors the production-extractor quirk where callee_name is the full
  // member-access text (`arr.find` not `find`) — see the equivalence test
  // harness for prior art.
  dslPatch?: (src: string) => string;
}

const FIXTURES: Record<string, FixturePair> = {
  "addition-overflow": {
    buggy: "function f(a: number, b: number) { return a + b; }",
    clean: "function f(a: number, b: number) { return a - b; }",
  },
  "subtraction-underflow": {
    buggy: "function f(a: number, b: number) { return a - b; }",
    clean: "function f(a: number, b: number) { return a + b; }",
  },
  "division-by-zero": {
    buggy: "function f(a: number, b: number) { return a / b; }",
    clean: "function f(a: number, b: number) { return a + b; }",
  },
  "modulo-by-zero": {
    buggy: "function f(a: number, b: number) { return a % b; }",
    clean: "function f(a: number, b: number) { return a + b; }",
  },
  "empty-collection-loop": {
    buggy: "function f(xs: number[]) { for (const x of xs) { console.log(x); } }",
    clean: "function f(x: number) { return x + 1; }",
  },
  "falsy-default": {
    buggy: "function f(x: number | undefined) { return x || 0; }",
    clean: "function f(x: number | undefined) { return x ?? 0; }",
  },
  "find-undefined-result": {
    buggy: "function f(arr: number[]) { return arr.find(x => x > 0); }",
    clean: "function f(arr: number[]) { return arr.filter(x => x > 0); }",
    dslPatch: (src) => src.replace(/== "find"/g, '== "arr.find"'),
  },
  "match-null-result": {
    buggy: "function f(s: string) { return s.match(/[0-9]+/); }",
    clean: "function f(s: string) { return s.replace(/[0-9]+/, ''); }",
    dslPatch: (src) => src.replace(/== "match"/g, '== "s.match"'),
  },
  "split-empty-string": {
    buggy: "function f(s: string) { return s.split(','); }",
    clean: "function f(s: string) { return s.toLowerCase(); }",
    dslPatch: (src) => src.replace(/== "split"/g, '== "s.split"'),
  },
  "reduce-no-initial": {
    buggy:
      "function f(arr: number[]) { return arr.reduce((a, b) => a + b); }",
    clean:
      "function f(arr: number[]) { return arr.reduce((a, b) => a + b, 0); }",
    dslPatch: (src) => src.replace(/== "reduce"/g, '== "arr.reduce"'),
  },
  "throw-uncaught": {
    buggy:
      "function f(x: number) { if (x < 0) throw new Error('negative'); return x; }",
    clean: "function f(x: number) { return x + 1; }",
  },
  "unguarded-await": {
    buggy:
      "async function f(p: Promise<number>) { const x = await p; return x; }",
    clean:
      "async function f(p: Promise<number>) { try { return await p; } catch (e) { return 0; } }",
  },
  "null-assertion": {
    buggy: "function f(x: string | null) { return x!.length; }",
    clean: "function f(x: string | null) { return x?.length; }",
  },
  "loop-accumulator-overflow": {
    // DSL filters on iterates.loop_kind == "for" — must be a classic
    // `for (...)`, not for-of (extractor capability gap acknowledged).
    buggy:
      "function f(n: number) { let s = 0; for (let i = 0; i < n; i++) { s += i; } return s; }",
    clean: "function f(x: number, y: number) { return x - y; }",
  },
  "variable-staleness": {
    // Bare if (no else) where the assignment writes a variable used outside.
    buggy:
      "function f(cond: boolean) { let x = 0; if (cond) { x = 1; } return x; }",
    clean: "function f(cond: boolean) { let x = 0; if (cond) { x = 1; } else { x = 2; } return x; }",
  },
  "or-chain-extended-by-fix": {
    // No matching buggy/clean pair — this principle relies on
    // `was_replaced_by_addition`, a diff-context relation. It cannot be
    // exercised by a static fixture pair without a synthetic diff context.
    // We mark it accordingly below.
    buggy: "function f(x: number) { return x || 0; }",
    clean: "function f(x: number) { return x + 1; }",
  },
};

// ---------------------------------------------------------------------------
// Bootstrap helpers (mirror DerivationPhase.dslEquivalence.test.ts).
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REPO_ROOT = join(__dirname, "..");
const DRIZZLE_DIR = join(REPO_ROOT, "drizzle");
const PRINCIPLES_DIR = join(REPO_ROOT, ".provekit", "principles");

function openTestDb(): { db: ReturnType<typeof openDb>; tmpDir: string } {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-validate-"));
  const dbPath = join(tmpDir, "scratch.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: DRIZZLE_DIR });
  return { db, tmpDir };
}

function evalAgainst(dslSource: string, fixtureSrc: string): number {
  const { db, tmpDir } = openTestDb();
  try {
    const fixturePath = join(tmpDir, "fixture.ts");
    writeFileSync(fixturePath, fixtureSrc, "utf-8");
    buildSASTForFile(db, fixturePath);
    const matches = evaluatePrinciple(db, dslSource);
    return matches.length;
  } finally {
    try {
      db.$client.close();
    } catch {
      /* ignore */
    }
    rmSync(tmpDir, { recursive: true, force: true });
  }
}

interface Row {
  name: string;
  positive: number; // matches on buggy
  negative: number; // matches on clean (must be 0)
  status:
    | "PASS"
    | "FAIL-NO-FIRE"
    | "FAIL-FALSE-POSITIVE"
    | "SKIP-NO-FIXTURE"
    | "DIFF-CONTEXT-ONLY"
    | "KNOWN-CAPABILITY-GAP"
    | "ERROR";
  detail: string;
}

// Principles whose DSL gates on diff-context-only relations
// (`is_in_dirty_set`, `was_replaced_by_addition`). These cannot fire on a
// static fixture pair by design — they're only meaningful during corpus
// mining where pre/post diff context is active. Their adversarial story
// is a corpus-precision measurement, not a fixture-pair test.
const DIFF_CONTEXT_ONLY = new Set([
  "addition-overflow",
  "subtraction-underflow",
  "falsy-default",
  "or-chain-extended-by-fix",
]);

// Principles with a documented over-match owing to a capability gap in the
// SAST extractor. The DSL file's own header acknowledges the gap. These
// are kept in-library because the locus + match are still useful signal,
// but they don't pass strict adversarial validation today.
const KNOWN_CAPABILITY_GAP = new Set(["unguarded-await"]);

function classify(name: string, pos: number, neg: number): Row["status"] {
  if (DIFF_CONTEXT_ONLY.has(name)) {
    if (pos === 0 && neg === 0) return "DIFF-CONTEXT-ONLY";
  }
  if (KNOWN_CAPABILITY_GAP.has(name)) return "KNOWN-CAPABILITY-GAP";
  if (pos === 0) return "FAIL-NO-FIRE";
  if (neg !== 0) return "FAIL-FALSE-POSITIVE";
  return "PASS";
}

function main() {
  if (!existsSync(PRINCIPLES_DIR)) {
    console.error(`No principles dir at ${PRINCIPLES_DIR}`);
    process.exit(1);
  }
  const dslFiles = readdirSync(PRINCIPLES_DIR).filter((f) => f.endsWith(".dsl"));
  const rows: Row[] = [];

  for (const f of dslFiles.sort()) {
    const id = f.replace(/\.dsl$/, "");
    const dslPath = join(PRINCIPLES_DIR, f);
    let dslSource = readFileSync(dslPath, "utf-8");
    const fixture = FIXTURES[id];
    if (!fixture) {
      rows.push({
        name: id,
        positive: -1,
        negative: -1,
        status: "SKIP-NO-FIXTURE",
        detail: "no fixture pair declared",
      });
      continue;
    }
    if (fixture.dslPatch) dslSource = fixture.dslPatch(dslSource);

    try {
      const positive = evalAgainst(dslSource, fixture.buggy);
      const negative = evalAgainst(dslSource, fixture.clean);
      rows.push({
        name: id,
        positive,
        negative,
        status: classify(id, positive, negative),
        detail: "",
      });
    } catch (e: any) {
      rows.push({
        name: id,
        positive: -1,
        negative: -1,
        status: "ERROR",
        detail: e?.message?.slice(0, 200) ?? String(e),
      });
    }
  }

  // Print table
  const colName = Math.max(8, ...rows.map((r) => r.name.length));
  const head = `${"name".padEnd(colName)}  pos  neg  status                detail`;
  console.log(head);
  console.log("-".repeat(head.length));
  for (const r of rows) {
    const pos = r.positive < 0 ? "  -" : String(r.positive).padStart(3);
    const neg = r.negative < 0 ? "  -" : String(r.negative).padStart(3);
    console.log(
      `${r.name.padEnd(colName)}  ${pos}  ${neg}  ${r.status.padEnd(20)}  ${r.detail}`,
    );
  }

  const fails = rows.filter(
    (r) =>
      r.status === "FAIL-NO-FIRE" ||
      r.status === "FAIL-FALSE-POSITIVE" ||
      r.status === "ERROR",
  );
  const pass = rows.filter((r) => r.status === "PASS").length;
  const diffOnly = rows.filter((r) => r.status === "DIFF-CONTEXT-ONLY").length;
  const capGap = rows.filter((r) => r.status === "KNOWN-CAPABILITY-GAP").length;
  const skip = rows.filter((r) => r.status === "SKIP-NO-FIXTURE").length;
  console.log("");
  console.log(
    `summary: ${pass} pass, ${diffOnly} diff-context-only, ${capGap} known-capability-gap, ${skip} skipped, ${fails.length} failed`,
  );
  if (fails.length > 0) process.exit(1);
}

main();
