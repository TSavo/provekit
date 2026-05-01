/**
 * provekit-lift adapter: fast-check.
 *
 * Walks `fc.assert(fc.property(<arb>, <arb>..., (a, b, ...) => <pred>))`
 * style property declarations and lifts each universally-quantified
 * property to a contract memento.
 *
 * Strategic positioning: we do NOT replace fast-check. Developers keep
 * their existing properties. This adapter reads them and produces a
 * signed `<cid>.proof` so the property becomes content-addressed.
 *
 * SHAPE (v0):
 *
 *   it("name", () => {
 *     fc.assert(fc.property(fc.integer(), x => x + x === 2 * x));
 *   });
 *
 *   lifts to:
 *     contract "name" { inv: forall x: Int. (x + x) = (2 * x) }
 *
 * The body must be a binary comparison whose operands are simple terms
 * (variable, numeric/string literal, single-arg call). Anything else
 * skips with a warning, mirroring the Rust proptest adapter's strict
 * v0 grammar.
 */

import ts from "typescript";
import type { IrFormula, IrTerm, AtomicFormula } from "../../ir/formulas.js";
import { Int, Real, String as StringSort, Bool, Ref } from "../../ir/sorts.js";
import type { Sort } from "../../ir/formulas.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "fast-check";

export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  let seen = 0;

  const visit = (node: ts.Node): void => {
    const cand = extractCandidate(node);
    if (cand) {
      seen += 1;
      const r = liftCandidate(cand, sourcePath);
      if (r.kind === "ok") decls.push(r.decl);
      else
        warnings.push({
          adapter: ADAPTER,
          sourcePath,
          itemName: cand.name,
          reason: r.reason,
        });
      // Don't descend into a recognized candidate — the inner
      // fc.assert(fc.property(...)) would otherwise re-match as a
      // module-scope candidate and double-count.
      return;
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  return { decls, seen, lifted: decls.length, warnings };
}

interface Candidate {
  name: string;
  /** The fc.property(...) call. */
  propertyCall: ts.CallExpression;
}

/**
 * Recognized shapes:
 *
 *   it("name", () => { fc.assert(fc.property(...)) })
 *   it("name", () => fc.assert(fc.property(...)))
 *   test("name", () => ...)            (jest)
 *   test("name", () => ...)            (vitest's `test` alias)
 *
 * We only treat the call as a candidate when fc.property is reached
 * inside the body. A bare `fc.property(...)` call at module scope also
 * qualifies; we use the variable name or `unnamed_property_<line>`.
 */
function extractCandidate(node: ts.Node): Candidate | null {
  // it("name", () => { ... })  /  test("name", () => { ... })
  if (ts.isCallExpression(node)) {
    const callee = node.expression;
    if (ts.isIdentifier(callee) && (callee.text === "it" || callee.text === "test")) {
      const [first, second] = node.arguments;
      if (
        first !== undefined &&
        (ts.isStringLiteral(first) || ts.isNoSubstitutionTemplateLiteral(first)) &&
        second !== undefined &&
        (ts.isArrowFunction(second) || ts.isFunctionExpression(second))
      ) {
        const propertyCall = findFcPropertyInBody(second.body);
        if (propertyCall) {
          return { name: first.text, propertyCall };
        }
      }
    }
    // Bare module-scoped fc.assert(fc.property(...)).
    if (
      ts.isPropertyAccessExpression(callee) &&
      ts.isIdentifier(callee.expression) &&
      callee.expression.text === "fc" &&
      callee.name.text === "assert"
    ) {
      if (node.arguments.length >= 1) {
        const inner = node.arguments[0]!;
        if (ts.isCallExpression(inner) && isFcProperty(inner)) {
          // No nice name; synthesize from line number.
          const line = node.getSourceFile().getLineAndCharacterOfPosition(node.pos).line + 1;
          return { name: `fc_property_at_line_${line}`, propertyCall: inner };
        }
      }
    }
    // Top-level assignment: `const myProp = fc.property(...)`.
  }

  return null;
}

function findFcPropertyInBody(body: ts.Node): ts.CallExpression | null {
  let found: ts.CallExpression | null = null;
  const visit = (n: ts.Node): void => {
    if (found) return;
    if (ts.isCallExpression(n) && isFcProperty(n)) {
      found = n;
      return;
    }
    ts.forEachChild(n, visit);
  };
  visit(body);
  return found;
}

function isFcProperty(call: ts.CallExpression): boolean {
  const e = call.expression;
  return (
    ts.isPropertyAccessExpression(e) &&
    ts.isIdentifier(e.expression) &&
    e.expression.text === "fc" &&
    e.name.text === "property"
  );
}

type LiftResult =
  | { kind: "ok"; decl: ContractDecl }
  | { kind: "skip"; reason: string };

function liftCandidate(c: Candidate, sourcePath: string): LiftResult {
  // fc.property(<arb1>, <arb2>, ..., (binders) => <body>).
  const args = c.propertyCall.arguments;
  if (args.length < 2) {
    return { kind: "skip", reason: "fc.property needs at least one arbitrary and a predicate" };
  }
  const predicate = args[args.length - 1]!;
  if (!ts.isArrowFunction(predicate) && !ts.isFunctionExpression(predicate)) {
    return { kind: "skip", reason: "predicate is not an arrow or function expression" };
  }
  const arbs = args.slice(0, -1);
  if (arbs.length === 0) {
    return { kind: "skip", reason: "fc.property had no arbitraries" };
  }
  const sorts: Sort[] = [];
  for (const a of arbs) {
    const s = sortFromArb(a);
    if (!s) return { kind: "skip", reason: "arbitrary kind not supported in v0" };
    sorts.push(s);
  }

  // Binder names.
  const binderNames: string[] = [];
  for (const p of predicate.parameters) {
    if (!ts.isIdentifier(p.name)) return { kind: "skip", reason: "non-ident parameter name" };
    binderNames.push(p.name.text);
  }
  if (binderNames.length !== sorts.length) {
    return {
      kind: "skip",
      reason: `arbitrary/binder arity mismatch (${sorts.length} arbs, ${binderNames.length} binders)`,
    };
  }

  // Predicate body — require a single `return <expr>` or expression body.
  const bodyExpr = extractPredicateBodyExpression(predicate);
  if (!bodyExpr) {
    return { kind: "skip", reason: "predicate body must be a single expression" };
  }

  const formula = liftPredicate(bodyExpr);
  if (formula.kind === "skip") return { kind: "skip", reason: formula.reason };

  // Wrap in nested foralls in declaration order.
  let body: IrFormula = formula.formula;
  for (let i = sorts.length - 1; i >= 0; i--) {
    body = {
      kind: "forall",
      name: binderNames[i]!,
      sort: sorts[i]!,
      body,
    };
  }
  return {
    kind: "ok",
    decl: { name: c.name, outBinding: "out", sourcePath, adapter: ADAPTER, inv: body },
  };
}

function extractPredicateBodyExpression(
  fn: ts.ArrowFunction | ts.FunctionExpression,
): ts.Expression | null {
  if (ts.isBlock(fn.body)) {
    const stmts = fn.body.statements;
    if (stmts.length === 1 && ts.isReturnStatement(stmts[0]!) && stmts[0]!.expression) {
      return stmts[0]!.expression!;
    }
    return null;
  }
  return fn.body;
}

/** Turn an arbitrary expression like `fc.integer()` / `fc.string()` into a Sort. */
function sortFromArb(node: ts.Expression): Sort | null {
  if (!ts.isCallExpression(node)) return null;
  const e = node.expression;
  if (!ts.isPropertyAccessExpression(e)) return null;
  if (!ts.isIdentifier(e.expression) || e.expression.text !== "fc") return null;
  switch (e.name.text) {
    case "integer":
    case "nat":
    case "bigInt":
      return Int;
    case "float":
    case "double":
      return Real;
    case "string":
    case "asciiString":
    case "unicodeString":
    case "hexaString":
    case "base64String":
    case "uuid":
      return StringSort;
    case "boolean":
      return Bool;
    default:
      return null;
  }
}

type PredicateLift =
  | { kind: "ok"; formula: IrFormula }
  | { kind: "skip"; reason: string };

/**
 * Lift the predicate body. v0 grammar mirrors the Rust proptest adapter:
 *   - top-level binary comparison (===, !==, <, <=, >, >=, ==, !=)
 *   - operands: identifier, number/string literal, single-arg call
 *
 * Anything else -> skip.
 */
function liftPredicate(expr: ts.Expression): PredicateLift {
  if (ts.isParenthesizedExpression(expr)) return liftPredicate(expr.expression);
  if (ts.isBinaryExpression(expr)) {
    const op = compareOp(expr.operatorToken.kind);
    if (!op) return { kind: "skip", reason: "non-comparison binary op at top level" };
    const lhs = liftOperand(expr.left);
    const rhs = liftOperand(expr.right);
    if (lhs.kind === "skip") return lhs;
    if (rhs.kind === "skip") return rhs;
    const a: AtomicFormula = { kind: "atomic", name: op, args: [lhs.term, rhs.term] };
    return { kind: "ok", formula: a };
  }
  return { kind: "skip", reason: "predicate body must be a top-level comparison in v0" };
}

function compareOp(kind: ts.SyntaxKind): string | null {
  switch (kind) {
    case ts.SyntaxKind.EqualsEqualsEqualsToken:
    case ts.SyntaxKind.EqualsEqualsToken:
      return "=";
    case ts.SyntaxKind.ExclamationEqualsEqualsToken:
    case ts.SyntaxKind.ExclamationEqualsToken:
      return "≠";
    case ts.SyntaxKind.LessThanToken:
      return "<";
    case ts.SyntaxKind.LessThanEqualsToken:
      return "≤";
    case ts.SyntaxKind.GreaterThanToken:
      return ">";
    case ts.SyntaxKind.GreaterThanEqualsToken:
      return "≥";
    default:
      return null;
  }
}

type OperandLift =
  | { kind: "ok"; term: IrTerm }
  | { kind: "skip"; reason: string };

function liftOperand(expr: ts.Expression): OperandLift {
  if (ts.isParenthesizedExpression(expr)) return liftOperand(expr.expression);
  if (ts.isIdentifier(expr)) {
    return { kind: "ok", term: { kind: "var", name: expr.text } };
  }
  if (ts.isNumericLiteral(expr)) {
    return {
      kind: "ok",
      term: { kind: "const", value: Number(expr.text), sort: Int },
    };
  }
  if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) {
    return { kind: "ok", term: { kind: "const", value: expr.text, sort: StringSort } };
  }
  // Arithmetic binary on simple terms — encode as Ctor (kit-extension).
  if (ts.isBinaryExpression(expr)) {
    const arithName = arithOp(expr.operatorToken.kind);
    if (!arithName) {
      return { kind: "skip", reason: "operand has non-arithmetic binary op" };
    }
    const l = liftOperand(expr.left);
    const r = liftOperand(expr.right);
    if (l.kind === "skip") return l;
    if (r.kind === "skip") return r;
    return {
      kind: "ok",
      term: { kind: "ctor", name: arithName, args: [l.term, r.term] },
    };
  }
  // Single-argument call — treat as a Ctor in the IR.
  if (ts.isCallExpression(expr) && expr.arguments.length <= 2) {
    const callee = expr.expression;
    let name: string | null = null;
    if (ts.isIdentifier(callee)) name = callee.text;
    else if (
      ts.isPropertyAccessExpression(callee) &&
      ts.isIdentifier(callee.expression) &&
      ts.isIdentifier(callee.name)
    ) {
      name = `${callee.expression.text}.${callee.name.text}`;
    }
    if (!name) return { kind: "skip", reason: "call target shape unsupported" };
    const argTerms: IrTerm[] = [];
    for (const a of expr.arguments) {
      const r = liftOperand(a);
      if (r.kind === "skip") return r;
      argTerms.push(r.term);
    }
    return { kind: "ok", term: { kind: "ctor", name, args: argTerms } };
  }
  return { kind: "skip", reason: "operand shape unsupported in v0" };
}

function arithOp(kind: ts.SyntaxKind): string | null {
  switch (kind) {
    case ts.SyntaxKind.PlusToken:
      return "add";
    case ts.SyntaxKind.MinusToken:
      return "sub";
    case ts.SyntaxKind.AsteriskToken:
      return "mul";
    case ts.SyntaxKind.SlashToken:
      return "div";
    case ts.SyntaxKind.PercentToken:
      return "mod";
    default:
      return null;
  }
}

// Currently unused but exported for parity with the zod adapter API.
export const __unused_sorts__: Sort = Ref;
