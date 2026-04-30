/**
 * Smoke test for the bulk-register helper. After registerAll(), all four
 * built-in adapters must be present in the registry by name.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  _clearIntakeRegistry,
  listIntakeAdapters,
  getIntakeAdapter,
} from "../intakeRegistry";
import { registerAll } from "./index";

beforeEach(() => {
  _clearIntakeRegistry();
});

describe("intakeAdapters registerAll", () => {
  it("registers all four built-in adapters", () => {
    registerAll();
    const names = listIntakeAdapters()
      .map((a) => a.name)
      .sort();
    expect(names).toEqual(["gap_report", "report", "runtime_log", "test_failure"]);
  });

  it("each registered adapter exposes a parse function", () => {
    registerAll();
    for (const name of ["report", "gap_report", "runtime_log", "test_failure"]) {
      const a = getIntakeAdapter(name);
      expect(a, `${name} should be present`).toBeDefined();
      expect(typeof a!.parse).toBe("function");
    }
  });

  it("the report adapter has a fallback detect score (>= 0.5)", () => {
    registerAll();
    const a = getIntakeAdapter("report")!;
    const score = a.detect ? a.detect({ text: "any free-form bug text" }) : 1;
    expect(typeof score === "number" || typeof score === "boolean").toBe(true);
    if (typeof score === "number") {
      expect(score).toBeGreaterThanOrEqual(0.5);
    }
  });
});
