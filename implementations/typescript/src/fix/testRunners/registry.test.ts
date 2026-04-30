/**
 * Test runner registry — unit tests.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerTestRunner,
  getTestRunner,
  listTestRunners,
  detectTestRunner,
  _clearTestRunnerRegistry,
} from "./registry.js";
import type { TestRunnerDescriptor } from "./registry.js";

function makeDescriptor(name: string, detectScore = 0.5): TestRunnerDescriptor {
  return {
    name,
    description: `test runner: ${name}`,
    detect: (_projectRoot) => detectScore,
    resolveRunnerBinary: (_projectRoot) => `/usr/bin/${name}`,
    invocation: (testFile) => [testFile],
    parseOutcome: (exitCode, stdout, _stderr) => ({
      passed: exitCode === 0,
      testCount: 1,
      details: stdout.slice(-200),
    }),
  };
}

describe("testRunnerRegistry — unit", () => {
  beforeEach(() => {
    _clearTestRunnerRegistry();
  });

  it("starts empty after _clearTestRunnerRegistry()", () => {
    expect(listTestRunners()).toHaveLength(0);
    expect(getTestRunner("vitest")).toBeUndefined();
  });

  it("registerTestRunner + getTestRunner + listTestRunners round-trip", () => {
    const d = makeDescriptor("vitest", 1.0);
    registerTestRunner(d);
    const retrieved = getTestRunner("vitest");
    expect(retrieved).toBeDefined();
    expect(retrieved!.name).toBe("vitest");
    expect(retrieved!.description).toContain("test runner");
    const all = listTestRunners();
    expect(all).toHaveLength(1);
    expect(all[0].name).toBe("vitest");
  });

  it("duplicate name overwrites and logs a warning", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const d1 = makeDescriptor("jest", 1.0);
    const d2 = { ...makeDescriptor("jest", 1.0), description: "overwritten" };
    registerTestRunner(d1);
    registerTestRunner(d2);

    expect(warnSpy).toHaveBeenCalledOnce();
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringContaining('"jest"'),
    );
    expect(getTestRunner("jest")!.description).toBe("overwritten");
    warnSpy.mockRestore();
  });

  it("importing index.ts registerAll() populates 5 descriptors", async () => {
    _clearTestRunnerRegistry();
    const { registerAll } = await import("./index.js");
    registerAll();
    expect(listTestRunners()).toHaveLength(5);
  });

  it("detectTestRunner returns 'none' when all detect scores are 0", () => {
    // Register two runners that return 0 plus the none fallback
    registerTestRunner({ ...makeDescriptor("vitest", 0), detect: () => 0 });
    registerTestRunner({ ...makeDescriptor("jest", 0), detect: () => 0 });
    // Register none manually (score 0.001 — spec says it still wins over 0)
    registerTestRunner({ ...makeDescriptor("none", 0.001), detect: () => 0.001 });

    const winner = detectTestRunner("/tmp/empty-project");
    expect(winner.name).toBe("none");
  });

  it("detectTestRunner returns highest-scoring runner when multiple match", () => {
    registerTestRunner({ ...makeDescriptor("mocha", 0.9), detect: () => 0.9 });
    registerTestRunner({ ...makeDescriptor("vitest", 1.0), detect: () => 1.0 });
    registerTestRunner({ ...makeDescriptor("none", 0.001), detect: () => 0.001 });

    const winner = detectTestRunner("/tmp/some-project");
    expect(winner.name).toBe("vitest");
  });
});
