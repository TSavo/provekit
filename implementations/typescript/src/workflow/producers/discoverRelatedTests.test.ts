import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

import { discoverRelatedTests } from "./discoverRelatedTests.js";

function makeProject(): string {
  const root = mkdtempSync(join(tmpdir(), "discover-related-tests-"));
  return root;
}

function writeFile(root: string, relPath: string, content: string): void {
  const abs = join(root, relPath);
  mkdirSync(join(abs, ".."), { recursive: true });
  writeFileSync(abs, content, "utf8");
}

describe("discoverRelatedTests", () => {
  let root: string;
  beforeEach(() => {
    root = makeProject();
  });

  it("classifies a brand-new test in the diff as 'added'", () => {
    writeFile(
      root,
      "src/math.test.ts",
      [
        "import { calculate } from './math';",
        "test('returns error for zero denominator', () => {",
        "  expect(calculate(10, 0)).toEqual({ error: 'divide by zero' });",
        "});",
      ].join("\n"),
    );

    const diff = [
      "--- /dev/null",
      "+++ b/src/math.test.ts",
      "@@ -0,0 +1,4 @@",
      "+import { calculate } from './math';",
      "+test('returns error for zero denominator', () => {",
      "+  expect(calculate(10, 0)).toEqual({ error: 'divide by zero' });",
      "+});",
    ].join("\n");

    const out = discoverRelatedTests({
      diff,
      modifiedFiles: ["src/math.test.ts"],
      projectRoot: root,
    });

    expect(out).toHaveLength(1);
    expect(out[0]!.filePath).toBe("src/math.test.ts");
    expect(out[0]!.relationship).toBe("added");
    expect(out[0]!.testNames).toEqual([
      "returns error for zero denominator",
    ]);
  });

  it("classifies an in-place edit of an existing test as 'modified'", () => {
    writeFile(
      root,
      "src/math.test.ts",
      "test('foo', () => {});\ntest('bar', () => {});",
    );

    const diff = [
      "--- a/src/math.test.ts",
      "+++ b/src/math.test.ts",
      "@@ -1,2 +1,2 @@",
      "-test('foo', () => {});",
      "+test('foo edited', () => {});",
      " test('bar', () => {});",
    ].join("\n");

    const out = discoverRelatedTests({
      diff,
      modifiedFiles: ["src/math.test.ts"],
      projectRoot: root,
    });

    expect(out).toHaveLength(1);
    expect(out[0]!.relationship).toBe("modified");
  });

  it("surfaces sibling .test.ts as 'preserves' when a production file is modified", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");
    writeFile(
      root,
      "src/math.test.ts",
      "test('legacy expectation', () => {});",
    );

    const diff = [
      "--- a/src/math.ts",
      "+++ b/src/math.ts",
      "@@ -1 +1 @@",
      "-export function calculate() {}",
      "+export function calculate(a, b) { return a / b; }",
    ].join("\n");

    const out = discoverRelatedTests({
      diff,
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    expect(out).toHaveLength(1);
    expect(out[0]!.filePath).toBe("src/math.test.ts");
    expect(out[0]!.relationship).toBe("preserves");
    expect(out[0]!.testNames).toEqual(["legacy expectation"]);
  });

  it("recognises __tests__/ subdirectory convention as 'preserves'", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");
    writeFile(
      root,
      "src/__tests__/math.test.ts",
      "test('via __tests__', () => {});",
    );

    const diff = [
      "--- a/src/math.ts",
      "+++ b/src/math.ts",
      "@@ -1 +1 @@",
      "-export function calculate() {}",
      "+export function calculate(x) { return x; }",
    ].join("\n");

    const out = discoverRelatedTests({
      diff,
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    expect(out.map((t) => t.filePath)).toContain(
      "src/__tests__/math.test.ts",
    );
  });

  it("flags a test in another file that imports the modified symbol as 'calls'", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");
    writeFile(
      root,
      "src/widget.test.ts",
      [
        "import { calculate } from './math';",
        "test('widget uses calculate', () => {",
        "  expect(calculate(1, 1)).toBe(1);",
        "});",
      ].join("\n"),
    );

    const diff = [
      "--- a/src/math.ts",
      "+++ b/src/math.ts",
      "@@ -1 +1 @@",
      "-export function calculate() {}",
      "+export function calculate(a, b) { return a / b; }",
    ].join("\n");

    const out = discoverRelatedTests({
      diff,
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    const widget = out.find((t) => t.filePath === "src/widget.test.ts");
    expect(widget).toBeDefined();
    expect(widget!.relationship).toBe("calls");
  });

  it("does not traverse node_modules or skipped directories", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");
    writeFile(
      root,
      "node_modules/some-pkg/test/math.test.ts",
      "import './math'; test('noisy', () => {});",
    );
    writeFile(
      root,
      "dist/math.test.ts",
      "import './math'; test('built', () => {});",
    );

    const out = discoverRelatedTests({
      diff: [
        "--- a/src/math.ts",
        "+++ b/src/math.ts",
        "@@ -1 +1 @@",
        "-export function calculate() {}",
        "+export function calculate(x) { return x; }",
      ].join("\n"),
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    for (const t of out) {
      expect(t.filePath).not.toMatch(/node_modules|^dist\//);
    }
  });

  it("returns [] when no tests can be located", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");

    const out = discoverRelatedTests({
      diff: [
        "--- a/src/math.ts",
        "+++ b/src/math.ts",
        "@@ -1 +1 @@",
        "-export function calculate() {}",
        "+export function calculate(x) { return x; }",
      ].join("\n"),
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    expect(out).toEqual([]);
  });

  it("output is sorted by filePath for deterministic downstream hashing", () => {
    writeFile(root, "src/math.ts", "export function calculate() {}");
    writeFile(
      root,
      "src/zeta.test.ts",
      "import { calculate } from './math'; test('z', () => {});",
    );
    writeFile(
      root,
      "src/alpha.test.ts",
      "import { calculate } from './math'; test('a', () => {});",
    );

    const out = discoverRelatedTests({
      diff: [
        "--- a/src/math.ts",
        "+++ b/src/math.ts",
        "@@ -1 +1 @@",
        "-export function calculate() {}",
        "+export function calculate(x) { return x; }",
      ].join("\n"),
      modifiedFiles: ["src/math.ts"],
      projectRoot: root,
    });

    const paths = out.map((t) => t.filePath);
    expect(paths).toEqual([...paths].sort());
  });
});
