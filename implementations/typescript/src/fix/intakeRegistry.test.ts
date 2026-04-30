import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerIntakeAdapter,
  getIntakeAdapter,
  listIntakeAdapters,
  _clearIntakeRegistry,
} from "./intakeRegistry.js";
import type { IntakeAdapter } from "./intakeRegistry.js";
import type { BugSignal } from "./types.js";

// Minimal stub adapter for unit tests.
function makeAdapter(name: string): IntakeAdapter {
  return {
    name,
    description: `test adapter: ${name}`,
    detect: () => 0.5,
    parse: async (): Promise<BugSignal> => ({
      source: name,
      rawText: "",
      summary: "",
      failureDescription: "",
      codeReferences: [],
    }),
  };
}

describe("intakeRegistry — unit", () => {
  beforeEach(() => {
    _clearIntakeRegistry();
  });

  it("starts empty after _clearIntakeRegistry()", () => {
    expect(listIntakeAdapters()).toHaveLength(0);
    expect(getIntakeAdapter("report")).toBeUndefined();
  });

  it("registerIntakeAdapter + getIntakeAdapter round-trip", () => {
    const a = makeAdapter("report");
    registerIntakeAdapter(a);
    const retrieved = getIntakeAdapter("report");
    expect(retrieved).toBeDefined();
    expect(retrieved!.name).toBe("report");
    expect(retrieved!.description).toContain("test adapter");
  });

  it("listIntakeAdapters returns all registered adapters", () => {
    registerIntakeAdapter(makeAdapter("report"));
    registerIntakeAdapter(makeAdapter("gap_report"));
    registerIntakeAdapter(makeAdapter("test_failure"));
    const all = listIntakeAdapters();
    expect(all).toHaveLength(3);
    expect(all.map((a) => a.name)).toContain("report");
    expect(all.map((a) => a.name)).toContain("gap_report");
    expect(all.map((a) => a.name)).toContain("test_failure");
  });

  it("duplicate registration overwrites and logs a warning", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const a1 = makeAdapter("report");
    const a2 = { ...makeAdapter("report"), description: "overwritten" };
    registerIntakeAdapter(a1);
    registerIntakeAdapter(a2);

    expect(warnSpy).toHaveBeenCalledOnce();
    expect(getIntakeAdapter("report")!.description).toBe("overwritten");
    warnSpy.mockRestore();
  });

  it("getIntakeAdapter returns undefined for unknown names", () => {
    expect(getIntakeAdapter("unknown_source")).toBeUndefined();
    expect(getIntakeAdapter("")).toBeUndefined();
  });

  it("importing src/fix/intake.js populates the four v1 adapters", async () => {
    // Fresh clear — module self-registrations have already fired; use registerAll.
    _clearIntakeRegistry();
    const { registerAll } = await import("./intakeAdapters/index.js");
    registerAll();
    const names = listIntakeAdapters().map((a) => a.name);
    expect(names).toHaveLength(4);
    expect(names).toContain("report");
    expect(names).toContain("gap_report");
    expect(names).toContain("test_failure");
    expect(names).toContain("runtime_log");
  });

  it("listIntakeAdapters returns a read-only snapshot (adding to returned array does not affect registry)", () => {
    registerIntakeAdapter(makeAdapter("report"));
    const snapshot = listIntakeAdapters() as IntakeAdapter[];
    const originalLength = snapshot.length;
    // Pushing to the snapshot should not affect the registry.
    (snapshot as IntakeAdapter[]).push(makeAdapter("injected"));
    expect(listIntakeAdapters()).toHaveLength(originalLength);
  });
});
