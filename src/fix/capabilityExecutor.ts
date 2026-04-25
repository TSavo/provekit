/**
 * Oracle #16 full execution: transpile + run extractor against scratch DB.
 *
 * Safety note: the transpiled extractor executes in the SAME process as this
 * code. A malicious LLM-generated extractor could crash the process, exfil
 * data, etc. For MVP this is accepted risk — oracles #14/#17 have already
 * validated the spec before #16 executes; #18 confirms the capability is
 * load-bearing.
 *
 * Tmpfiles are always cleaned up in try/finally.
 *
 * The tmpdir is placed under <projectRoot>/node_modules/.cache/ so that
 * Node's CommonJS require() resolution walks up and finds the project's
 * node_modules. A tmpdir under os.tmpdir() would cause MODULE_NOT_FOUND on
 * every import inside the transpiled extractor.
 */

import * as ts from "typescript";
import { mkdtempSync, writeFileSync, rmSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath, pathToFileURL } from "url";
import { openDb } from "../db/index.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { buildSASTAndReturnHandles } from "../sast/builder.js";
import type { CapabilitySpec } from "./types.js";

// ESM-compatible __dirname (vitest runs TS directly in ESM context)
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface OracleResult {
  passed: boolean;
  detail: string;
}

interface FixtureResult {
  label: string;
  passed: boolean;
  rowCount: number;
  expectedMin: number; // 0 means must be zero
  error?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Transpile TypeScript source to CJS JavaScript in-memory. */
function transpileTs(source: string): string {
  const result = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
      skipLibCheck: true,
    },
  });
  return result.outputText;
}

/**
 * Detect the exported function name from extractorTs.
 * Uses a simple regex: first `export function <Name>(`.
 * Falls back to `extract` + PascalCase capabilityName.
 */
function detectExtractorFunctionName(extractorTs: string, capabilityName: string): string {
  const match = /export\s+function\s+(\w+)\s*\(/.exec(extractorTs);
  if (match && match[1]) return match[1];
  // Fallback: extract + CapabilityName (capitalised first char)
  return "extract" + capabilityName.charAt(0).toUpperCase() + capabilityName.slice(1);
}

/**
 * Resolve the project root directory — the directory containing package.json.
 * The source file lives at src/fix/capabilityExecutor.ts, so going up two
 * directories from __dirname reaches the project root.
 */
function resolveProjectRoot(): string {
  return join(__dirname, "..", "..");
}

// ---------------------------------------------------------------------------
// Core executor
// ---------------------------------------------------------------------------

/**
 * executeExtractorSpec:
 *   1. Opens a fresh scratch SQLite DB in a tmpfile.
 *   2. Runs drizzle migrations against the scratch DB (creates files, nodes, etc.).
 *   3. Applies spec.migrationSql on top (adds the new capability table).
 *   4. Transpiles extractorTs to CJS and dynamic-imports it.
 *   5. For each positive fixture: builds SAST, calls extractor, checks row count >= expectedRowCount.
 *   6. For each negative fixture: builds SAST, calls extractor, checks row count == 0.
 *   7. Cleans up tmpfiles in try/finally.
 */
export async function executeExtractorSpec(
  spec: CapabilitySpec,
  options?: { cacheDirOverride?: string },
): Promise<OracleResult> {
  const projectRoot = resolveProjectRoot();
  // cacheDirOverride lets tests scope tmpdir creation to a per-test directory
  // so their cleanup-verification (counting provekit-extractor-* dirs) isn't
  // poisoned by concurrent test runs creating their own dirs in the shared
  // cache. Production callers omit it.
  const cacheDir = options?.cacheDirOverride ?? join(projectRoot, "node_modules", ".cache");

  // Ensure cache dir exists (created on first use)
  mkdirSync(cacheDir, { recursive: true });

  const tmpDir = mkdtempSync(join(cacheDir, "provekit-extractor-"));

  try {
    // 1. Open scratch DB
    const dbPath = join(tmpDir, "scratch.db");
    const scratchDb = openDb(dbPath);

    // 2. Run core SAST migrations (files, nodes, node_children + capability tables)
    migrate(scratchDb, { migrationsFolder: join(projectRoot, "drizzle") });

    // 3. Apply spec's own migration (CREATE TABLE for the new capability)
    //    Execute via raw SQLite client since migrationSql may contain multiple statements.
    const stmts = spec.migrationSql
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    for (const stmt of stmts) {
      scratchDb.$client.exec(stmt);
    }

    // 4. Transpile extractorTs to CJS
    let transpiled: string;
    try {
      transpiled = transpileTs(spec.extractorTs);
    } catch (err) {
      return {
        passed: false,
        detail: `Oracle #16: transpile failed: ${err instanceof Error ? err.message : String(err)}`,
      };
    }

    // Write transpiled JS next to the DB so require() can resolve project modules
    const jsPath = join(tmpDir, "extractor.cjs");
    writeFileSync(jsPath, transpiled);

    // 5. Dynamic-import the extractor module
    let extractorFn: (...args: unknown[]) => void;
    try {
      const mod = await import(pathToFileURL(jsPath).href) as Record<string, unknown>;
      const fnName = detectExtractorFunctionName(spec.extractorTs, spec.capabilityName);
      const candidate = mod[fnName] ?? mod["default"];
      if (typeof candidate !== "function") {
        return {
          passed: false,
          detail: `Oracle #16: exported function '${fnName}' not found in transpiled extractor`,
        };
      }
      extractorFn = candidate as (...args: unknown[]) => void;
    } catch (err) {
      return {
        passed: false,
        detail: `Oracle #16: dynamic import failed: ${err instanceof Error ? err.message : String(err)}`,
      };
    }

    // Helper: get row count from the new capability table
    const capTable = deriveTableName(spec.migrationSql);
    function rowCount(): number {
      const rows = scratchDb.$client.prepare(`SELECT COUNT(*) AS cnt FROM "${capTable}"`).get() as { cnt: number };
      return rows.cnt;
    }

    // Helper: clear capability table between fixtures
    function clearCapTable(): void {
      scratchDb.$client.exec(`DELETE FROM "${capTable}"`);
    }

    // Helper: write fixture source to a tmpfile and build SAST
    let fixtureIndex = 0;

    async function runFixture(source: string): Promise<{
      error?: string;
      rowsInserted: number;
    }> {
      const fixturePath = join(tmpDir, `fixture_${fixtureIndex++}.ts`);
      writeFileSync(fixturePath, source);

      // Clear any rows from previous fixture runs
      clearCapTable();

      try {
        const { sourceFile, nodeIdByNode } = buildSASTAndReturnHandles(scratchDb, fixturePath);

        // Run extractor inside a transaction — rollback on error
        let extractError: Error | undefined;
        try {
          scratchDb.transaction((tx) => {
            extractorFn(tx, sourceFile, nodeIdByNode);
          });
        } catch (e) {
          extractError = e instanceof Error ? e : new Error(String(e));
        }

        if (extractError) {
          return { error: extractError.message, rowsInserted: 0 };
        }

        return { rowsInserted: rowCount() };
      } catch (err) {
        return {
          error: err instanceof Error ? err.message : String(err),
          rowsInserted: 0,
        };
      }
    }

    // 6. Run positive fixtures
    const positiveResults: FixtureResult[] = [];
    for (let i = 0; i < spec.positiveFixtures.length; i++) {
      const fixture = spec.positiveFixtures[i]!;
      const { error, rowsInserted } = await runFixture(fixture.source);
      const passed = !error && rowsInserted >= fixture.expectedRowCount;
      positiveResults.push({
        label: `positive[${i}]`,
        passed,
        rowCount: rowsInserted,
        expectedMin: fixture.expectedRowCount,
        error,
      });
    }

    // 7. Run negative fixtures
    const negativeResults: FixtureResult[] = [];
    for (let i = 0; i < spec.negativeFixtures.length; i++) {
      const fixture = spec.negativeFixtures[i]!;
      const { error, rowsInserted } = await runFixture(fixture.source);
      const passed = !error && rowsInserted === 0;
      negativeResults.push({
        label: `negative[${i}]`,
        passed,
        rowCount: rowsInserted,
        expectedMin: 0,
        error,
      });
    }

    // 8. Aggregate
    const allPassed =
      positiveResults.every((r) => r.passed) &&
      negativeResults.every((r) => r.passed);

    const failureDetails: string[] = [];
    for (const r of [...positiveResults, ...negativeResults]) {
      if (!r.passed) {
        if (r.error) {
          failureDetails.push(`  ${r.label}: error — ${r.error}`);
        } else if (r.expectedMin === 0) {
          failureDetails.push(`  ${r.label}: expected 0 rows, got ${r.rowCount}`);
        } else {
          failureDetails.push(
            `  ${r.label}: expected >= ${r.expectedMin} rows, got ${r.rowCount}`,
          );
        }
      }
    }

    const positivePass = positiveResults.filter((r) => r.passed).length;
    const negativePass = negativeResults.filter((r) => r.passed).length;

    const detail = [
      `positive fixtures: ${positivePass}/${positiveResults.length}`,
      `negative fixtures: ${negativePass}/${negativeResults.length}`,
      ...failureDetails,
    ].join("\n");

    return { passed: allPassed, detail };
  } finally {
    // Cleanup tmpfiles regardless of success/failure
    try {
      rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // Best-effort cleanup — don't mask the real error
    }
  }
}

// ---------------------------------------------------------------------------
// Internal: derive table name from migrationSql
// ---------------------------------------------------------------------------

/**
 * Extract the first table name from a CREATE TABLE statement.
 * Handles both `CREATE TABLE foo (...)` and `CREATE TABLE IF NOT EXISTS foo (...)`.
 */
function deriveTableName(migrationSql: string): string {
  const match = /CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?["']?(\w+)["']?/i.exec(migrationSql);
  if (match && match[1]) return match[1];
  throw new Error(`Oracle #16: cannot derive table name from migrationSql: ${migrationSql.slice(0, 100)}`);
}
