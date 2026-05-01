import { describe, it, expect } from "vitest";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { liftPath, mintProof, defaultLiftOptions } from "./index.js";

function tempDir(): string {
  return mkdtempSync(join(tmpdir(), "provekit-lift-vt-"));
}

describe("lift / vitest-tests adapter (point-specific behavior witnesses)", () => {
  it("lifts 4 simple expect(actual).toBe(expected) calls and 1 deliberately skipped pattern", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "fixture.test.ts"),
      `
import { describe, it, expect } from "vitest";

it("parse_int_42", () => {
  expect(parseInt("42")).toBe(42);
});

it("parse_int_zero", () => {
  expect(parseInt("0")).toBe(0);
});

it("multiple asserts in one test", () => {
  expect(answer).toEqual(42);
  expect(count).toBeGreaterThan(0);
});

// Deliberately skipped: async resolves matcher.
it("skipped async", () => {
  expect(loadValue()).resolves.toBe(99);
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt).toBeDefined();
    // 4 expect calls lifted: 2 toBe + 1 toEqual + 1 toBeGreaterThan.
    // 1 skipped: resolves.toBe.
    expect(vt.lifted).toBe(4);
    expect(vt.warnings).toHaveLength(1);
    expect(vt.warnings[0]!.reason).toMatch(/async|resolves/);

    // Each expect becomes its OWN contract decl, named <test>::<index>.
    const names = r.decls
      .filter((d) => d.adapter === "vitest-tests")
      .map((d) => d.name)
      .sort();
    expect(names).toContain("parse_int_42::0");
    expect(names).toContain("parse_int_zero::0");
    expect(names).toContain("multiple asserts in one test::0");
    expect(names).toContain("multiple asserts in one test::1");
  });

  it("skips expect(fn).toThrow(...) with warning", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "throws.test.ts"),
      `
import { it, expect } from "vitest";
it("throws", () => {
  expect(() => parseInt("nope")).toThrow();
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(0);
    expect(vt.warnings).toHaveLength(1);
    expect(vt.warnings[0]!.reason).toMatch(/toThrow/);
  });

  it("skips operands with method-call chains (honest under-coverage)", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "method.test.ts"),
      `
import { it, expect } from "vitest";
it("method chain", () => {
  expect("hello".toUpperCase()).toBe("HELLO");
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.seen).toBe(1);
    expect(vt.lifted).toBe(0);
    expect(vt.warnings).toHaveLength(1);
  });

  it("each lifted assertion mints its own content-addressed memento", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "facts.test.ts"),
      `
import { it, expect } from "vitest";
it("triple", () => {
  expect(f(1)).toBe(1);
  expect(f(2)).toBe(2);
  expect(f(3)).toBe(3);
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(3);
    const minted = mintProof(r.decls, defaultLiftOptions());
    expect(minted.cid.startsWith("blake3-512:")).toBe(true);
    // Three distinct facts -> three distinct mementos in the catalog.
    expect(minted.memberCount).toBeGreaterThanOrEqual(3);
    // Diagnostic: CID for the report. Stable given the dev seed and
    // the canonical IR; printing here so the parent agent can capture
    // it from the test log.
    console.log(`VITEST_TESTS_FIXTURE_CID=${minted.cid}`);
  });
});
