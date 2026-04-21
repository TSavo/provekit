import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { summarizeTriangle } from "./testOracle";
import { TestCache } from "./testCache";
import { getAdapter, listAdapters } from "./testAdapters";
import { mkdirSync, writeFileSync, rmSync, utimesSync, statSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

const tmpRoot = join(tmpdir(), `neurallog-triangle-test-${Date.now()}`);

beforeAll(() => {
  mkdirSync(tmpRoot, { recursive: true });
});

afterAll(() => {
  try { rmSync(tmpRoot, { recursive: true, force: true }); } catch {}
});

describe("summarizeTriangle", () => {
  const passResult = { reference: any("ref-pass"), outcome: { kind: "pass" as const, message: "ok", durationMs: 10 }, cached: false };
  const failResult = { reference: any("ref-fail"), outcome: { kind: "fail" as const, message: "expected 1 got 2", durationMs: 12 }, cached: false };
  const errorResult = { reference: any("ref-err"), outcome: { kind: "adapter-error" as const, message: "spawn failed", durationMs: 0 }, cached: false };

  function any(testName: string) {
    return { file: "t.test.ts", lineStart: 1, lineEnd: 5, snippet: "", testName };
  }

  it("returns empty note for empty oracle results", () => {
    const r = summarizeTriangle("pass", []);
    expect(r.note).toBe("");
    expect(r.hasAgreement).toBe(false);
    expect(r.hasDisagreement).toBe(false);
  });

  it("harness=pass + all tests pass → agreement", () => {
    const r = summarizeTriangle("pass", [passResult, passResult]);
    expect(r.hasAgreement).toBe(true);
    expect(r.hasDisagreement).toBe(false);
    expect(r.note).toContain("2 test(s) pass");
  });

  it("harness=pass + uniformly failing tests → disagreement", () => {
    const r = summarizeTriangle("pass", [failResult, failResult]);
    expect(r.hasDisagreement).toBe(true);
    expect(r.hasAgreement).toBe(false);
    expect(r.note).toContain("2 test(s) fail");
  });

  it("harness=pass + mixed pass/fail oracles → neither agreement nor disagreement (ambiguous)", () => {
    const r = summarizeTriangle("pass", [passResult, failResult]);
    expect(r.hasAgreement).toBe(false);
    expect(r.hasDisagreement).toBe(false);
    expect(r.note).toContain("1 test(s) pass");
    expect(r.note).toContain("1 test(s) fail");
  });

  it("harness=encoding-gap + tests fail → agreement (both signal problem)", () => {
    const r = summarizeTriangle("encoding-gap", [failResult]);
    expect(r.hasAgreement).toBe(true);
    expect(r.hasDisagreement).toBe(false);
  });

  it("harness=encoding-gap + tests pass → disagreement (harness may be wrong)", () => {
    const r = summarizeTriangle("encoding-gap", [passResult, passResult]);
    expect(r.hasDisagreement).toBe(true);
    expect(r.hasAgreement).toBe(false);
  });

  it("adapter-errors don't count as pass or fail for agreement purposes", () => {
    const r = summarizeTriangle("pass", [errorResult, errorResult]);
    expect(r.hasAgreement).toBe(false);
    expect(r.hasDisagreement).toBe(false);
    expect(r.note).toContain("2 could not run");
  });

  it("neutral harness kinds (harness-error, timeout) produce no agreement/disagreement", () => {
    expect(summarizeTriangle("harness-error", [passResult])).toMatchObject({ hasAgreement: false, hasDisagreement: false });
    expect(summarizeTriangle("timeout", [failResult])).toMatchObject({ hasAgreement: false, hasDisagreement: false });
    expect(summarizeTriangle("synthesis-failed", [passResult, failResult])).toMatchObject({ hasAgreement: false, hasDisagreement: false });
  });

  it("note does not end with a trailing comma when only skipped/errored outcomes are present", () => {
    const skippedResult = { reference: any("s"), outcome: { kind: "skipped" as const, message: "over cap", durationMs: 0 }, cached: false };
    const r = summarizeTriangle("pass", [skippedResult, errorResult]);
    expect(r.note).not.toMatch(/,\s*$/);
    expect(r.note).toContain("no actionable oracle verdicts");
  });
});

describe("TestCache", () => {
  it("returns null on miss", () => {
    const cache = new TestCache(tmpRoot);
    expect(cache.get("vitest", "nonexistent.ts", "some test", "src/foo.ts")).toBeNull();
  });

  it("round-trips an outcome", () => {
    const testFile = join(tmpRoot, "a.test.ts");
    const sourceFile = join(tmpRoot, "a.ts");
    writeFileSync(testFile, "// test");
    writeFileSync(sourceFile, "// source");
    const cache = new TestCache(tmpRoot);
    cache.put("vitest", testFile, "my test", sourceFile, { kind: "pass", message: "ok", durationMs: 50 });
    const got = cache.get("vitest", testFile, "my test", sourceFile);
    expect(got).not.toBeNull();
    expect(got!.kind).toBe("pass");
    expect(got!.durationMs).toBe(50);
  });

  it("invalidates on source file mtime change (via utimes — deterministic)", () => {
    const testFile = join(tmpRoot, "b.test.ts");
    const sourceFile = join(tmpRoot, "b.ts");
    writeFileSync(testFile, "// test");
    writeFileSync(sourceFile, "// v1");
    const cache = new TestCache(tmpRoot);
    cache.put("vitest", testFile, "t", sourceFile, { kind: "pass", message: "ok", durationMs: 1 });
    expect(cache.get("vitest", testFile, "t", sourceFile)).not.toBeNull();

    // Force mtime forward by 10 seconds regardless of filesystem granularity.
    const now = statSync(sourceFile).mtime;
    const future = new Date(now.getTime() + 10_000);
    utimesSync(sourceFile, future, future);
    expect(cache.get("vitest", testFile, "t", sourceFile)).toBeNull();
  });

  it("different frameworks are cached independently", () => {
    const testFile = join(tmpRoot, "c.test.ts");
    const sourceFile = join(tmpRoot, "c.ts");
    writeFileSync(testFile, "// test");
    writeFileSync(sourceFile, "// source");
    const cache = new TestCache(tmpRoot);
    cache.put("vitest", testFile, "t", sourceFile, { kind: "pass", message: "via vitest", durationMs: 1 });
    expect(cache.get("vitest", testFile, "t", sourceFile)!.message).toBe("via vitest");
    // Same file/test/source under a different framework: should be a miss
    expect(cache.get("jest", testFile, "t", sourceFile)).toBeNull();
  });
});

describe("testAdapters registry", () => {
  it("exposes adapters for all four frameworks", () => {
    expect(getAdapter("vitest")).not.toBeNull();
    expect(getAdapter("jest")).not.toBeNull();
    expect(getAdapter("mocha")).not.toBeNull();
    expect(getAdapter("node-test")).not.toBeNull();
  });

  it("returns null for unknown framework", () => {
    expect(getAdapter("pytest")).toBeNull();
    expect(getAdapter("ava")).toBeNull();
  });

  it("listAdapters returns all registered", () => {
    const all = listAdapters();
    expect(all.length).toBeGreaterThanOrEqual(4);
    const frameworks = all.map((a) => a.framework);
    expect(frameworks).toContain("vitest");
    expect(frameworks).toContain("jest");
    expect(frameworks).toContain("mocha");
    expect(frameworks).toContain("node-test");
  });

  it("each adapter has a canonical framework and a human name", () => {
    for (const a of listAdapters()) {
      expect(typeof a.framework).toBe("string");
      expect(a.framework.length).toBeGreaterThan(0);
      expect(typeof a.name).toBe("string");
      expect(a.name.length).toBeGreaterThan(0);
    }
  });
});
