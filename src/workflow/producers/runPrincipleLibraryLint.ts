/**
 * RunPrincipleLibraryLint Stage — wraps the existing principle-library
 * scan (formerly runLint in src/cli.ts) as a workflow Stage.
 *
 * Walks projectRoot for .ts/.tsx files (skipping node_modules, dist,
 * test directories, etc.), builds the SAST per file in a scratch DB,
 * evaluates every .dsl in the principles directory, and returns the
 * principle_matches as a structured payload.
 *
 * Determinism / cacheability — same compromise as verifyProjectInvariants:
 * the Stage is "pure given the named projectRoot." Two runs with the
 * same on-disk content produce the same matches; if files change between
 * runs, the cache may be stale. This is intentional: the alternative
 * (hashing every file's content into the input) would defeat caching for
 * lint, where the developer expects edit-then-rerun. The cache miss is
 * keyed on the (projectRoot, principlesDir) pair; the consumer flushes
 * the cache by changing either.
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import { mkdtempSync, readdirSync, rmSync, readFileSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join, relative, resolve } from "path";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import type { Stage } from "../types.js";

export const RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY = "run-principle-library-lint";

export interface RunPrincipleLibraryLintInput {
  /** Absolute path to the project root being linted. */
  projectRoot: string;
  /**
   * Absolute path to the principles directory. Caller chooses local
   * (.provekit/principles) vs bundled per their resolution rules; the
   * Stage doesn't fall back. Keeping the choice external keeps the
   * Stage deterministic with respect to its declared input.
   */
  principlesDir: string;
  /** Absolute path to the drizzle migrations folder. */
  drizzleFolder: string;
  /** Emit per-file parser errors / per-principle errors to stderr. */
  verbose: boolean;
}

export interface PrincipleMatch {
  principleName: string;
  severity: string;
  message: string;
  sourceLine: number;
  path: string;
}

export interface RunPrincipleLibraryLintOutput {
  /** Every principle_match row, ordered by file path then source line. */
  matches: PrincipleMatch[];
  /** Files the SAST builder successfully indexed. */
  filesIndexed: number;
  /** Files the SAST builder rejected (parser failure). */
  parserFailures: number;
  /** Total .ts/.tsx files discovered (filesIndexed + parserFailures). */
  filesDiscovered: number;
  /** Principles that successfully evaluated. */
  principlesEvaluated: number;
  /** Principles that errored during evaluation. */
  principleErrors: number;
}

export interface MakeRunPrincipleLibraryLintStageDeps {
  /** Override producer identity. Default: "runPrincipleLibraryLint@v1". */
  producerVersion?: string;
}

export function makeRunPrincipleLibraryLintStage(
  deps: MakeRunPrincipleLibraryLintStageDeps = {},
): Stage<RunPrincipleLibraryLintInput, RunPrincipleLibraryLintOutput> {
  const producedBy = deps.producerVersion ?? "runPrincipleLibraryLint@v1";

  return {
    name: "runPrincipleLibraryLint",
    producedBy,

    serializeInput(input) {
      return {
        projectRoot: input.projectRoot,
        principlesDir: input.principlesDir,
        verbose: input.verbose,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as RunPrincipleLibraryLintOutput;
    },

    async run(input) {
      return runPrincipleLibraryLint(input);
    },
  };
}

export async function runPrincipleLibraryLint(
  input: RunPrincipleLibraryLintInput,
): Promise<RunPrincipleLibraryLintOutput> {
  const { evaluatePrinciple } = await import("../../dsl/evaluator.js");
  const { buildSASTForFile } = await import("../../sast/builder.js");
  const { openDb } = await import("../../db/index.js");
  const { enumeratePrincipleFiles } = await import("../../principleEnumeration.js");

  const projectRoot = resolve(input.projectRoot);
  const principlesDir = resolve(input.principlesDir);
  if (!existsSync(principlesDir)) {
    throw new Error(`principles directory not found: ${principlesDir}`);
  }

  const tsFiles = walkProject(projectRoot);

  const scratchDir = mkdtempSync(join(tmpdir(), "provekit-lint-"));
  const dbPath = join(scratchDir, "scratch.db");
  const db = openDb(dbPath);
  try {
    migrate(db, { migrationsFolder: input.drizzleFolder });

    let filesIndexed = 0;
    let parserFailures = 0;
    for (const f of tsFiles) {
      try {
        buildSASTForFile(db, f);
        filesIndexed++;
      } catch (e) {
        parserFailures++;
        if (input.verbose) {
          process.stderr.write(
            `parser failed on ${f}: ${e instanceof Error ? e.message : String(e)}\n`,
          );
        }
      }
    }

    const { dslPaths } = enumeratePrincipleFiles(principlesDir, { projectRoot });
    let principleErrors = 0;
    for (const dslPath of dslPaths) {
      const dslFile = relative(principlesDir, dslPath);
      let dsl: string;
      try {
        dsl = readFileSync(dslPath, "utf-8");
      } catch {
        principleErrors++;
        continue;
      }
      try {
        evaluatePrinciple(db, dsl);
      } catch (e) {
        principleErrors++;
        if (input.verbose) {
          process.stderr.write(
            `principle ${dslFile} failed: ${e instanceof Error ? e.message : String(e)}\n`,
          );
        }
      }
    }

    const rows = db.$client
      .prepare(
        `SELECT pm.principle_name, pm.severity, pm.message, n.source_line, f.path
         FROM principle_matches pm
         JOIN nodes n ON n.id = pm.root_match_node_id
         JOIN files f ON f.id = pm.file_id
         ORDER BY f.path, n.source_line`,
      )
      .all() as Array<{
        principle_name: string;
        severity: string;
        message: string;
        source_line: number;
        path: string;
      }>;

    const matches: PrincipleMatch[] = rows.map((r) => ({
      principleName: r.principle_name,
      severity: r.severity,
      message: r.message,
      sourceLine: r.source_line,
      path: r.path,
    }));

    return {
      matches,
      filesIndexed,
      parserFailures,
      filesDiscovered: tsFiles.length,
      principlesEvaluated: dslPaths.length - principleErrors,
      principleErrors,
    };
  } finally {
    db.$client.close();
    try {
      rmSync(scratchDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
}

function walkProject(projectRoot: string): string[] {
  const skip = new Set([
    "node_modules",
    "dist",
    ".git",
    ".provekit",
    "test",
    "tests",
    "__tests__",
  ]);
  const out: string[] = [];
  function walk(dir: string): void {
    let entries: { name: string; isDirectory: boolean }[];
    try {
      entries = readdirSync(dir, { withFileTypes: true }).map((e) => ({
        name: e.name,
        isDirectory: e.isDirectory(),
      }));
    } catch {
      return;
    }
    for (const entry of entries) {
      if (skip.has(entry.name)) continue;
      const full = join(dir, entry.name);
      if (entry.isDirectory) {
        walk(full);
      } else if (
        /\.(ts|tsx|mts|cts)$/.test(entry.name) &&
        !/\.(test|spec)\.[^/]+$/.test(entry.name)
      ) {
        out.push(full);
      }
    }
  }
  walk(projectRoot);
  return out;
}
