import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerRemediationLayer,
  getRemediationLayer,
  listRemediationLayers,
  _clearRemediationLayerRegistry,
} from "./remediationLayerRegistry.js";
import type { RemediationLayerDescriptor } from "./remediationLayerRegistry.js";

function makeDescriptor(name: string): RemediationLayerDescriptor {
  return {
    name,
    description: `test layer: ${name}`,
    promptHint: `Use this layer when ${name} is the issue.`,
    artifactKinds: ["code_patch"],
  };
}

describe("remediationLayerRegistry — unit", () => {
  beforeEach(() => {
    _clearRemediationLayerRegistry();
  });

  it("starts empty after _clearRemediationLayerRegistry()", () => {
    expect(listRemediationLayers()).toHaveLength(0);
    expect(getRemediationLayer("code_invariant")).toBeUndefined();
  });

  it("registerRemediationLayer + getRemediationLayer round-trip", () => {
    const d = makeDescriptor("code_invariant");
    registerRemediationLayer(d);
    const retrieved = getRemediationLayer("code_invariant");
    expect(retrieved).toBeDefined();
    expect(retrieved!.name).toBe("code_invariant");
    expect(retrieved!.description).toContain("test layer");
  });

  it("listRemediationLayers returns all registered layers", () => {
    registerRemediationLayer(makeDescriptor("code_invariant"));
    registerRemediationLayer(makeDescriptor("config"));
    registerRemediationLayer(makeDescriptor("infrastructure"));
    const all = listRemediationLayers();
    expect(all).toHaveLength(3);
    const names = all.map((l) => l.name);
    expect(names).toContain("code_invariant");
    expect(names).toContain("config");
    expect(names).toContain("infrastructure");
  });

  it("duplicate registration overwrites and logs a warning", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const d1 = makeDescriptor("code_invariant");
    const d2 = { ...makeDescriptor("code_invariant"), description: "overwritten" };
    registerRemediationLayer(d1);
    registerRemediationLayer(d2);

    expect(warnSpy).toHaveBeenCalledOnce();
    expect(getRemediationLayer("code_invariant")!.description).toBe("overwritten");
    warnSpy.mockRestore();
  });

  it("importing remediationLayers/index.js populates 8 layers", async () => {
    _clearRemediationLayerRegistry();
    const { registerAll } = await import("./remediationLayers/index.js");
    registerAll();
    expect(listRemediationLayers()).toHaveLength(8);
  });

  it("canTriggerSubstrateExtension === true only for code_invariant", async () => {
    _clearRemediationLayerRegistry();
    const { registerAll } = await import("./remediationLayers/index.js");
    registerAll();
    const layers = listRemediationLayers();
    for (const layer of layers) {
      if (layer.name === "code_invariant") {
        expect(layer.canTriggerSubstrateExtension).toBe(true);
      } else {
        expect(layer.canTriggerSubstrateExtension).not.toBe(true);
      }
    }
  });
});
