/**
 * Triangle oracle: user's existing test suite as a third verification oracle.
 *
 * Two modes:
 *   1. Advisory: detect the framework, find tests referencing the target
 *      function, and expose their source for prompt injection so harness
 *      synthesis can align with existing test conventions.
 *   2. Executing: actually invoke the test runner (via a framework-specific
 *      adapter in ./testAdapters/) against those tests and return
 *      structured outcomes. Enables cross-referencing test pass/fail with
 *      harness pass/encoding-gap for a true three-oracle triangulation.
 */

import { readFileSync, existsSync, readdirSync, statSync } from "fs";
import { join, relative, isAbsolute, resolve as resolvePath } from "path";
import { getAdapter } from "./testAdapters";
import { TestCache } from "./testCache";
import type { TestOutcome } from "./testAdapters/Adapter";

export type TestFramework = "vitest" | "jest" | "mocha" | "ava" | "tap" | "node-test" | "unknown";

export interface TestReference {
  file: string;
  lineStart: number;
  lineEnd: number;
  snippet: string;
  testName: string;
}

const TEST_FILE_RE = /\.(test|spec)\.(t|j)sx?$/;
const TEST_DIRS = ["__tests__", "test", "tests", "spec"];

export function detectTestFramework(projectRoot: string): TestFramework | null {
  const pkgPath = join(projectRoot, "package.json");
  if (!existsSync(pkgPath)) return null;
  try {
    const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
    const deps = { ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) };
    if (deps.vitest) return "vitest";
    if (deps.jest) return "jest";
    if (deps.mocha) return "mocha";
    if (deps.ava) return "ava";
    if (deps.tap || deps["node-tap"]) return "tap";
    const testScript = pkg.scripts?.test || "";
    if (/\bvitest\b/.test(testScript)) return "vitest";
    if (/\bjest\b/.test(testScript)) return "jest";
    if (/\bmocha\b/.test(testScript)) return "mocha";
    if (/\bava\b/.test(testScript)) return "ava";
    if (/\bnode\s+--test\b/.test(testScript)) return "node-test";
    if (testScript) return "unknown";
    return null;
  } catch {
    return null;
  }
}

function walk(dir: string, out: string[], depth: number = 0): void {
  if (depth > 6) return;
  let entries: string[] = [];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const e of entries) {
    if (e === "node_modules" || e === "dist" || e === ".git" || e === ".neurallog" || e === ".worktrees") continue;
    const full = join(dir, e);
    let s;
    try { s = statSync(full); } catch { continue; }
    if (s.isDirectory()) walk(full, out, depth + 1);
    else if (s.isFile() && TEST_FILE_RE.test(e)) out.push(full);
  }
}

export function findTestFiles(projectRoot: string): string[] {
  const results: string[] = [];
  for (const d of TEST_DIRS) {
    const path = join(projectRoot, d);
    if (existsSync(path)) walk(path, results);
  }
  walk(projectRoot, results);
  return [...new Set(results)];
}

export function findTestsForFunction(projectRoot: string, fnName: string): TestReference[] {
  const refs: TestReference[] = [];
  const files = findTestFiles(projectRoot);
  const needle = new RegExp(`\\b${fnName}\\b`);

  for (const file of files) {
    let source: string;
    try { source = readFileSync(file, "utf-8"); } catch { continue; }
    if (!needle.test(source)) continue;

    const lines = source.split("\n");
    const testBlockRe = /^\s*(?:it|test|describe)\s*\(\s*['"`]([^'"`]+)['"`]/;

    let currentStart = -1;
    let currentName = "";
    let depth = 0;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      if (currentStart < 0) {
        const m = line.match(testBlockRe);
        if (m) {
          currentStart = i;
          currentName = m[1]!;
          depth = 0;
        }
      }
      if (currentStart >= 0) {
        for (const ch of line) {
          if (ch === "(" || ch === "{") depth++;
          else if (ch === ")" || ch === "}") depth--;
        }
        if (depth <= 0 && i > currentStart) {
          const block = lines.slice(currentStart, i + 1).join("\n");
          if (needle.test(block)) {
            refs.push({
              file: relative(projectRoot, file),
              lineStart: currentStart + 1,
              lineEnd: i + 1,
              snippet: block.length > 2000 ? block.slice(0, 2000) + "\n// ... (truncated)" : block,
              testName: currentName,
            });
          }
          currentStart = -1;
          currentName = "";
        }
      }
    }
  }

  return refs;
}

export function formatForPrompt(refs: TestReference[]): string {
  if (refs.length === 0) return "";
  const lines: string[] = [
    "",
    "## Existing tests for this function (oracle context)",
    "",
    "Tests already written for this function describe the intended behaviour. Use them as reference when constructing your harness fixture — a harness consistent with these tests is more likely to be checking a meaningful property. Do NOT copy a test's assertion verbatim (that would be tautology); use the tests to calibrate what the function is supposed to do.",
    "",
  ];
  for (const r of refs.slice(0, 3)) {
    lines.push(`### \`${r.file}:${r.lineStart}\` — test: "${r.testName}"`);
    lines.push("```typescript");
    lines.push(r.snippet);
    lines.push("```");
    lines.push("");
  }
  return lines.join("\n");
}

export interface ExecutedOracleResult {
  reference: TestReference;
  outcome: TestOutcome;
  cached: boolean;
}

/**
 * Run a single test reference via the framework-appropriate adapter.
 * Consults the TestCache first; on miss, spawns the runner, stores the
 * outcome. Returns adapter-error if no adapter is registered for the
 * detected framework.
 */
export async function runTest(
  projectRoot: string,
  framework: TestFramework,
  reference: TestReference,
  sourceFile: string,
  timeoutMs: number = 30000
): Promise<ExecutedOracleResult> {
  const cache = new TestCache(projectRoot);
  const testFile = isAbsolute(reference.file) ? reference.file : resolvePath(projectRoot, reference.file);
  const sourceAbs = isAbsolute(sourceFile) ? sourceFile : resolvePath(projectRoot, sourceFile);

  const cached = cache.get(testFile, reference.testName, sourceAbs);
  if (cached) return { reference, outcome: cached, cached: true };

  const adapter = getAdapter(framework);
  if (!adapter) {
    return {
      reference,
      outcome: { kind: "adapter-error", message: `no adapter registered for framework "${framework}"`, durationMs: 0 },
      cached: false,
    };
  }

  const outcome = await adapter.runTest({
    projectRoot,
    testFile,
    testName: reference.testName,
    timeoutMs,
  });

  if (outcome.kind !== "adapter-error" && outcome.kind !== "timeout") {
    cache.put(testFile, reference.testName, sourceAbs, outcome);
  }

  return { reference, outcome, cached: false };
}

/**
 * Run every test reference for a function. Bounded by maxTests to keep
 * total runtime predictable — excess references are skipped with a note.
 */
export async function runTestsForReferences(
  projectRoot: string,
  framework: TestFramework,
  references: TestReference[],
  sourceFile: string,
  opts: { maxTests?: number; timeoutMsEach?: number } = {}
): Promise<ExecutedOracleResult[]> {
  const maxTests = opts.maxTests ?? 5;
  const timeoutMsEach = opts.timeoutMsEach ?? 30000;
  const results: ExecutedOracleResult[] = [];
  for (const ref of references.slice(0, maxTests)) {
    results.push(await runTest(projectRoot, framework, ref, sourceFile, timeoutMsEach));
  }
  return results;
}

/**
 * Summarise a triangle (harness outcome vs test-oracle outcomes) into a
 * single diagnostic string suitable for the CheckResult.error field.
 * The caller still decides the verdict; this just renders the signal.
 */
export function summarizeTriangle(
  harnessKind: string,
  oracleResults: ExecutedOracleResult[]
): { note: string; hasAgreement: boolean; hasDisagreement: boolean } {
  if (oracleResults.length === 0) {
    return { note: "", hasAgreement: false, hasDisagreement: false };
  }

  const passes = oracleResults.filter((r) => r.outcome.kind === "pass");
  const fails = oracleResults.filter((r) => r.outcome.kind === "fail" || r.outcome.kind === "error");
  const errors = oracleResults.filter((r) => r.outcome.kind === "adapter-error" || r.outcome.kind === "timeout");
  const harnessSuggestsProblem = harnessKind === "encoding-gap" || harnessKind === "fail" || harnessKind === "violation";

  const agreementFragments: string[] = [];
  if (passes.length > 0) agreementFragments.push(`${passes.length} test(s) pass`);
  if (fails.length > 0) agreementFragments.push(`${fails.length} test(s) fail`);
  if (errors.length > 0) agreementFragments.push(`${errors.length} test(s) could not run`);

  const note = `triangle: harness=${harnessKind}, ${agreementFragments.join(", ")}`;
  const hasAgreement =
    (harnessSuggestsProblem && fails.length > 0) ||
    (!harnessSuggestsProblem && passes.length > 0 && fails.length === 0);
  const hasDisagreement =
    (harnessSuggestsProblem && passes.length > 0 && fails.length === 0) ||
    (!harnessSuggestsProblem && fails.length > 0);

  return { note, hasAgreement, hasDisagreement };
}
