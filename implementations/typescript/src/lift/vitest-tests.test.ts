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
    // 2 Layer 0 atomic mementos: parse_int_42::0, parse_int_zero::0.
    // 1 Layer 2 characterization memento (2 expects in one test
    //   collapses to a single conjunction memento named "multiple
    //   asserts in one test").
    // 1 skipped: resolves.toBe (the async test has 1 expect; Layer 2
    //   only fires for >=2 expects, so it falls through to Layer 0,
    //   which warns and skips).
    // Total: 3 lifted, 1 warning.
    expect(vt.lifted).toBe(3);
    expect(vt.warnings).toHaveLength(1);
    expect(vt.warnings[0]!.reason).toMatch(/async|resolves/);

    const names = r.decls
      .filter((d) => d.adapter === "vitest-tests")
      .map((d) => d.name)
      .sort();
    expect(names).toContain("parse_int_42::0");
    expect(names).toContain("parse_int_zero::0");
    // Layer 2 owns this name: a single conjunction memento, NOT
    // individually-indexed Layer 0 mementos.
    expect(names).toContain("multiple asserts in one test");
    expect(names).not.toContain("multiple asserts in one test::0");
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

  // v0.6: method calls on the operand now lift as UFCS Ctor terms
  // (the previous v0.5 negative test was inverted; the original lived
  // here and asserted a skip).
  it("v0.6: lifts operand method-call as UFCS Ctor", () => {
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
    expect(vt.lifted).toBe(1);
    expect(vt.warnings).toHaveLength(0);

    const decl = r.decls.find((d) => d.name === "method chain::0")!;
    expect(decl).toBeDefined();
    // Shape: atomic("=", [Ctor("toUpperCase", [Const("hello")]),
    //                    Const("HELLO")])
    const f = decl.inv as { kind: string; name: string; args: unknown[] };
    expect(f.kind).toBe("atomic");
    expect(f.name).toBe("=");
    const recv = f.args[0] as { kind: string; name: string; args: unknown[] };
    expect(recv.kind).toBe("ctor");
    expect(recv.name).toBe("toUpperCase");
    expect(recv.args.length).toBe(1);
  });

  it("v0.6: lifts multi-arg free-function call as Ctor with all args", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "multi.test.ts"),
      `
import { it, expect } from "vitest";
it("clamp_lift", () => {
  expect(clamp(5, 0, 10)).toBe(5);
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(1);

    const decl = r.decls.find((d) => d.name === "clamp_lift::0")!;
    const f = decl.inv as { kind: string; args: unknown[] };
    const lhs = f.args[0] as { kind: string; name: string; args: unknown[] };
    expect(lhs.kind).toBe("ctor");
    expect(lhs.name).toBe("clamp");
    expect(lhs.args.length).toBe(3);
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
    // Three expects in one test -> Layer 2 collapses to one
    // characterization-conjunction memento. This is the deliberate
    // semantics: the test characterizes ONE thing, the conjunction.
    expect(vt.lifted).toBe(1);
    const minted = mintProof(r.decls, defaultLiftOptions());
    expect(minted.cid.startsWith("blake3-512:")).toBe(true);
    expect(minted.memberCount).toBeGreaterThanOrEqual(1);
    // Diagnostic: CID for the report. Stable given the dev seed and
    // the canonical IR; printing here so the parent agent can capture
    // it from the test log.
    console.log(`VITEST_TESTS_FIXTURE_CID=${minted.cid}`);
  });
});

describe("lift / vitest-tests Layer 2 (structural patterns)", () => {
  it("Pattern 1: bounded for-loop lifts to forall-implies", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "loop.test.ts"),
      `
import { it, expect } from "vitest";
it("squares_nonneg", () => {
  for (let i = 0; i < 100; i++) {
    expect(i).toBeGreaterThanOrEqual(0);
  }
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(1);
    const decl = r.decls.find((d) => d.name === "squares_nonneg")!;
    expect(decl).toBeDefined();
    // Top-level shape is a forall.
    expect(decl.inv).toBeDefined();
    expect((decl.inv as { kind: string }).kind).toBe("forall");
  });

  it("Pattern 1: nested for-loop deliberately skips with structured warning", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "nested.test.ts"),
      `
import { it, expect } from "vitest";
it("nested", () => {
  for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
      expect(i).toBeGreaterThanOrEqual(0);
    }
  }
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(0);
    expect(vt.warnings.some((w) => w.reason.includes("nested"))).toBe(true);
  });

  it("Pattern 2: helper function calls inline at each call site", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "helper.test.ts"),
      `
import { it, expect } from "vitest";
function checkIs42(x) {
  expect(x).toBe(42);
}
it("many_42s", () => {
  checkIs42(42);
  checkIs42(42);
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(2);
    const names = r.decls.filter((d) => d.adapter === "vitest-tests").map((d) => d.name);
    expect(names).toContain("many_42s::call::0");
    expect(names).toContain("many_42s::call::1");
  });

  it("Pattern 3: multi-expect characterization collapses to and-conjunction", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "char.test.ts"),
      `
import { it, expect } from "vitest";
it("multi_facts", () => {
  expect(f(1)).toBe(1);
  expect(f(2)).toBe(2);
  expect(f(3)).toBe(3);
});
      `,
    );
    const r = liftPath(td);
    const vt = r.adapterReports.find((a) => a.adapter === "vitest-tests")!;
    expect(vt.lifted).toBe(1);
    const decl = r.decls.find((d) => d.name === "multi_facts")!;
    expect(decl).toBeDefined();
    expect((decl.inv as { kind: string }).kind).toBe("and");
    expect((decl.inv as { operands: unknown[] }).operands.length).toBe(3);
  });
});
