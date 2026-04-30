/**
 * Smoke tests for built-in relation registration. Verifies all 10 relations
 * are present after registerBuiltinRelations() is called and that argument
 * validation in compile() rejects malformed input.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  registerRelation,
  getRelation,
  listRelations,
  _clearRelationRegistry,
} from "./relationRegistry.js";
import { registerBuiltinRelations } from "./relations.js";

const EXPECTED_NAMES = [
  "before",
  "dominates",
  "same_value",
  "stale_assignment",
  "encloses",
  "flows_from_param",
  "was_replaced_by_addition",
  "is_in_dirty_set",
  "is_post_added",
  "data_flow_reaches",
];

describe("registerBuiltinRelations", () => {
  beforeEach(() => {
    _clearRelationRegistry();
    registerBuiltinRelations();
  });

  it("registers every expected built-in relation", () => {
    const names = listRelations()
      .map((r) => r.name)
      .sort();
    expect(names).toEqual([...EXPECTED_NAMES].sort());
  });

  it("each builtin has paramTypes and a compile function", () => {
    for (const name of EXPECTED_NAMES) {
      const r = getRelation(name);
      expect(r, `expected ${name} to be registered`).toBeDefined();
      expect(typeof r!.compile).toBe("function");
      expect(Array.isArray(r!.paramTypes)).toBe(true);
    }
  });

  it("before.compile produces SQL referencing source_start", () => {
    const r = getRelation("before")!;
    const sql = r.compile({
      args: [
        { kind: "node", alias: "n1" },
        { kind: "node", alias: "n2" },
      ],
    });
    expect(sql).toContain("source_start");
    expect(sql).toContain("n1");
    expect(sql).toContain("n2");
  });

  it("before.compile throws when args are not node refs", () => {
    const r = getRelation("before")!;
    expect(() =>
      r.compile({
        args: [
          { kind: "literal", value: 1 },
          { kind: "node", alias: "n2" },
        ],
      }),
    ).toThrow(/must be node/);
  });
});
