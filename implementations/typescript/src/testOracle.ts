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

  const cached = cache.get(framework, testFile, reference.testName, sourceAbs);
  if (cached) return { reference, outcome: cached, cached: true };

  const adapter = getAdapter(framework);
  if (!adapter) {
    return {
      reference,
      outcome: { kind: "adapter-error", message: `no adapter registered for framework "${framework}"`, durationMs: 0 },
      cached: false,
    };
  }

  let outcome;
  try {
    outcome = await adapter.runTest({
      projectRoot,
      testFile,
      testName: reference.testName,
      timeoutMs,
    });
  } catch (e: any) {
    // Adapters are documented to never throw — they should convert
    // errors into adapter-error outcomes themselves. If one escapes
    // anyway, we must normalize here so the checker never sees a
    // rejected promise from a call it assumed was safe.
    return {
      reference,
      outcome: {
        kind: "adapter-error",
        message: `adapter threw (contract violation): ${String(e?.message || e).slice(0, 160)}`,
        durationMs: 0,
      },
      cached: false,
    };
  }

  if (outcome.kind !== "adapter-error" && outcome.kind !== "timeout") {
    cache.put(framework, testFile, reference.testName, sourceAbs, outcome);
  }

  return { reference, outcome, cached: false };
}

/**
 * Run every test reference for a function, bounded by maxTests to keep
 * total runtime predictable. Excess references (beyond maxTests) are
 * returned with outcome { kind: "skipped", message: "... (over cap) ..." }
 * so the caller sees that references existed but weren't executed.
 */
export async function runTestsForReferences(
  projectRoot: string,
  framework: TestFramework,
  references: TestReference[],
  sourceFile: string,
  opts: { maxTests?: number; timeoutMsEach?: number } = {}
): Promise<ExecutedOracleResult[]> {
  const maxTests = opts.maxTests ?? 3;
  const timeoutMsEach = opts.timeoutMsEach ?? 30000;
  const results: ExecutedOracleResult[] = [];
  for (const ref of references.slice(0, maxTests)) {
    results.push(await runTest(projectRoot, framework, ref, sourceFile, timeoutMsEach));
  }
  for (const ref of references.slice(maxTests)) {
    results.push({
      reference: ref,
      outcome: {
        kind: "skipped",
        message: `reference skipped — exceeds maxTests=${maxTests} cap (${references.length} total references)`,
        durationMs: 0,
      },
      cached: false,
    });
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
  const skipped = oracleResults.filter((r) => r.outcome.kind === "skipped");
  const errors = oracleResults.filter((r) => r.outcome.kind === "adapter-error" || r.outcome.kind === "timeout");

  // harnessKind classification:
  //   suggests-problem:  encoding-gap, fail, violation
  //   suggests-ok:       pass
  //   neutral:           harness-error, timeout, synthesis-failed, untestable — the harness
  //                      itself didn't provide a verdict about the claim, so we don't
  //                      infer agreement or disagreement from the tests.
  const harnessSuggestsProblem = harnessKind === "encoding-gap" || harnessKind === "fail" || harnessKind === "violation";
  const harnessSuggestsOk = harnessKind === "pass";
  const harnessNeutral = !harnessSuggestsProblem && !harnessSuggestsOk;

  const fragments: string[] = [];
  if (passes.length > 0) fragments.push(`${passes.length} test(s) pass`);
  if (fails.length > 0) fragments.push(`${fails.length} test(s) fail`);
  if (skipped.length > 0) fragments.push(`${skipped.length} skipped`);
  if (errors.length > 0) fragments.push(`${errors.length} could not run`);

  // If no test actually produced a verdict (everything skipped/errored), there's nothing
  // to agree or disagree with.
  const actionableOutcomes = passes.length + fails.length;
  if (actionableOutcomes === 0) {
    return {
      note: `triangle: harness=${harnessKind}, ${fragments.join(", ")} (no actionable oracle verdicts)`,
      hasAgreement: false,
      hasDisagreement: false,
    };
  }

  const note = `triangle: harness=${harnessKind}, ${fragments.join(", ")}`;
  if (harnessNeutral) {
    return { note, hasAgreement: false, hasDisagreement: false };
  }

  // Only declare agreement when the oracle outcomes are uniform and point
  // the same direction as the harness. Mixed pass+fail is intrinsically
  // ambiguous — some tests exercise the claim's input region and agree,
  // others exercise different regions and disagree — neither "agree" nor
  // "disagree" cleanly applies, so we return both false and let callers
  // surface the mixed state via the note string.
  const uniformPass = passes.length > 0 && fails.length === 0;
  const uniformFail = fails.length > 0 && passes.length === 0;
  const hasAgreement =
    (harnessSuggestsProblem && uniformFail) ||
    (harnessSuggestsOk && uniformPass);
  const hasDisagreement =
    (harnessSuggestsProblem && uniformPass) ||
    (harnessSuggestsOk && uniformFail);

  return { note, hasAgreement, hasDisagreement };
}
