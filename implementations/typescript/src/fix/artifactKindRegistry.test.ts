import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerArtifactKind,
  getArtifactKind,
  listArtifactKinds,
  _clearArtifactKindRegistry,
} from "./artifactKindRegistry.js";
import type { ArtifactKindDescriptor } from "./artifactKindRegistry.js";
import type { FixBundle } from "./types.js";

function makeDescriptor(name: string, scope: ArtifactKindDescriptor["bundleTypeScope"] = "both"): ArtifactKindDescriptor {
  return {
    name,
    description: `test descriptor: ${name}`,
    oraclesThatApply: [1, 2],
    isPresent: () => false,
    bundleTypeScope: scope,
  };
}

type Artifacts = FixBundle["artifacts"];

function makeArtifacts(overrides: Partial<Artifacts> = {}): Artifacts {
  return {
    primaryFix: null,
    complementary: [],
    test: null,
    principle: null,
    capabilitySpec: null,
    ...overrides,
  };
}

function makePrimaryFix(files: string[]): Artifacts["primaryFix"] {
  return {
    patch: {
      fileEdits: files.map((f) => ({ file: f, newContent: "x" })),
      description: "test",
    },
    llmRationale: "test",
    llmConfidence: 0.9,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 0,
      overlayClosed: false,
    },
  };
}

function makeComplementary(kind: import("./types.js").ComplementarySiteKind): import("./types.js").ComplementaryChange {
  return {
    kind,
    targetNodeId: "node-001",
    patch: { fileEdits: [{ file: "src/foo.ts", newContent: "x" }], description: "comp" },
    rationale: "test",
    verifiedAgainstOverlay: true,
    overlayZ3Verdict: "unsat",
    priority: 1,
    audit: { siteKind: kind, discoveredVia: "llm_reflection", z3RunMs: 0 },
  };
}

describe("artifactKindRegistry — unit", () => {
  beforeEach(() => {
    _clearArtifactKindRegistry();
  });

  it("starts empty after _clearArtifactKindRegistry()", () => {
    expect(listArtifactKinds()).toHaveLength(0);
    expect(getArtifactKind("code_patch")).toBeUndefined();
  });

  it("registerArtifactKind + getArtifactKind round-trip", () => {
    const d = makeDescriptor("code_patch");
    registerArtifactKind(d);
    const retrieved = getArtifactKind("code_patch");
    expect(retrieved).toBeDefined();
    expect(retrieved!.name).toBe("code_patch");
    expect(retrieved!.bundleTypeScope).toBe("both");
  });

  it("listArtifactKinds returns all registered descriptors", () => {
    registerArtifactKind(makeDescriptor("code_patch"));
    registerArtifactKind(makeDescriptor("regression_test"));
    registerArtifactKind(makeDescriptor("principle_candidate"));
    const all = listArtifactKinds();
    expect(all).toHaveLength(3);
    expect(all.map((d) => d.name)).toContain("code_patch");
    expect(all.map((d) => d.name)).toContain("regression_test");
    expect(all.map((d) => d.name)).toContain("principle_candidate");
  });

  it("duplicate registration overwrites and logs a warning", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const d1 = makeDescriptor("code_patch");
    const d2 = { ...makeDescriptor("code_patch"), description: "overwritten" };
    registerArtifactKind(d1);
    registerArtifactKind(d2);

    expect(warnSpy).toHaveBeenCalledOnce();
    expect(getArtifactKind("code_patch")!.description).toBe("overwritten");
    warnSpy.mockRestore();
  });

  it("registerAll populates 14 kinds", async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
    expect(listArtifactKinds()).toHaveLength(14);
  });

  it("capability_spec has bundleTypeScope 'substrate'", async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
    const spec = getArtifactKind("capability_spec");
    expect(spec).toBeDefined();
    expect(spec!.bundleTypeScope).toBe("substrate");
  });

  it("active kinds (code_patch, regression_test, principle_candidate, complementary_change, capability_spec) have at least 1 oracle", async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
    const activeKinds = ["code_patch", "regression_test", "principle_candidate", "complementary_change", "capability_spec"];
    for (const name of activeKinds) {
      const d = getArtifactKind(name);
      expect(d).toBeDefined();
      expect(d!.oraclesThatApply.length).toBeGreaterThan(0);
    }
  });
});

describe("artifactKindRegistry — un-stubbed kind isPresent logic", () => {
  beforeEach(async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
  });

  it("startup_assert: present when complementary has kind startup_assert", () => {
    const d = getArtifactKind("startup_assert")!;
    const withAssert = makeArtifacts({ complementary: [makeComplementary("startup_assert")] });
    const without = makeArtifacts({ complementary: [makeComplementary("observability")] });
    expect(d.isPresent(withAssert)).toBe(true);
    expect(d.isPresent(without)).toBe(false);
  });

  it("observability_hook: present when complementary has kind observability", () => {
    const d = getArtifactKind("observability_hook")!;
    const withObs = makeArtifacts({ complementary: [makeComplementary("observability")] });
    const without = makeArtifacts({ complementary: [makeComplementary("startup_assert")] });
    expect(d.isPresent(withObs)).toBe(true);
    expect(d.isPresent(without)).toBe(false);
  });

  it("documentation: present when primaryFix touches a .md file", () => {
    const d = getArtifactKind("documentation")!;
    const withDoc = makeArtifacts({ primaryFix: makePrimaryFix(["docs/guide.md"]) });
    const withoutDoc = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    const empty = makeArtifacts();
    expect(d.isPresent(withDoc)).toBe(true);
    expect(d.isPresent(withoutDoc)).toBe(false);
    expect(d.isPresent(empty)).toBe(false);
  });

  it("documentation: present when complementary touches a README file", () => {
    const d = getArtifactKind("documentation")!;
    const comp = makeComplementary("adjacent_site_fix");
    comp.patch.fileEdits = [{ file: "README.md", newContent: "x" }];
    const withReadme = makeArtifacts({ complementary: [comp] });
    expect(d.isPresent(withReadme)).toBe(true);
  });
});

describe("artifactKindRegistry — new kind isPresent logic", () => {
  beforeEach(async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
  });

  it("test_fix: present when primaryFix touches only test files", () => {
    const d = getArtifactKind("test_fix")!;
    const onlyTests = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.test.ts", "src/bar.spec.ts"]) });
    const mixedFiles = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.test.ts", "src/foo.ts"]) });
    const noFix = makeArtifacts();
    expect(d.isPresent(onlyTests)).toBe(true);
    expect(d.isPresent(mixedFiles)).toBe(false);
    expect(d.isPresent(noFix)).toBe(false);
  });

  it("config_update: present when primaryFix touches package.json (nested path)", () => {
    const d = getArtifactKind("config_update")!;
    const withConfig = makeArtifacts({ primaryFix: makePrimaryFix(["packages/foo/package.json"]) });
    const withTsconfig = makeArtifacts({ primaryFix: makePrimaryFix(["tsconfig.json"]) });
    const withEnv = makeArtifacts({ primaryFix: makePrimaryFix([".env.production"]) });
    const withDeploy = makeArtifacts({ primaryFix: makePrimaryFix(["deploy/k8s.yaml"]) });
    const noConfig = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    expect(d.isPresent(withConfig)).toBe(true);
    expect(d.isPresent(withTsconfig)).toBe(true);
    expect(d.isPresent(withEnv)).toBe(true);
    expect(d.isPresent(withDeploy)).toBe(true);
    expect(d.isPresent(noConfig)).toBe(false);
  });

  it("dependency_update: present when primaryFix touches lockfiles", () => {
    const d = getArtifactKind("dependency_update")!;
    const withLock = makeArtifacts({ primaryFix: makePrimaryFix(["pnpm-lock.yaml"]) });
    const withYarn = makeArtifacts({ primaryFix: makePrimaryFix(["yarn.lock"]) });
    const withPkg = makeArtifacts({ primaryFix: makePrimaryFix(["package.json"]) });
    const noLock = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    expect(d.isPresent(withLock)).toBe(true);
    expect(d.isPresent(withYarn)).toBe(true);
    expect(d.isPresent(withPkg)).toBe(true);
    expect(d.isPresent(noLock)).toBe(false);
  });

  it("prompt_update: present when primaryFix touches files in prompts/ dir", () => {
    const d = getArtifactKind("prompt_update")!;
    const withPrompt = makeArtifacts({ primaryFix: makePrimaryFix(["src/prompts/fix.ts"]) });
    const withSystemPrompt = makeArtifacts({ primaryFix: makePrimaryFix(["src/systemPromptBuilder.ts"]) });
    const noPrompt = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    expect(d.isPresent(withPrompt)).toBe(true);
    expect(d.isPresent(withSystemPrompt)).toBe(true);
    expect(d.isPresent(noPrompt)).toBe(false);
  });

  it("lint_rule: present when primaryFix touches lint config files", () => {
    const d = getArtifactKind("lint_rule")!;
    const withEslint = makeArtifacts({ primaryFix: makePrimaryFix([".eslintrc.js"]) });
    const withBiome = makeArtifacts({ primaryFix: makePrimaryFix(["biome.json"]) });
    const withEslintConfig = makeArtifacts({ primaryFix: makePrimaryFix(["eslint.config.ts"]) });
    const noLint = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    expect(d.isPresent(withEslint)).toBe(true);
    expect(d.isPresent(withBiome)).toBe(true);
    expect(d.isPresent(withEslintConfig)).toBe(true);
    expect(d.isPresent(noLint)).toBe(false);
  });

  it("migration_fix: present when primaryFix adds file in drizzle/ or migrations/", () => {
    const d = getArtifactKind("migration_fix")!;
    const withDrizzle = makeArtifacts({ primaryFix: makePrimaryFix(["drizzle/0042_add_index.sql"]) });
    const withNestedDrizzle = makeArtifacts({ primaryFix: makePrimaryFix(["packages/db/drizzle/0001.sql"]) });
    const withMigrations = makeArtifacts({ primaryFix: makePrimaryFix(["migrations/20240101_fix.ts"]) });
    const noMigration = makeArtifacts({ primaryFix: makePrimaryFix(["src/foo.ts"]) });
    expect(d.isPresent(withDrizzle)).toBe(true);
    expect(d.isPresent(withNestedDrizzle)).toBe(true);
    expect(d.isPresent(withMigrations)).toBe(true);
    expect(d.isPresent(noMigration)).toBe(false);
  });
});
