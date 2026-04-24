import { describe, it, expect } from "vitest";
import { join } from "path";
import { fileURLToPath } from "url";
import { parseFile } from "../parser";
import { TemplateEngine } from "./TemplateEngine";

// The worktree root contains .neurallog/principles/
const PROJECT_ROOT = join(fileURLToPath(import.meta.url), "../../../../..");

const FIXTURE = `function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}`;

describe("TemplateEngine.extractBindings for binary_expression (division-by-zero)", () => {
  it("produces bindings mapping SMT constants to source positions", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(FIXTURE);

    // Find the function_declaration node
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "divide", "test.ts");

    // Filter to division-by-zero
    const dzResult = results.find((r) => r.principle === "division-by-zero");
    expect(dzResult).toBeTruthy();

    const { bindings } = dzResult!;

    // Must have at least one binding
    expect(bindings.length).toBeGreaterThan(0);

    // The denominator binding: smt_constant "b", source_expr "b"
    // `a / b` is on line 2 of the fixture (1-indexed)
    const denomBinding = bindings.find((b) => b.smt_constant === "b");
    expect(denomBinding).toBeTruthy();
    expect(denomBinding!.source_expr).toBe("b");
    expect(denomBinding!.source_line).toBe(2);
    // division-by-zero template declares Int
    expect(denomBinding!.sort).toBe("Int");
  });
});

// ---- non_null_expression (null-assertion) ----
// Template: (declare-const {{value}}_is_null Int) (declare-const {{value}}_checked Int)
// For `foo!`, vars.value = "foo", so smt constants are "foo_is_null" and "foo_checked".
// The binding extractor does a prefix match: safeName "foo" -> finds "foo_is_null".
const NULL_ASSERT_FIXTURE = `function get(key: string): string {
  const x = foo!;
  return x;
}`;

describe("TemplateEngine.extractBindings for non_null_expression (null-assertion)", () => {
  it("produces a binding for the asserted value with correct source position", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(NULL_ASSERT_FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "get", "test.ts");
    const result = results.find((r) => r.principle === "null-assertion");
    expect(result).toBeTruthy();

    const { bindings } = result!;
    // Must have a binding for foo_is_null (compound prefix match: vars.value="foo" -> "foo_is_null")
    const isNullBinding = bindings.find((b) => b.smt_constant === "foo_is_null");
    expect(isNullBinding).toBeTruthy();
    // source_expr must be the text of the non_null_expression's firstNamedChild ("foo")
    expect(isNullBinding!.source_expr).toBe("foo");
    // foo! is on line 2 of the fixture
    expect(isNullBinding!.source_line).toBe(2);
    expect(isNullBinding!.sort).toBe("Int");
  });
});

// ---- try_statement (empty-catch) ----
// Template uses bare constants (exception_thrown, etc.) with no {{...}} placeholders.
// instantiateTemplate leaves them as-is; they don't end with _<line> either.
// So bindings MUST be an empty array — just verify no crash.
const EMPTY_CATCH_FIXTURE = `function safe() {
  try { foo(); } catch (e) {}
}`;

describe("TemplateEngine.extractBindings for try_statement (empty-catch)", () => {
  it("does not crash and returns bindings array (possibly empty)", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(EMPTY_CATCH_FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "safe", "test.ts");
    const result = results.find((r) => r.principle === "empty-catch");
    expect(result).toBeTruthy();

    // bindings must be an array (empty is correct for bare-constant templates)
    expect(Array.isArray(result!.bindings)).toBe(true);
  });
});

// ---- assignment_expression (param-mutation) ----
// Template: (declare-const {{prop}}_before Int) (declare-const {{prop}}_after Int)
// For `params.foo = 1`, vars.prop = "foo", smt constants: "foo_before", "foo_after".
// Prefix match: safeName "foo" -> finds "foo_before" and "foo_after".
// astNodeForVar returns the property node ("foo" identifier), source_line = line of mutation.
const PARAM_MUTATION_FIXTURE = `function f(params: any) {
  params.foo = 1;
}`;

describe("TemplateEngine.extractBindings for assignment_expression (param-mutation)", () => {
  it("produces a prop binding with source_expr matching the mutated property name", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(PARAM_MUTATION_FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "f", "test.ts");
    const result = results.find((r) => r.principle === "param-mutation");
    expect(result).toBeTruthy();

    const { bindings } = result!;
    // Prefix match: vars.prop = "foo" -> smt constants "foo_before" and "foo_after"
    const propBeforeBinding = bindings.find((b) => b.smt_constant === "foo_before");
    expect(propBeforeBinding).toBeTruthy();
    // astNodeForVar returns the property child node whose text is "foo"
    expect(propBeforeBinding!.source_expr).toBe("foo");
    // params.foo = 1 is on line 2 of the fixture
    expect(propBeforeBinding!.source_line).toBe(2);
    expect(propBeforeBinding!.sort).toBe("Int");
  });
});

// ---- while_statement (loop-accumulator-overflow) ----
// Template: (declare-const {{accumulator}} Int) ...
// For `total += x`, vars.accumulator = "total", smt constant: "total" (exact match).
// Use while_statement to avoid conflict with empty-collection-loop (which only matches
// for_in_statement and would block loop-accumulator-overflow on the same line).
const LOOP_ACCUMULATOR_FIXTURE = `function sum(items: number[]): number {
  let total = 0;
  let i = 0;
  while (i < items.length) { total += items[i]!; i++; }
  return total;
}`;

describe("TemplateEngine.extractBindings for while_statement (loop-accumulator-overflow)", () => {
  it("produces an accumulator binding pointing to the augmented-assignment LHS", () => {
    const engine = new TemplateEngine(PROJECT_ROOT);
    const tree = parseFile(LOOP_ACCUMULATOR_FIXTURE);
    const root = tree.rootNode;
    const fnNode = root.children.find((c) => c.type === "function_declaration");
    expect(fnNode).toBeTruthy();

    const results = engine.generateProofs(fnNode!, "sum", "test.ts");
    const result = results.find((r) => r.principle === "loop-accumulator-overflow");
    expect(result).toBeTruthy();

    const { bindings } = result!;
    // smt_constant "total" maps to the accumulator (exact match: {{accumulator}} -> "total")
    const accBinding = bindings.find((b) => b.smt_constant === "total");
    expect(accBinding).toBeTruthy();
    // astNodeForVar re-scans the loop body and returns the first augmented-assignment
    // LHS identifier ("total") — first interesting site semantics.
    expect(accBinding!.source_expr).toBe("total");
    // total += items[i]! is on line 4 of the fixture
    expect(accBinding!.source_line).toBe(4);
    expect(accBinding!.sort).toBe("Int");
  });
});
