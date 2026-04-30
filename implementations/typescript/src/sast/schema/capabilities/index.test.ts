/**
 * Bulk smoke for all 17 SAST capabilities. After registerAll(), each
 * built-in capability descriptor is available via getCapability(name).
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  _clearRegistry,
  listCapabilities,
  getCapability,
} from "../../capabilityRegistry";
import { registerAll } from "./index";

const EXPECTED = [
  "arithmetic",
  "assigns",
  "returns",
  "member_access",
  "non_null_assertion",
  "truthiness",
  "narrows",
  "decides",
  "iterates",
  "yields",
  "throws",
  "calls",
  "captures",
  "pattern",
  "binding",
  "signal",
  "signal_interpolations",
];

beforeEach(() => {
  _clearRegistry();
});

describe("schema/capabilities registerAll", () => {
  it("registers all 17 SAST capabilities", () => {
    registerAll();
    const names = listCapabilities()
      .map((c) => c.dslName)
      .sort();
    expect(names).toEqual([...EXPECTED].sort());
  });

  it("each capability has a non-empty columns map", () => {
    registerAll();
    for (const name of EXPECTED) {
      const cap = getCapability(name);
      expect(cap, `${name} should be registered`).toBeDefined();
      expect(typeof cap!.columns).toBe("object");
      expect(Object.keys(cap!.columns).length).toBeGreaterThan(0);
    }
  });
});
