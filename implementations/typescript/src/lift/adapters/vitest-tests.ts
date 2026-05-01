/**
 * provekit-lift adapter: vitest unit tests.
 *
 * Walks `it("name", () => { expect(<actual>).toBe(<expected>) })` blocks
 * and lifts each `expect(...)` matcher invocation to its OWN
 * point-specific contract memento.
 *
 * THE FRAMING:
 *
 *   A unit test is a point-specific predicate: "at this input, this
 *   output." A property test is a universal predicate: "forall input,
 *   this property." Both are content-addressable behavior witnesses.
 *   ProvekIt lifts both. Every passing test in your codebase becomes a
 *   content-addressed signed contract memento. Test authors don't need
 *   to write contracts; they already wrote the contracts. We just
 *   promote them.
 *
 * SHAPE (v0):
 *
 *   it("name", () => {
 *     expect(parseInt("42")).toBe(42);          // -> name::0
 *     expect(parseInt("42")).toEqual(42);       // -> name::1
 *     expect(count).toBeGreaterThan(0);         // -> name::2
 *   });
 *
 * Each `expect(actual).<matcher>(expected)` call lifts to:
 *   contract "<test_name>::<index>" { inv: <atomic-formula> }
 *
 * Supported matchers (v0): toBe, toEqual, toStrictEqual, toBeGreaterThan,
 * toBeGreaterThanOrEqual, toBeLessThan, toBeLessThanOrEqual.
 *
 * Skipped with warning (v0):
 *   - `expect(...).resolves.<matcher>(...)` (async / promises)
 *   - `expect(fn).toThrow(...)`
 *   - `expect(...).not.<matcher>(...)` for matchers other than toBe/toEqual
 *   - Anything outside the operand whitelist (identifier, numeric/string
 *     literal, single-arg call). Method chains, member access, multi-arg
 *     calls, randomness, filesystem ops -> skipped.
 *
 * Honest under-coverage beats polluting the lattice with unverifiable
 * atoms.
 */

import ts from "typescript";
import type { IrFormula, IrTerm, AtomicFormula } from "../../ir/formulas.js";
import { Int, String as StringSort } from "../../ir/sorts.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "vitest-tests";

export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  let seen = 0;

  const visit = (node: ts.Node): void => {
    const block = extractItBlock(node);
    if (block) {
      // Walk the body for expect(...) statements; each one is a candidate.
      const candidates = collectExpectStatements(block.body);
      let idx = 0;
      for (const c of candidates) {
        seen += 1;
        const memento = `${block.testName}::${idx}`;
        idx += 1;
        const r = liftExpect(c);
        if (r.kind === "ok") {
          decls.push({
            name: memento,
            outBinding: "out",
            sourcePath,
            adapter: ADAPTER,
            inv: r.formula,
          });
        } else {
          warnings.push({
            adapter: ADAPTER,
            sourcePath,
            itemName: memento,
            reason: r.reason,
          });
        }
      }
      return;
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  return { decls, seen, lifted: decls.length, warnings };
}

interface ItBlock {
  testName: string;
  body: ts.Node;
}

function extractItBlock(node: ts.Node): ItBlock | null {
  if (!ts.isCallExpression(node)) return null;
  const callee = node.expression;
  if (!ts.isIdentifier(callee)) return null;
  // Recognize bare `it` / `test` only. (it.skip / it.only are not
  // contracts of the SUT — they're scaffolding.)
  if (callee.text !== "it" && callee.text !== "test") return null;
  const [first, second] = node.arguments;
  if (!first) return null;
  if (!(ts.isStringLiteral(first) || ts.isNoSubstitutionTemplateLiteral(first))) return null;
  if (!second) return null;
  if (!ts.isArrowFunction(second) && !ts.isFunctionExpression(second)) return null;
  return { testName: first.text, body: second.body };
}

/**
 * Find every `expect(...)<matcher chain>` call expression in the test
 * body, in source order. We want the OUTERMOST call expression of the
 * chain — `expect(x).toBe(1)` is one candidate, not two.
 *
 * Strategy: walk the body; for each ExpressionStatement / call site
 * whose top-level call's callee is a property access whose root
 * expression is `expect(...)`, record the call. Skip nested calls
 * inside an already-recorded chain.
 */
function collectExpectStatements(body: ts.Node): ts.CallExpression[] {
  const out: ts.CallExpression[] = [];
  const visit = (n: ts.Node): void => {
    if (ts.isCallExpression(n) && isExpectChain(n)) {
      out.push(n);
      return; // don't descend into already-matched chain
    }
    ts.forEachChild(n, visit);
  };
  visit(body);
  return out;
}

/** Check whether a call expression's root is a call to `expect(...)`. */
function isExpectChain(call: ts.CallExpression): boolean {
  const callee = call.expression;
  if (!ts.isPropertyAccessExpression(callee)) return false;
  // Walk leftward through property accesses and call expressions until
  // we either hit an `expect(...)` call or something else.
  let cursor: ts.Expression = callee.expression;
  while (true) {
    if (ts.isCallExpression(cursor)) {
      const inner = cursor.expression;
      if (ts.isIdentifier(inner) && inner.text === "expect") return true;
      if (ts.isPropertyAccessExpression(inner)) {
        cursor = inner.expression;
        continue;
      }
      return false;
    }
    if (ts.isPropertyAccessExpression(cursor)) {
      cursor = cursor.expression;
      continue;
    }
    return false;
  }
}

type LiftResult =
  | { kind: "ok"; formula: IrFormula }
  | { kind: "skip"; reason: string };

/**
 * Decompose the chain into:
 *   - the actual expression passed to expect(...)
 *   - the modifier path (e.g., `not`, `resolves`, `rejects`)
 *   - the matcher name (e.g., `toBe`)
 *   - the matcher arguments
 */
interface ExpectChain {
  actual: ts.Expression;
  modifiers: string[];
  matcher: string;
  matcherArgs: ts.NodeArray<ts.Expression>;
}

function decomposeChain(call: ts.CallExpression): ExpectChain | null {
  const callee = call.expression;
  if (!ts.isPropertyAccessExpression(callee)) return null;
  const matcher = callee.name.text;
  const matcherArgs = call.arguments;
  // Walk leftward gathering modifier names (every property access between
  // the matcher and the expect(...) call).
  const modifiers: string[] = [];
  let cursor: ts.Expression = callee.expression;
  while (ts.isPropertyAccessExpression(cursor)) {
    modifiers.unshift(cursor.name.text);
    cursor = cursor.expression;
  }
  if (!ts.isCallExpression(cursor)) return null;
  if (!ts.isIdentifier(cursor.expression) || cursor.expression.text !== "expect") return null;
  if (cursor.arguments.length !== 1) return null;
  return {
    actual: cursor.arguments[0]!,
    modifiers,
    matcher,
    matcherArgs,
  };
}

const PROMISE_MODIFIERS = new Set(["resolves", "rejects"]);

function liftExpect(call: ts.CallExpression): LiftResult {
  const chain = decomposeChain(call);
  if (!chain) {
    return { kind: "skip", reason: "expect chain shape not recognized" };
  }
  // Async / promise modifiers — skip in v0.
  for (const m of chain.modifiers) {
    if (PROMISE_MODIFIERS.has(m)) {
      return {
        kind: "skip",
        reason: `expect(...).${m}.<matcher>() is async; not liftable in v0`,
      };
    }
  }
  // toThrow — skip in v0 (negative-fact, function-valued actual).
  if (chain.matcher === "toThrow" || chain.matcher === "toThrowError") {
    return { kind: "skip", reason: "expect(fn).toThrow(...) is not liftable in v0" };
  }
  // Negation handling: only fold `.not.toBe` / `.not.toEqual` to ne;
  // anything richer skips.
  let negated = false;
  for (const m of chain.modifiers) {
    if (m === "not") {
      negated = !negated;
      continue;
    }
    return { kind: "skip", reason: `unsupported modifier .${m} in v0` };
  }

  const actual = liftOperand(chain.actual);
  if (actual.kind === "skip") return actual;

  switch (chain.matcher) {
    case "toBe":
    case "toEqual":
    case "toStrictEqual": {
      if (chain.matcherArgs.length !== 1) {
        return { kind: "skip", reason: `${chain.matcher} expects exactly one argument` };
      }
      const expected = liftOperand(chain.matcherArgs[0]!);
      if (expected.kind === "skip") return expected;
      const op = negated ? "≠" : "=";
      return makeAtomic(op, actual.term, expected.term);
    }
    case "toBeGreaterThan":
      return cmpMatcher(">", negated, actual, chain);
    case "toBeGreaterThanOrEqual":
      return cmpMatcher("≥", negated, actual, chain);
    case "toBeLessThan":
      return cmpMatcher("<", negated, actual, chain);
    case "toBeLessThanOrEqual":
      return cmpMatcher("≤", negated, actual, chain);
    default:
      return {
        kind: "skip",
        reason: `matcher .${chain.matcher}() is not in the v0 whitelist`,
      };
  }
}

function cmpMatcher(
  op: string,
  negated: boolean,
  actual: { kind: "ok"; term: IrTerm },
  chain: ExpectChain,
): LiftResult {
  if (chain.matcherArgs.length !== 1) {
    return { kind: "skip", reason: `${chain.matcher} expects exactly one argument` };
  }
  if (negated) {
    return {
      kind: "skip",
      reason: `not.${chain.matcher} comparison negation is not in the v0 whitelist`,
    };
  }
  const expected = liftOperand(chain.matcherArgs[0]!);
  if (expected.kind === "skip") return expected;
  return makeAtomic(op, actual.term, expected.term);
}

function makeAtomic(op: string, a: IrTerm, b: IrTerm): LiftResult {
  const f: AtomicFormula = { kind: "atomic", name: op, args: [a, b] };
  return { kind: "ok", formula: f };
}

type OperandLift =
  | { kind: "ok"; term: IrTerm }
  | { kind: "skip"; reason: string };

/**
 * Operand whitelist (v0): identifier, numeric/string literal,
 * single-arg call, unary `-` literal. Everything else skips.
 */
function liftOperand(expr: ts.Expression): OperandLift {
  if (ts.isParenthesizedExpression(expr)) return liftOperand(expr.expression);
  if (ts.isIdentifier(expr)) {
    return { kind: "ok", term: { kind: "var", name: expr.text } };
  }
  if (ts.isNumericLiteral(expr)) {
    return { kind: "ok", term: { kind: "const", value: Number(expr.text), sort: Int } };
  }
  if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) {
    return { kind: "ok", term: { kind: "const", value: expr.text, sort: StringSort } };
  }
  if (
    ts.isPrefixUnaryExpression(expr) &&
    expr.operator === ts.SyntaxKind.MinusToken &&
    ts.isNumericLiteral(expr.operand)
  ) {
    return {
      kind: "ok",
      term: { kind: "const", value: -Number(expr.operand.text), sort: Int },
    };
  }
  if (ts.isCallExpression(expr) && expr.arguments.length === 1) {
    const callee = expr.expression;
    let name: string | null = null;
    if (ts.isIdentifier(callee)) name = callee.text;
    else if (
      ts.isPropertyAccessExpression(callee) &&
      ts.isIdentifier(callee.expression) &&
      ts.isIdentifier(callee.name)
    ) {
      // Allow `Module.fn(arg)` style ctor-call only when the receiver
      // is itself a bare identifier (no chains).
      name = `${callee.expression.text}.${callee.name.text}`;
    }
    if (!name) return { kind: "skip", reason: "call target shape unsupported" };
    const inner = liftOperand(expr.arguments[0]!);
    if (inner.kind === "skip") return inner;
    return { kind: "ok", term: { kind: "ctor", name, args: [inner.term] } };
  }
  if (
    ts.isPropertyAccessExpression(expr) &&
    ts.isIdentifier(expr.expression) &&
    ts.isIdentifier(expr.name) &&
    !expr.questionDotToken
  ) {
    // Allow `Foo.None` / `MyEnum.Variant` as a zero-arg ctor.
    return {
      kind: "ok",
      term: { kind: "ctor", name: `${expr.expression.text}.${expr.name.text}`, args: [] },
    };
  }
  if (ts.isToken(expr) && expr.kind === ts.SyntaxKind.TrueKeyword) {
    return { kind: "ok", term: { kind: "ctor", name: "True", args: [] } };
  }
  if (ts.isToken(expr) && expr.kind === ts.SyntaxKind.FalseKeyword) {
    return { kind: "ok", term: { kind: "ctor", name: "False", args: [] } };
  }
  if (ts.isToken(expr) && expr.kind === ts.SyntaxKind.NullKeyword) {
    return { kind: "ok", term: { kind: "ctor", name: "Null", args: [] } };
  }
  return {
    kind: "skip",
    reason: "operand shape not in v0 lift whitelist (no method chains, member chains, multi-arg calls, complex nesting)",
  };
}
