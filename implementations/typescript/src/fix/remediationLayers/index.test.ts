/**
 * B3 layer-registry bulk smoke. Mirrors the intakeAdapters index test shape:
 * after registerAll(), all eight built-in remediation layers are registered.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  _clearRemediationLayerRegistry,
  listRemediationLayers,
  getRemediationLayer,
} from "../remediationLayerRegistry";
import { registerAll } from "./index";

const EXPECTED = [
  "code_invariant",
  "config",
  "infrastructure",
  "data_ingress",
  "timing",
  "spec_drift",
  "observability",
  "out_of_scope",
];

beforeEach(() => {
  _clearRemediationLayerRegistry();
});

describe("remediationLayers registerAll", () => {
  it("registers all eight built-in layers", () => {
    registerAll();
    const names = listRemediationLayers()
      .map((l) => l.name)
      .sort();
    expect(names).toEqual([...EXPECTED].sort());
  });

  it("each layer has the load-bearing descriptor fields populated", () => {
    registerAll();
    for (const name of EXPECTED) {
      const l = getRemediationLayer(name);
      expect(l, `${name} should be present`).toBeDefined();
      expect(typeof l!.description).toBe("string");
      expect(l!.description.length).toBeGreaterThan(0);
      expect(typeof l!.promptHint).toBe("string");
      expect(Array.isArray(l!.artifactKinds)).toBe(true);
    }
  });

  it("each layer's promptHint stays under the 500-char advisory budget", () => {
    registerAll();
    for (const name of EXPECTED) {
      const l = getRemediationLayer(name)!;
      expect(l.promptHint.length, `${name}.promptHint`).toBeLessThanOrEqual(500);
    }
  });
});
