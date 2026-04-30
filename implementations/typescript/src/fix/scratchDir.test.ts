import { describe, it, expect, afterEach, beforeEach } from "vitest";
import { mkdtempSync, rmSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { createScratchDir } from "./scratchDir";

const RESET_KEYS = [
  "PROVEKIT_SCRATCH_PARENT",
  "PROVEKIT_SCRATCH_PARENT_PROVEKIT_APPLY",
  "PROVEKIT_SCRATCH_PARENT_REGRESSION",
];

const created: string[] = [];

beforeEach(() => {
  for (const k of RESET_KEYS) delete process.env[k];
});

afterEach(() => {
  while (created.length > 0) {
    const d = created.pop()!;
    try {
      rmSync(d, { recursive: true, force: true });
    } catch {}
  }
  for (const k of RESET_KEYS) delete process.env[k];
});

function track(d: string): string {
  created.push(d);
  return d;
}

describe("createScratchDir", () => {
  it("creates a directory whose name starts with the prefix", () => {
    const dir = track(createScratchDir("provekit-apply-"));
    expect(existsSync(dir)).toBe(true);
    expect(dir).toContain("provekit-apply-");
  });

  it("explicit parent argument wins over env vars", () => {
    const customRoot = track(mkdtempSync(join(tmpdir(), "custom-root-")));
    process.env.PROVEKIT_SCRATCH_PARENT = "/should/not/be/used";

    const dir = track(createScratchDir("provekit-apply-", customRoot));
    expect(dirname(dir)).toBe(customRoot);
  });

  it("PROVEKIT_SCRATCH_PARENT redirects creation when no explicit parent", () => {
    const overrideRoot = track(mkdtempSync(join(tmpdir(), "override-root-")));
    process.env.PROVEKIT_SCRATCH_PARENT = overrideRoot;

    const dir = track(createScratchDir("regression-"));
    expect(dirname(dir)).toBe(overrideRoot);
  });

  it("per-stage env var resolves from the prefix", () => {
    const stageRoot = track(mkdtempSync(join(tmpdir(), "stage-root-")));
    process.env.PROVEKIT_SCRATCH_PARENT_PROVEKIT_APPLY = stageRoot;

    const dir = track(createScratchDir("provekit-apply-"));
    expect(dirname(dir)).toBe(stageRoot);
  });

  it("PROVEKIT_SCRATCH_PARENT wins over per-stage env var", () => {
    const globalRoot = track(mkdtempSync(join(tmpdir(), "global-root-")));
    const stageRoot = track(mkdtempSync(join(tmpdir(), "stage-root-")));
    process.env.PROVEKIT_SCRATCH_PARENT = globalRoot;
    process.env.PROVEKIT_SCRATCH_PARENT_PROVEKIT_APPLY = stageRoot;

    const dir = track(createScratchDir("provekit-apply-"));
    expect(dirname(dir)).toBe(globalRoot);
  });

  it("falls back to os.tmpdir() when no env or explicit parent given", () => {
    const dir = track(createScratchDir("regression-"));
    expect(dirname(dir)).toBe(tmpdir());
  });
});
