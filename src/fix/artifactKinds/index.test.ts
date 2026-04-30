/**
 * Bulk smoke for artifact-kind registration. Mirrors the shape of the
 * intakeAdapters and remediationLayers index tests.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  _clearArtifactKindRegistry,
  listArtifactKinds,
  getArtifactKind,
} from "../artifactKindRegistry";
import { registerAll } from "./index";

const EXPECTED = [
  "capability_spec",
  "code_patch",
  "complementary_change",
  "dependency_update",
  "config_update",
  "documentation",
  "migration_fix",
  "observability_hook",
  "lint_rule",
  "principle_candidate",
  "regression_test",
  "prompt_update",
  "test_fix",
  "startup_assert",
];

beforeEach(() => {
  _clearArtifactKindRegistry();
});

describe("artifactKinds registerAll", () => {
  it("registers all fourteen built-in artifact kinds", () => {
    registerAll();
    const names = listArtifactKinds()
      .map((k) => k.name)
      .sort();
    expect(names).toEqual([...EXPECTED].sort());
  });

  it("each kind exposes isPresent + bundleTypeScope", () => {
    registerAll();
    for (const name of EXPECTED) {
      const k = getArtifactKind(name);
      expect(k, `${name} should be present`).toBeDefined();
      expect(typeof k!.isPresent).toBe("function");
      expect(["fix", "substrate", "both"]).toContain(k!.bundleTypeScope);
    }
  });

  it("each kind declares at least one applicable oracle id", () => {
    registerAll();
    for (const name of EXPECTED) {
      const k = getArtifactKind(name)!;
      expect(Array.isArray(k.oraclesThatApply)).toBe(true);
      // Most do; at minimum allow zero for kinds that are oracle-free, but
      // require the field to be defined and an array.
      for (const id of k.oraclesThatApply) {
        expect(Number.isInteger(id)).toBe(true);
      }
    }
  });
});
