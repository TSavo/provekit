/**
 * A7c: Relation registry tests.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerRelation,
  getRelation,
  listRelations,
  _clearRelationRegistry,
} from "./relationRegistry.js";

beforeEach(() => {
  _clearRelationRegistry();
});

describe("relationRegistry", () => {
  it("registry starts empty after _clearRelationRegistry()", () => {
    expect(listRelations()).toHaveLength(0);
    expect(getRelation("before")).toBeUndefined();
  });

  it("registerRelation stores and getRelation retrieves (round-trip)", () => {
    const compileFn = vi.fn(() => "1=1");
    registerRelation({
      name: "test_rel",
      paramCount: 1,
      paramTypes: ["node"],
      compile: compileFn,
    });
    const descriptor = getRelation("test_rel");
    expect(descriptor).toBeDefined();
    expect(descriptor!.name).toBe("test_rel");
    expect(descriptor!.paramCount).toBe(1);
    expect(descriptor!.paramTypes).toEqual(["node"]);
  });

  it("listRelations returns all registered relations", () => {
    registerRelation({ name: "rel_a", paramCount: 2, paramTypes: ["node", "node"], compile: () => "a" });
    registerRelation({ name: "rel_b", paramCount: 1, paramTypes: ["node"], compile: () => "b" });
    const names = listRelations().map((r) => r.name);
    expect(names).toContain("rel_a");
    expect(names).toContain("rel_b");
    expect(names).toHaveLength(2);
  });

  it("duplicate registration overwrites idempotently with a console.warn", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    registerRelation({ name: "dup", paramCount: 2, paramTypes: ["node", "node"], compile: () => "first" });
    registerRelation({ name: "dup", paramCount: 2, paramTypes: ["node", "node"], compile: () => "second" });
    expect(warnSpy).toHaveBeenCalledOnce();
    expect(warnSpy.mock.calls[0]![0]).toContain("dup");
    const descriptor = getRelation("dup");
    // The second registration wins.
    expect(descriptor!.compile({ args: [] })).toBe("second");
    warnSpy.mockRestore();
  });

  it("importing relations.js populates the registry with before and dominates", async () => {
    // Ensure the module's side-effect fires after a fresh clear.
    const { registerBuiltinRelations } = await import("./relations.js");
    registerBuiltinRelations();
    const names = listRelations().map((r) => r.name);
    expect(names).toContain("before");
    expect(names).toContain("dominates");
  });

  it("fake relation compile is called with correct arg shapes", () => {
    const compileFn = vi.fn(({ args }: { args: Array<{ kind: string; alias?: string }> }) => {
      return `${args[0]?.alias}.id = ${args[1]?.alias}.id`;
    });
    registerRelation({
      name: "test_two_nodes",
      paramCount: 2,
      paramTypes: ["node", "node"],
      compile: compileFn as any,
    });
    const descriptor = getRelation("test_two_nodes")!;
    const result = descriptor.compile({
      args: [
        { kind: "node", alias: "node_x" },
        { kind: "node", alias: "node_y" },
      ],
    });
    expect(compileFn).toHaveBeenCalledOnce();
    const callArgs = compileFn.mock.calls[0]![0];
    expect(callArgs.args[0]).toEqual({ kind: "node", alias: "node_x" });
    expect(callArgs.args[1]).toEqual({ kind: "node", alias: "node_y" });
    expect(result).toBe("node_x.id = node_y.id");
  });
});

describe("built-in relations SQL output", () => {
  beforeEach(async () => {
    // Re-register built-ins after the global clear.
    const { registerBuiltinRelations } = await import("./relations.js");
    registerBuiltinRelations();
  });

  it("before produces correct SQL fragment", () => {
    const descriptor = getRelation("before")!;
    expect(descriptor).toBeDefined();
    const sql = descriptor.compile({
      args: [
        { kind: "node", alias: "node_guard" },
        { kind: "node", alias: "node_target" },
      ],
    });
    expect(sql).toContain("node_guard.source_start < node_target.source_start");
    expect(sql).toContain("node_guard.file_id = node_target.file_id");
  });

  it("dominates produces correct SQL fragment", () => {
    const descriptor = getRelation("dominates")!;
    expect(descriptor).toBeDefined();
    const sql = descriptor.compile({
      args: [
        { kind: "node", alias: "node_guard" },
        { kind: "node", alias: "node_target" },
      ],
    });
    expect(sql).toContain("SELECT 1 FROM dominance");
    expect(sql).toContain("dominator = node_guard.id");
    expect(sql).toContain("dominated = node_target.id");
  });

  it("before throws if arg kind is not node", () => {
    const descriptor = getRelation("before")!;
    expect(() =>
      descriptor.compile({
        args: [
          { kind: "literal", value: "oops" },
          { kind: "node", alias: "node_target" },
        ],
      }),
    ).toThrow("before: both args must be node");
  });
});
