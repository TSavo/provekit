/**
 * A7b: Compiler tests.
 *
 * These tests compile ASTs but do NOT execute them — no DB required.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { parseDSL } from "./parser.js";
import { compilePrinciple, compileProgram, CompileError } from "./compiler.js";
import { _clearRegistry, registerCapability } from "../sast/capabilityRegistry.js";
import { registerAll } from "../sast/schema/capabilities/index.js";

// ---------------------------------------------------------------------------
// DSL source for division-by-zero (full).
// ---------------------------------------------------------------------------

const DIV_BY_ZERO_SRC = `
predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) before $div
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
`.trim();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

beforeEach(() => {
  _clearRegistry();
  registerAll();
});

afterEach(() => {
  // No cleanup needed (registry cleared in beforeEach).
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("compilePrinciple", () => {
  it("compiles division-by-zero and returns a query function without throwing", () => {
    const program = parseDSL(DIV_BY_ZERO_SRC);
    const queries = compileProgram(program.nodes);
    expect(queries.has("division-by-zero")).toBe(true);
    const fn = queries.get("division-by-zero");
    expect(typeof fn).toBe("function");
  });

  it("compile-time error: unknown capability", () => {
    const src = `
principle test {
  match $x: node where unknown_capability.foo == "bar"
  report violation { at $x captures { x: $x } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    const pred = program.nodes[0];
    if (pred.kind !== "principle") throw new Error("expected principle");
    const predicates = new Map();
    expect(() => compilePrinciple(pred, predicates)).toThrow(CompileError);
    try {
      compilePrinciple(pred, predicates);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      expect((e as CompileError).message).toContain("unknown_capability");
    }
  });

  it("compile-time error: unknown column on known capability (did-you-mean)", () => {
    const src = `
principle test {
  match $x: node where arithmetic.opp == "/"
  report violation { at $x captures { x: $x } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    const pred = program.nodes[0];
    if (pred.kind !== "principle") throw new Error("expected principle");
    const predicates = new Map();
    expect(() => compilePrinciple(pred, predicates)).toThrow(CompileError);
    try {
      compilePrinciple(pred, predicates);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      const msg = (e as CompileError).message;
      expect(msg).toContain("'opp'");
      expect(msg).toContain("arithmetic");
      expect(msg).toContain("Did you mean 'op'?");
    }
  });

  it("compile-time error: enum violation on arithmetic.op", () => {
    const src = `
principle test {
  match $x: node where arithmetic.op == "modulo"
  report violation { at $x captures { x: $x } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    const pred = program.nodes[0];
    if (pred.kind !== "principle") throw new Error("expected principle");
    const predicates = new Map();
    expect(() => compilePrinciple(pred, predicates)).toThrow(CompileError);
    try {
      compilePrinciple(pred, predicates);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      const msg = (e as CompileError).message;
      expect(msg).toContain("'modulo'");
      expect(msg).toContain("arithmetic.op");
    }
  });

  it("compile-time error: unbound variable in require clause relation", () => {
    const src = `
predicate zg($var: node) {
  match $g: node where narrows.target_node == $var
}

principle test {
  match $div: node where arithmetic.op == "/"
  require no $guard: zg($div.arithmetic.rhs_node) before $unbound
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(() => compileProgram(program.nodes)).toThrow(CompileError);
    try {
      compileProgram(program.nodes);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      expect((e as CompileError).message).toContain("$unbound");
    }
  });

  it("compile-time error: unbound variable in varDeref predicate argument", () => {
    const src = `
predicate zg($var: node) {
  match $g: node where narrows.target_node == $var
}

principle test {
  match $div: node where arithmetic.op == "/"
  require no $guard: zg($missing.arithmetic.rhs_node) before $div
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(() => compileProgram(program.nodes)).toThrow(CompileError);
    try {
      compileProgram(program.nodes);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      expect((e as CompileError).message).toContain("$missing");
    }
  });

  it("compile-time error: unknown relation name lists registered relations", () => {
    const src = `
predicate zg($var: node) {
  match $g: node where narrows.target_node == $var
}

principle test {
  match $div: node where arithmetic.op == "/"
  require no $guard: zg($div.arithmetic.rhs_node) reaches $div
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(() => compileProgram(program.nodes)).toThrow(CompileError);
    try {
      compileProgram(program.nodes);
    } catch (e) {
      expect(e).toBeInstanceOf(CompileError);
      const msg = (e as CompileError).message;
      expect(msg).toContain("reaches");
      // Registered names should be listed in the error.
      expect(msg).toContain("before");
      expect(msg).toContain("dominates");
      expect(msg).toContain("same_value");
    }
  });

  it("compileProgram: returns empty map for source with only predicates", () => {
    const src = `
predicate only_pred($var: node) {
  match $g: node where narrows.target_node == $var
}
    `.trim();
    const program = parseDSL(src);
    const queries = compileProgram(program.nodes);
    expect(queries.size).toBe(0);
  });

  it("compileProgram: registers custom capability via registry and resolves it", async () => {
    // Register a mock capability for testing.
    const { sqliteTable, text } = await import("drizzle-orm/sqlite-core");
    const mockTable = sqliteTable("mock_cap_table", {
      nodeId: text("node_id").primaryKey(),
      kind: text("kind").notNull(),
    });
    registerCapability({
      dslName: "mock_cap",
      table: mockTable,
      columns: {
        node_id: { dslName: "node_id", drizzleColumn: mockTable.nodeId, isNodeRef: true, nullable: false },
        kind: { dslName: "kind", drizzleColumn: mockTable.kind, isNodeRef: false, nullable: false, kindEnum: ["A", "B"] },
      },
    });
    const src = `
principle mock-test {
  match $x: node where mock_cap.kind == "A"
  report info { at $x captures { x: $x } message "mock" }
}
    `.trim();
    const program = parseDSL(src);
    const queries = compileProgram(program.nodes);
    expect(queries.has("mock-test")).toBe(true);
  });

  it("compiles same_value relation with varDeref target without throwing", () => {
    const src = `
predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero-sv {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) same_value $div.arithmetic.rhs_node
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
    `.trim();
    const program = parseDSL(src);
    const queries = compileProgram(program.nodes);
    expect(queries.has("division-by-zero-sv")).toBe(true);
    const fn = queries.get("division-by-zero-sv");
    expect(typeof fn).toBe("function");
  });
});
