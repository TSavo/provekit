/**
 * C6 SQL-error isolation harness.
 *
 * Bypasses the 25-minute fix-loop pipeline. Takes DSL source on stdin
 * (or via --src) and runs the full DSL → AST → compileProgram → SQL →
 * sqlite.prepare() chain. Surfaces any "near 'X': syntax error" with the
 * exact failing SQL printed.
 *
 * Usage:
 *   echo "$DSL" | npx tsx scratch/c6-repro.ts
 *   npx tsx scratch/c6-repro.ts --fixture div-by-zero
 *   PROVEKIT_SQL_TRACE=1 npx tsx scratch/c6-repro.ts < my-dsl.txt
 *
 * Exit codes:
 *   0 — all principles compiled and prepared cleanly
 *   1 — at least one principle's SQL failed to prepare (bug reproduced)
 *   2 — DSL parse or compile-time error (separate bug class)
 */

import { readFileSync } from "fs";
import { parseDSL } from "../src/dsl/parser.js";
import { compileProgram } from "../src/dsl/compiler.js";
import { _clearRegistry } from "../src/sast/capabilityRegistry.js";
import { registerAll } from "../src/sast/schema/capabilities/index.js";
import { openDb } from "../src/db/index.js";
import { mkdtempSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

const FIXTURES: Record<string, string> = {
  "div-by-zero": `
predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) before $div
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
  `.trim(),

  // A principle that contains "SQL" as a literal — first guess at what
  // the LLM might have emitted. The DSL grammar permits string literals
  // inside `where ... == "..."`, so this should compile but may produce
  // surprising SQL. (Currently fails at compile-time: 'ts' isn't a
  // registered capability.)
  "sql-string-literal": `
principle sql-mention {
  match $n: node where ts.kind == "SQL"
  report violation {
    at $n
    captures { node: $n }
    message "node references SQL"
  }
}
  `.trim(),

  // Probe: callee_name is a Text column on calls — does the compiler
  // emit valid SQL when the literal contains a SQL keyword?
  "calls-named-sql": `
principle calls-sql {
  match $c: node where calls.callee_name == "SQL"
  report violation {
    at $c
    captures { call: $c }
    message "calls SQL"
  }
}
  `.trim(),

  // Probe: capture name that resembles a SQL keyword. Compiler builds
  // alias __cap_<name> — if cap.name is "SQL" the alias becomes
  // __cap_SQL which is a valid identifier; should compile fine, but
  // worth verifying.
  "capture-named-sql": `
principle capture-sql {
  match $div: node where arithmetic.op == "/"
  report violation {
    at $div
    captures { SQL: $div }
    message "capture named SQL"
  }
}
  `.trim(),

  // Probe: predicate with SQL-ish names. Predicates inline; the
  // generated SQL incorporates predicate body atoms with rebound
  // variables.
  "predicate-named-sql": `
predicate SQL($var: node) {
  match $g: node where narrows.target_node == $var
}

principle uses-sql-predicate {
  match $div: node where arithmetic.op == "/"
  require no $g: SQL($div.arithmetic.rhs_node)
  report violation {
    at $div
    captures { division: $div }
    message "no guard for division"
  }
}
  `.trim(),

  // Probe: principle name resembling a SQL keyword. The compiler uses
  // principle.name only for error messages; should not appear in SQL.
  "principle-named-sql": `
principle SQL {
  match $div: node where arithmetic.op == "/"
  report violation {
    at $div
    captures { division: $div }
    message "principle named SQL"
  }
}
  `.trim(),

  // Probe: variable name resembling a SQL keyword. Variable names
  // become parts of table aliases like cap_<cap>_<varname>; aliases
  // may or may not be quoted.
  "variable-named-sql": `
principle var-sql {
  match $SQL: node where arithmetic.op == "/"
  report violation {
    at $SQL
    captures { division: $SQL }
    message "variable named SQL"
  }
}
  `.trim(),
};

function readSource(): string {
  const args = process.argv.slice(2);
  const fixIdx = args.indexOf("--fixture");
  if (fixIdx >= 0) {
    const name = args[fixIdx + 1];
    if (!name || !FIXTURES[name]) {
      console.error(`Unknown fixture: ${name}. Known: ${Object.keys(FIXTURES).join(", ")}`);
      process.exit(2);
    }
    return FIXTURES[name];
  }
  const srcIdx = args.indexOf("--src");
  if (srcIdx >= 0) {
    const path = args[srcIdx + 1];
    if (!path) {
      console.error("--src requires a path");
      process.exit(2);
    }
    return readFileSync(path, "utf-8");
  }
  // Default: read stdin
  return readFileSync(0, "utf-8");
}

function openTestDb() {
  const dir = mkdtempSync(join(tmpdir(), "c6-repro-"));
  const dbPath = join(dir, "provekit.db");
  const db = openDb(dbPath);
  // Apply provekit's migrations so the substrate tables exist.
  const migrationsFolder = join(__dirname, "..", "drizzle");
  migrate(db, { migrationsFolder });
  return { db, dbPath };
}

async function main() {
  const src = readSource();
  console.error(`[c6-repro] DSL source (${src.length} chars):\n---\n${src}\n---`);

  // Register all built-in capabilities (otherwise compilePrinciple throws
  // "unknown capability" before we even get to SQL).
  _clearRegistry();
  registerAll();

  let program;
  try {
    program = parseDSL(src);
  } catch (err) {
    console.error(`[c6-repro] PARSE FAILED: ${(err as Error).message}`);
    process.exit(2);
  }
  console.error(`[c6-repro] parsed ${program.nodes.length} top-level node(s)`);

  let queries;
  try {
    queries = compileProgram(program.nodes);
  } catch (err) {
    console.error(`[c6-repro] COMPILE FAILED: ${(err as Error).message}`);
    process.exit(2);
  }
  console.error(`[c6-repro] compiled ${queries.size} principle(s)`);

  // Open a substrate DB so we can actually prepare the queries.
  const { db, dbPath } = openTestDb();
  console.error(`[c6-repro] substrate DB at ${dbPath}`);

  let anyFailed = false;
  for (const [name, query] of queries) {
    const sql = (query as any).__sql as string;
    console.error(`\n[c6-repro] === principle: ${name} ===`);
    console.error(`[c6-repro] generated SQL:\n${sql}`);
    try {
      query(db); // exercise the full prepare()+all() path
      console.error(`[c6-repro] OK (prepare + execute succeeded)`);
    } catch (err) {
      anyFailed = true;
      console.error(`[c6-repro] FAILED: ${(err as Error).message}`);
      console.error(`[c6-repro] failing SQL was:\n${sql}`);
    }
  }

  process.exit(anyFailed ? 1 : 0);
}

main().catch((err) => {
  console.error(`[c6-repro] uncaught: ${(err as Error).message}`);
  process.exit(3);
});
