/**
 * Triangle oracle: user's existing test suite as a third verification oracle.
 *
 * Clover's architecture insight: when the project has unit tests, those tests
 * are an independent source of truth alongside the Z3 proof and the runtime
 * harness. This module detects the test framework, finds tests referencing
 * a target function, and exposes their source so harness synthesis can use
 * them as reference.
 *
 * v1 scope: detection + source extraction for prompt injection. Actual test
 * *execution* as a runtime oracle (invoking the test runner with specific
 * inputs and cross-referencing outcomes) is a larger architectural step and
 * left as followup.
 */

import { readFileSync, existsSync, readdirSync, statSync } from "fs";
import { join, relative } from "path";

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
