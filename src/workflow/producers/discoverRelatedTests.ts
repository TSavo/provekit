/**
 * discoverRelatedTests — pure-filesystem helper that surfaces tests
 * relevant to a diff so they can flow through `intent-from-diff` (and
 * downstream into `formulate-via-lifter`) as existential evidence.
 *
 * Spec: protocol/specs/2026-04-29-ts-ir-language.md §15 — tests are the
 * highest-value intent source.
 *
 * The helper is intentionally cheap. No LLM, no network. It walks the
 * project root for test files, parses the diff for added/modified
 * markers, and stitches the two together with a small relationship
 * heuristic. If discovery yields nothing the caller can still run with
 * an empty `tests` array; the producer falls back to diff + message.
 *
 * The "calls" relationship is best-effort: it greps existing test
 * files for `from "..."` imports referencing each modified file's
 * stem. Symbol-resolution would be tighter; v1 trades precision for
 * zero-dependency simplicity.
 */

import { readFileSync, readdirSync, statSync } from "fs";
import { join, basename, dirname, relative } from "path";

import type { TestRelationship, TestSource } from "./intentFromDiff.js";

export interface DiscoverRelatedTestsArgs {
  /** Unified diff describing the change. Used to classify new vs modified test files. */
  diff: string;
  /** Project-root-relative paths of files the diff touches (production + test). */
  modifiedFiles: string[];
  /** Absolute project-root path. */
  projectRoot: string;
}

const TEST_FILE_RX = /\.(test|spec)\.(ts|tsx|mts|cts)$/;
const TS_SOURCE_RX = /\.(ts|tsx|mts|cts)$/;
const SKIP_DIR_NAMES = new Set([
  "node_modules",
  ".git",
  "dist",
  "build",
  "lib",
  "out",
  "coverage",
  ".next",
  ".turbo",
  ".provekit",
]);

export function discoverRelatedTests(
  args: DiscoverRelatedTestsArgs,
): TestSource[] {
  const { diff, modifiedFiles, projectRoot } = args;

  const diffTestRelationships = classifyDiffTestFiles(diff);
  const seen = new Map<string, TestSource>();

  // 1. Tests directly touched by the diff: "added" or "modified".
  for (const [relPath, relationship] of diffTestRelationships) {
    const abs = join(projectRoot, relPath);
    const source = safeRead(abs);
    if (source === null) continue;
    seen.set(relPath, {
      source,
      testNames: extractTestNames(source),
      filePath: relPath,
      relationship,
    });
  }

  // 2. For each modified non-test file, look up parallel test files
  //    ("preserves") via a few standard conventions.
  const modifiedProductionFiles = modifiedFiles.filter(
    (f) => !TEST_FILE_RX.test(f) && TS_SOURCE_RX.test(f),
  );
  for (const productionFile of modifiedProductionFiles) {
    for (const candidateRel of parallelTestCandidates(productionFile)) {
      if (seen.has(candidateRel)) continue;
      const abs = join(projectRoot, candidateRel);
      const source = safeRead(abs);
      if (source === null) continue;
      seen.set(candidateRel, {
        source,
        testNames: extractTestNames(source),
        filePath: candidateRel,
        relationship: "preserves",
      });
    }
  }

  // 3. "calls": walk the project for test files that import any
  //    modified production file's stem. Lexical match — best-effort.
  if (modifiedProductionFiles.length > 0) {
    const stems = new Set<string>();
    for (const f of modifiedProductionFiles) {
      stems.add(stripTsExt(basename(f)));
    }
    for (const testFileAbs of walkTestFiles(projectRoot)) {
      const relPath = relative(projectRoot, testFileAbs);
      if (seen.has(relPath)) continue;
      const source = safeRead(testFileAbs);
      if (source === null) continue;
      if (!testFileImportsAnyStem(source, stems)) continue;
      seen.set(relPath, {
        source,
        testNames: extractTestNames(source),
        filePath: relPath,
        relationship: "calls",
      });
    }
  }

  return [...seen.values()].sort((a, b) =>
    a.filePath.localeCompare(b.filePath),
  );
}

/**
 * Parse the diff for `+++ b/<path>` markers and decide whether each
 * test file is freshly added (paired `--- /dev/null`) or modified.
 * Returns project-root-relative paths.
 */
function classifyDiffTestFiles(
  diff: string,
): Map<string, Extract<TestRelationship, "added" | "modified">> {
  const out = new Map<
    string,
    Extract<TestRelationship, "added" | "modified">
  >();
  const lines = diff.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    if (!line.startsWith("+++ ")) continue;
    const newPath = stripDiffPathPrefix(line.slice(4).trim());
    if (newPath === null) continue;
    if (!TEST_FILE_RX.test(newPath)) continue;
    const prevLine = i > 0 ? lines[i - 1] ?? "" : "";
    const isAdd =
      prevLine.startsWith("--- ") &&
      stripDiffPathPrefix(prevLine.slice(4).trim()) === null;
    out.set(newPath, isAdd ? "added" : "modified");
  }
  return out;
}

/**
 * `--- a/foo.ts` → `foo.ts`. `--- /dev/null` → null. Bare `--- foo.ts`
 * (no a/b prefix) → `foo.ts`. Anything quoted is unquoted-then-stripped.
 */
function stripDiffPathPrefix(s: string): string | null {
  let raw = s;
  if (raw.startsWith('"') && raw.endsWith('"')) raw = raw.slice(1, -1);
  if (raw === "/dev/null") return null;
  if (raw.startsWith("a/") || raw.startsWith("b/")) return raw.slice(2);
  return raw;
}

/**
 * Standard parallel-test conventions for a TS production file.
 * - sibling `.test.ts` / `.spec.ts` (same dir, same stem)
 * - `__tests__/<stem>.test.ts` (same dir's `__tests__` subdir)
 */
function parallelTestCandidates(productionFile: string): string[] {
  const dir = dirname(productionFile);
  const stem = stripTsExt(basename(productionFile));
  const exts = ["ts", "tsx", "mts", "cts"];
  const out: string[] = [];
  for (const ext of exts) {
    out.push(joinPosix(dir, `${stem}.test.${ext}`));
    out.push(joinPosix(dir, `${stem}.spec.${ext}`));
    out.push(joinPosix(dir, "__tests__", `${stem}.test.${ext}`));
    out.push(joinPosix(dir, "__tests__", `${stem}.spec.${ext}`));
  }
  return out;
}

function stripTsExt(filename: string): string {
  return filename.replace(TS_SOURCE_RX, "");
}

/**
 * Recursively walk the project root for test files. Skips
 * `node_modules`, build outputs, and dotfile directories. Yields
 * absolute paths.
 */
function* walkTestFiles(root: string): Iterable<string> {
  type Entry = { name: string; isDirectory: boolean; isFile: boolean };
  let entries: Entry[];
  try {
    entries = readdirSync(root, { withFileTypes: true }).map((e) => ({
      name: e.name,
      isDirectory: e.isDirectory(),
      isFile: e.isFile(),
    }));
  } catch {
    return;
  }
  for (const entry of entries) {
    const abs = join(root, entry.name);
    if (entry.isDirectory) {
      if (SKIP_DIR_NAMES.has(entry.name)) continue;
      if (entry.name.startsWith(".")) continue;
      yield* walkTestFiles(abs);
    } else if (entry.isFile) {
      if (TEST_FILE_RX.test(entry.name)) yield abs;
    }
  }
}

function testFileImportsAnyStem(source: string, stems: Set<string>): boolean {
  // Lexical: look for any `from "<...>"` or `from '<...>'` whose
  // module path's basename (with optional .js suffix) matches a stem.
  const importRx = /from\s+["']([^"']+)["']/g;
  let m: RegExpExecArray | null;
  while ((m = importRx.exec(source)) !== null) {
    const spec = m[1] ?? "";
    const stem = stripJsExt(basename(spec));
    if (stems.has(stem)) return true;
  }
  return false;
}

function stripJsExt(name: string): string {
  return name.replace(/\.(js|mjs|cjs|ts|tsx|mts|cts)$/, "");
}

/** Best-effort test-name extraction. Recognises `test("name", ...)`,
 * `it("name", ...)`, `describe("name", ...)`. Returns the names in
 * source order. Bad parses just yield fewer names — safe to be loose. */
function extractTestNames(source: string): string[] {
  const out: string[] = [];
  const rx = /\b(?:test|it|describe)\s*\(\s*(['"`])((?:\\.|(?!\1).)*?)\1/g;
  let m: RegExpExecArray | null;
  while ((m = rx.exec(source)) !== null) {
    const name = m[2] ?? "";
    if (name.length > 0) out.push(name);
  }
  return out;
}

function safeRead(absPath: string): string | null {
  try {
    const st = statSync(absPath);
    if (!st.isFile()) return null;
    return readFileSync(absPath, "utf8");
  } catch {
    return null;
  }
}

/** path.join always uses POSIX separators here so test file paths
 * compare equal regardless of host platform. The producer's caller
 * supplies POSIX paths via diff parsing already. */
function joinPosix(...parts: string[]): string {
  return parts
    .filter((p) => p !== "" && p !== ".")
    .join("/")
    .replace(/\/+/g, "/");
}
