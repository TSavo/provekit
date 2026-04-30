import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerCapability,
  getCapability,
  getCapabilityColumn,
  listCapabilities,
  _clearRegistry,
} from "./capabilityRegistry.js";
import type { CapabilityDescriptor } from "./capabilityRegistry.js";

// Fake table reference for unit tests — shape only matters to A7b compiler.
const fakeTable = {} as any;

function makeFakeDescriptor(dslName: string, colNames: string[]): CapabilityDescriptor {
  const columns: CapabilityDescriptor["columns"] = {};
  for (const colName of colNames) {
    columns[colName] = {
      dslName: colName,
      drizzleColumn: {},
      isNodeRef: colName.endsWith("_node"),
      nullable: false,
    };
  }
  return { dslName, table: fakeTable, columns };
}

describe("capabilityRegistry — unit", () => {
  beforeEach(() => {
    _clearRegistry();
  });

  it("starts empty after _clearRegistry()", () => {
    expect(listCapabilities()).toHaveLength(0);
    expect(getCapability("arithmetic")).toBeUndefined();
  });

  it("registerCapability stores and getCapability retrieves", () => {
    const d = makeFakeDescriptor("arithmetic", ["node_id", "op", "lhs_node", "rhs_node"]);
    registerCapability(d);
    const retrieved = getCapability("arithmetic");
    expect(retrieved).toBeDefined();
    expect(retrieved!.dslName).toBe("arithmetic");
    expect(retrieved!.table).toBe(fakeTable);
  });

  it("getCapabilityColumn returns the right column by capability+column name", () => {
    const d = makeFakeDescriptor("foo", ["col_a", "col_b", "ref_node"]);
    registerCapability(d);

    const colA = getCapabilityColumn("foo", "col_a");
    expect(colA).toBeDefined();
    expect(colA!.dslName).toBe("col_a");
    expect(colA!.isNodeRef).toBe(false);

    const refNode = getCapabilityColumn("foo", "ref_node");
    expect(refNode).toBeDefined();
    expect(refNode!.isNodeRef).toBe(true);

    expect(getCapabilityColumn("foo", "nonexistent")).toBeUndefined();
    expect(getCapabilityColumn("bar", "col_a")).toBeUndefined();
  });

  it("listCapabilities returns all registered descriptors", () => {
    const d1 = makeFakeDescriptor("cap1", ["node_id"]);
    const d2 = makeFakeDescriptor("cap2", ["node_id", "value"]);
    const d3 = makeFakeDescriptor("cap3", ["node_id"]);
    registerCapability(d1);
    registerCapability(d2);
    registerCapability(d3);

    const all = listCapabilities();
    expect(all).toHaveLength(3);
    expect(all.map((d) => d.dslName)).toContain("cap1");
    expect(all.map((d) => d.dslName)).toContain("cap2");
    expect(all.map((d) => d.dslName)).toContain("cap3");
  });

  it("duplicate registration overwrites and logs a warning", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const d1 = makeFakeDescriptor("dup", ["node_id"]);
    const d2 = makeFakeDescriptor("dup", ["node_id", "extra"]);
    registerCapability(d1);
    registerCapability(d2);

    expect(warnSpy).toHaveBeenCalledOnce();
    expect(getCapability("dup")!.columns).toHaveProperty("extra");
    warnSpy.mockRestore();
  });
});

describe("capabilityRegistry — integration with schema/index.js", () => {
  // Import schema index to trigger all side-effect registrations.
  // We don't clear here — module-level calls already fired when the module loaded;
  // we just verify the count + spot checks.
  beforeEach(async () => {
    // Re-populate in case a previous test cleared the registry.
    _clearRegistry();
    const { registerAll } = await import("./schema/capabilities/index.js");
    registerAll();
  });

  it("importing schema/index.js populates 17 capabilities", () => {
    expect(listCapabilities().length).toBe(17);
  });

  it("all known capability names are present", () => {
    const names = listCapabilities().map((d) => d.dslName);
    const expected = [
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
    for (const name of expected) {
      expect(names).toContain(name);
    }
  });

  it("each registered capability has a non-empty columns map", () => {
    for (const d of listCapabilities()) {
      expect(Object.keys(d.columns).length).toBeGreaterThan(0);
    }
  });

  it("closed-enum columns are populated where expected", () => {
    const opCol = getCapabilityColumn("arithmetic", "op");
    expect(opCol).toBeDefined();
    expect(opCol!.kindEnum).toContain("+");
    expect(opCol!.kindEnum).toContain("/");
    expect(opCol!.kindEnum).toContain("^");

    const assignKind = getCapabilityColumn("assigns", "assign_kind");
    expect(assignKind!.kindEnum).toContain("=");
    expect(assignKind!.kindEnum).toContain("??=");

    const exitKind = getCapabilityColumn("returns", "exit_kind");
    expect(exitKind!.kindEnum).toContain("return");
    expect(exitKind!.kindEnum).toContain("process_exit");

    const signalKind = getCapabilityColumn("signal", "signal_kind");
    expect(signalKind!.kindEnum).toContain("log");
    expect(signalKind!.kindEnum).toContain("throw_message");

    const patternKind = getCapabilityColumn("pattern", "pattern_kind");
    expect(patternKind!.kindEnum).toContain("object");
    expect(patternKind!.kindEnum).toContain("identifier");
  });

  it("isNodeRef is true for *_node FK columns", () => {
    expect(getCapabilityColumn("arithmetic", "lhs_node")!.isNodeRef).toBe(true);
    expect(getCapabilityColumn("arithmetic", "rhs_node")!.isNodeRef).toBe(true);
    expect(getCapabilityColumn("decides", "condition_node")!.isNodeRef).toBe(true);
    expect(getCapabilityColumn("throws", "handler_node")!.isNodeRef).toBe(true);
    expect(getCapabilityColumn("signal_interpolations", "interpolated_node")!.isNodeRef).toBe(true);
  });

  it("boolean columns have sort Bool and isNodeRef false", () => {
    const computed = getCapabilityColumn("member_access", "computed");
    expect(computed!.sort).toBe("Bool");
    expect(computed!.isNodeRef).toBe(false);

    const isInsideHandler = getCapabilityColumn("throws", "is_inside_handler");
    expect(isInsideHandler!.sort).toBe("Bool");

    const executes = getCapabilityColumn("iterates", "executes_at_least_once");
    expect(executes!.sort).toBe("Bool");
  });

  it("nullable is correct for optional FK columns", () => {
    // value_node in returns is nullable
    expect(getCapabilityColumn("returns", "value_node")!.nullable).toBe(true);
    // lhs_node in arithmetic is NOT nullable
    expect(getCapabilityColumn("arithmetic", "lhs_node")!.nullable).toBe(false);
    // rhs_node in assigns IS nullable
    expect(getCapabilityColumn("assigns", "rhs_node")!.nullable).toBe(true);
  });

  it("signal_interpolations slot_index has sort Int", () => {
    const slotIndex = getCapabilityColumn("signal_interpolations", "slot_index");
    expect(slotIndex).toBeDefined();
    expect(slotIndex!.sort).toBe("Int");
    expect(slotIndex!.isNodeRef).toBe(false);
  });
});
