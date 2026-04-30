/**
 * Test runner detection tests — file-system heuristics.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import {
  detectTestRunner,
  listTestRunners,
  _clearTestRunnerRegistry,
} from "./registry.js";

// We need real adapters registered for detection to work
import { registerAll } from "./index.js";

let tmpDir: string;

describe("detectTestRunner — filesystem heuristics", () => {
  beforeEach(() => {
    _clearTestRunnerRegistry();
    registerAll();
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-detect-"));
  });

  afterEach(() => {
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it("vitest.config.ts in projectRoot → vitest selected", () => {
    writeFileSync(join(tmpDir, "vitest.config.ts"), 'export default {};\n', "utf8");
    const runner = detectTestRunner(tmpDir);
    expect(runner.name).toBe("vitest");
  });

  it("jest.config.js in projectRoot → jest selected", () => {
    writeFileSync(join(tmpDir, "jest.config.js"), 'module.exports = {};\n', "utf8");
    const runner = detectTestRunner(tmpDir);
    expect(runner.name).toBe("jest");
  });

  it("package.json with scripts.test containing 'vitest' → vitest selected", () => {
    writeFileSync(
      join(tmpDir, "package.json"),
      JSON.stringify({ scripts: { test: "vitest run" } }),
      "utf8",
    );
    const runner = detectTestRunner(tmpDir);
    expect(runner.name).toBe("vitest");
  });

  it("empty projectRoot (no config files, no package.json) → 'none' selected", () => {
    const runner = detectTestRunner(tmpDir);
    expect(runner.name).toBe("none");
  });

  it("both vitest.config.ts and jest.config.js present → vitest wins (higher confidence)", () => {
    writeFileSync(join(tmpDir, "vitest.config.ts"), 'export default {};\n', "utf8");
    writeFileSync(join(tmpDir, "jest.config.js"), 'module.exports = {};\n', "utf8");
    const runner = detectTestRunner(tmpDir);
    // Both score 1.0 on config-file check. vitest should still be selected
    // (deterministic: first in registration order when tied, which is vitest).
    // Both score 1.0, sort is stable, vitest was registered first.
    expect(runner.name).toBe("vitest");
  });

  it("listTestRunners returns exactly 5 after registerAll", () => {
    expect(listTestRunners()).toHaveLength(5);
    const names = listTestRunners().map((r) => r.name);
    expect(names).toContain("vitest");
    expect(names).toContain("jest");
    expect(names).toContain("mocha");
    expect(names).toContain("node-test");
    expect(names).toContain("none");
  });
});
