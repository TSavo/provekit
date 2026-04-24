import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerArtifactKind,
  getArtifactKind,
  listArtifactKinds,
  _clearArtifactKindRegistry,
} from "./artifactKindRegistry.js";
import type { ArtifactKindDescriptor } from "./artifactKindRegistry.js";

function makeDescriptor(name: string, scope: ArtifactKindDescriptor["bundleTypeScope"] = "both"): ArtifactKindDescriptor {
  return {
    name,
    description: `test descriptor: ${name}`,
    oraclesThatApply: [1, 2],
    isPresent: () => false,
    bundleTypeScope: scope,
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

  it("registerAll populates 8 kinds", async () => {
    _clearArtifactKindRegistry();
    const { registerAll } = await import("./artifactKinds/index.js");
    registerAll();
    expect(listArtifactKinds()).toHaveLength(8);
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
