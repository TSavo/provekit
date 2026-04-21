import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { detectTestFramework, findTestsForFunction } from "./testOracle";
import { mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

const tmpRoot = join(tmpdir(), `neurallog-oracle-test-${Date.now()}`);

beforeAll(() => {
  mkdirSync(tmpRoot, { recursive: true });
});

afterAll(() => {
  try { rmSync(tmpRoot, { recursive: true, force: true }); } catch {}
});

describe("detectTestFramework", () => {
  it("returns null when package.json is missing", () => {
    const dir = join(tmpRoot, "no-pkg");
    mkdirSync(dir, { recursive: true });
    expect(detectTestFramework(dir)).toBeNull();
  });

  it("detects vitest from devDependencies", () => {
    const dir = join(tmpRoot, "vitest");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", devDependencies: { vitest: "^4.0.0" } })
    );
    expect(detectTestFramework(dir)).toBe("vitest");
  });

  it("detects jest from dependencies", () => {
    const dir = join(tmpRoot, "jest");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", dependencies: { jest: "^29.0.0" } })
    );
    expect(detectTestFramework(dir)).toBe("jest");
  });

  it("detects mocha from scripts.test", () => {
    const dir = join(tmpRoot, "mocha");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", scripts: { test: "mocha -r ts-node/register" } })
    );
    expect(detectTestFramework(dir)).toBe("mocha");
  });

  it("detects node --test", () => {
    const dir = join(tmpRoot, "node-test");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", scripts: { test: "node --test" } })
    );
    expect(detectTestFramework(dir)).toBe("node-test");
  });

  it("returns 'unknown' when a test script exists but framework is unclear", () => {
    const dir = join(tmpRoot, "unknown");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", scripts: { test: "echo 'no tests'" } })
    );
    expect(detectTestFramework(dir)).toBe("unknown");
  });

  it("returns null when there's no test infrastructure at all", () => {
    const dir = join(tmpRoot, "plain");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ name: "x", scripts: { build: "tsc" } })
    );
    expect(detectTestFramework(dir)).toBeNull();
  });
});

describe("findTestsForFunction", () => {
  it("finds a test block that calls the target function", () => {
    const dir = join(tmpRoot, "find-tests");
    mkdirSync(join(dir, "__tests__"), { recursive: true });
    writeFileSync(
      join(dir, "__tests__", "sample.test.ts"),
      `import { describe, it, expect } from "vitest";
describe("math", () => {
  it("adds two numbers", () => {
    const r = myAdd(1, 2);
    expect(r).toBe(3);
  });
  it("subtracts", () => {
    const r = mySubtract(5, 2);
    expect(r).toBe(3);
  });
});`
    );

    const refs = findTestsForFunction(dir, "myAdd");
    expect(refs.length).toBeGreaterThan(0);
    // Finds either the describe block or a specific it — both contain myAdd.
    expect(refs.some((r) => r.snippet.includes("myAdd"))).toBe(true);
    expect(refs.every((r) => typeof r.testName === "string" && r.testName.length > 0)).toBe(true);
  });

  it("returns empty array when no test references the function", () => {
    const dir = join(tmpRoot, "no-tests");
    mkdirSync(join(dir, "__tests__"), { recursive: true });
    writeFileSync(
      join(dir, "__tests__", "other.test.ts"),
      `it("something", () => { expect(1).toBe(1); });`
    );
    expect(findTestsForFunction(dir, "nonexistentFn")).toEqual([]);
  });
});
