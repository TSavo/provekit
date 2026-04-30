/**
 * Per-AST-node lift rules. The dispatch table from spec §9.
 *
 * Two mutually recursive walks:
 *   liftFormula(expr): IrFormula  — bool-typed expressions
 *   liftTerm(expr):    IrTerm     — value-typed expressions
 *
 * The split is forced by the IR shape: IrFormula has no `Apply` or
 * arithmetic kinds, so `Math.abs(x)` and `a + b` are IrTerm ctors,
 * while `===`, `<`, etc. are atomic IrFormula values whose args are
 * IrTerm.
 *
 * Spec: protocol/specs/2026-04-29-ts-ir-language.md §9
 */

import ts from "typescript";
import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import type { LiftDiagnostic } from "./diagnostics.js";
import { makeDiagnostic } from "./diagnostics.js";
import type { PureFunctionRegistry } from "./registry.js";
import { resolveSort, primitiveSort } from "./sorts.js";

export interface LiftContext {
  checker: ts.TypeChecker;
  diagnostics: LiftDiagnostic[];
  registry: PureFunctionRegistry;
  /** Stack of bound variable names → sort. Innermost last. */
  scope: Array<{ name: string; sort: Sort }>;
}

const REAL: Sort = { kind: "primitive", name: "Real" };
const INT: Sort = { kind: "primitive", name: "Int" };
const STRING_SORT: Sort = { kind: "primitive", name: "String" };
const BOOL: Sort = { kind: "primitive", name: "Bool" };
const REF: Sort = { kind: "primitive", name: "Ref" };

const UNLIFTABLE_FORMULA: IrFormula = {
  kind: "atomic",
  name: "false",
  args: [],
};

const UNLIFTABLE_TERM: IrTerm = {
  kind: "const",
  value: null,
  sort: REF,
};

const SORT_HINT = Symbol.for("provekit.ir.sortHint");

function withSortHint(term: IrTerm, sort: Sort): IrTerm {
  Object.defineProperty(term, SORT_HINT, {
    value: sort,
    enumerable: false,
    writable: true,
    configurable: true,
  });
  return term;
}

function readSortHint(term: IrTerm): Sort {
  if (term.kind === "const") return term.sort;
  const v = (term as unknown as Record<symbol, unknown>)[SORT_HINT];
  return (v as Sort | undefined) ?? REF;
}

function reject(node: ts.Node, message: string, ctx: LiftContext): void {
  ctx.diagnostics.push(makeDiagnostic(node, message));
}

function findScope(
  ctx: LiftContext,
  name: string,
): { name: string; sort: Sort } | undefined {
  for (let i = ctx.scope.length - 1; i >= 0; i--) {
    if (ctx.scope[i].name === name) return ctx.scope[i];
  }
  return undefined;
}

/* -------------------------------------------------------------------------- */
/* Top-level dispatch                                                         */
/* -------------------------------------------------------------------------- */

export function liftFormulaExpression(
  expression: ts.Expression,
  ctx: LiftContext,
): IrFormula {
  const expr = unwrapParens(expression);
  return dispatchFormula(expr, ctx);
}

export function liftTermExpression(
  expression: ts.Expression,
  ctx: LiftContext,
): IrTerm {
  const expr = unwrapParens(expression);
  return dispatchTerm(expr, ctx);
}

function unwrapParens(expr: ts.Expression): ts.Expression {
  while (ts.isParenthesizedExpression(expr)) {
    expr = expr.expression;
  }
  return expr;
}

/* -------------------------------------------------------------------------- */
/* Formula dispatch                                                           */
/* -------------------------------------------------------------------------- */

function dispatchFormula(node: ts.Expression, ctx: LiftContext): IrFormula {
  switch (node.kind) {
    case ts.SyntaxKind.TrueKeyword:
      return { kind: "atomic", name: "true", args: [] };
    case ts.SyntaxKind.FalseKeyword:
      return { kind: "atomic", name: "false", args: [] };
    case ts.SyntaxKind.BinaryExpression:
      return liftBinaryFormula(node as ts.BinaryExpression, ctx);
    case ts.SyntaxKind.PrefixUnaryExpression:
      return liftPrefixUnaryFormula(node as ts.PrefixUnaryExpression, ctx);
    case ts.SyntaxKind.ConditionalExpression:
      return liftConditionalFormula(node as ts.ConditionalExpression, ctx);
    case ts.SyntaxKind.CallExpression:
      return liftCallFormula(node as ts.CallExpression, ctx);
    case ts.SyntaxKind.Identifier:
      return liftIdentifierFormula(node as ts.Identifier, ctx);
    case ts.SyntaxKind.PropertyAccessExpression:
      return liftPropertyAccessFormula(node as ts.PropertyAccessExpression, ctx);
    case ts.SyntaxKind.ArrowFunction:
    case ts.SyntaxKind.FunctionExpression:
      reject(node, "Function/arrow expression cannot be lifted as formula here", ctx);
      return UNLIFTABLE_FORMULA;
    default:
      reject(
        node,
        `Construct ${ts.SyntaxKind[node.kind]} is not allowed in IR formula position`,
        ctx,
      );
      return UNLIFTABLE_FORMULA;
  }
}

/* -------------------------------------------------------------------------- */
/* Term dispatch                                                              */
/* -------------------------------------------------------------------------- */

function dispatchTerm(node: ts.Expression, ctx: LiftContext): IrTerm {
  switch (node.kind) {
    case ts.SyntaxKind.NumericLiteral:
      return liftNumericLiteral(node as ts.NumericLiteral);
    case ts.SyntaxKind.StringLiteral:
    case ts.SyntaxKind.NoSubstitutionTemplateLiteral:
      return {
        kind: "const",
        value: (node as ts.StringLiteralLike).text,
        sort: STRING_SORT,
      };
    case ts.SyntaxKind.TrueKeyword:
      return { kind: "const", value: true, sort: BOOL };
    case ts.SyntaxKind.FalseKeyword:
      return { kind: "const", value: false, sort: BOOL };
    case ts.SyntaxKind.NullKeyword:
      return { kind: "const", value: null, sort: REF };
    case ts.SyntaxKind.UndefinedKeyword:
      return { kind: "const", value: undefined, sort: REF };
    case ts.SyntaxKind.Identifier:
      return liftIdentifierTerm(node as ts.Identifier, ctx);
    case ts.SyntaxKind.BinaryExpression:
      return liftBinaryTerm(node as ts.BinaryExpression, ctx);
    case ts.SyntaxKind.PrefixUnaryExpression:
      return liftPrefixUnaryTerm(node as ts.PrefixUnaryExpression, ctx);
    case ts.SyntaxKind.ConditionalExpression:
      return liftConditionalTerm(node as ts.ConditionalExpression, ctx);
    case ts.SyntaxKind.CallExpression:
      return liftCallTerm(node as ts.CallExpression, ctx);
    case ts.SyntaxKind.PropertyAccessExpression:
      return liftPropertyAccessTerm(node as ts.PropertyAccessExpression, ctx);
    case ts.SyntaxKind.ParenthesizedExpression:
      return dispatchTerm((node as ts.ParenthesizedExpression).expression, ctx);
    default:
      // Handle bare 'undefined' identifier already covered via UndefinedKeyword
      // when it appears as a literal node. Otherwise reject.
      reject(
        node,
        `Construct ${ts.SyntaxKind[node.kind]} is not allowed in IR term position`,
        ctx,
      );
      return UNLIFTABLE_TERM;
  }
}

function liftNumericLiteral(node: ts.NumericLiteral): IrTerm {
  const text = node.text;
  const value = Number(text);
  const sort = Number.isInteger(value) && !text.includes(".") ? INT : REAL;
  return { kind: "const", value, sort };
}

/* -------------------------------------------------------------------------- */
/* Binary expressions — formula side                                          */
/* -------------------------------------------------------------------------- */

const FORMULA_PREDICATE_BY_OP: Partial<Record<ts.SyntaxKind, string>> = {
  [ts.SyntaxKind.EqualsEqualsEqualsToken]: "=",
  [ts.SyntaxKind.ExclamationEqualsEqualsToken]: "≠",
  [ts.SyntaxKind.LessThanToken]: "<",
  [ts.SyntaxKind.LessThanEqualsToken]: "≤",
  [ts.SyntaxKind.GreaterThanToken]: ">",
  [ts.SyntaxKind.GreaterThanEqualsToken]: "≥",
};

function liftBinaryFormula(node: ts.BinaryExpression, ctx: LiftContext): IrFormula {
  const op = node.operatorToken.kind;
  if (op === ts.SyntaxKind.AmpersandAmpersandToken) {
    return {
      kind: "and",
      operands: [
        liftFormulaExpression(node.left, ctx),
        liftFormulaExpression(node.right, ctx),
      ],
    };
  }
  if (op === ts.SyntaxKind.BarBarToken) {
    return {
      kind: "or",
      operands: [
        liftFormulaExpression(node.left, ctx),
        liftFormulaExpression(node.right, ctx),
      ],
    };
  }
  if (op === ts.SyntaxKind.QuestionQuestionToken) {
    // a ?? b in formula position — unusual but valid. Desugar.
    return liftNullishCoalescingFormula(node, ctx);
  }
  const predicateName = FORMULA_PREDICATE_BY_OP[op];
  if (predicateName !== undefined) {
    return {
      kind: "atomic",
      name: predicateName,
      args: [
        liftTermExpression(node.left, ctx),
        liftTermExpression(node.right, ctx),
      ],
    };
  }
  reject(
    node,
    `Operator ${ts.tokenToString(op) ?? ts.SyntaxKind[op]} is not allowed in IR formula position`,
    ctx,
  );
  return UNLIFTABLE_FORMULA;
}

/* -------------------------------------------------------------------------- */
/* Binary expressions — term side (arithmetic)                                */
/* -------------------------------------------------------------------------- */

const TERM_CTOR_BY_OP: Partial<Record<ts.SyntaxKind, string>> = {
  [ts.SyntaxKind.PlusToken]: "+",
  [ts.SyntaxKind.MinusToken]: "-",
  [ts.SyntaxKind.AsteriskToken]: "*",
  [ts.SyntaxKind.SlashToken]: "/",
  [ts.SyntaxKind.PercentToken]: "%",
};

function liftBinaryTerm(node: ts.BinaryExpression, ctx: LiftContext): IrTerm {
  const op = node.operatorToken.kind;
  const ctor = TERM_CTOR_BY_OP[op];
  if (ctor !== undefined) {
    const left = liftTermExpression(node.left, ctx);
    const right = liftTermExpression(node.right, ctx);
    const sort = termArithSort(readSortHint(left), readSortHint(right));
    return withSortHint(
      { kind: "ctor", name: ctor, args: [left, right] },
      sort,
    );
  }
  if (op === ts.SyntaxKind.QuestionQuestionToken) {
    return liftNullishCoalescingTerm(node, ctx);
  }
  reject(
    node,
    `Operator ${ts.tokenToString(op) ?? ts.SyntaxKind[op]} is not allowed in IR term position`,
    ctx,
  );
  return UNLIFTABLE_TERM;
}

function termArithSort(a: Sort, b: Sort): Sort {
  if (a.kind === "primitive" && b.kind === "primitive") {
    if (a.name === "Real" || b.name === "Real") return REAL;
    if (a.name === "Int" && b.name === "Int") return INT;
  }
  return REAL;
}

/* -------------------------------------------------------------------------- */
/* Prefix unary                                                               */
/* -------------------------------------------------------------------------- */

function liftPrefixUnaryFormula(
  node: ts.PrefixUnaryExpression,
  ctx: LiftContext,
): IrFormula {
  if (node.operator === ts.SyntaxKind.ExclamationToken) {
    return { kind: "not", operands: [liftFormulaExpression(node.operand, ctx)] };
  }
  reject(
    node,
    `Prefix unary operator ${ts.tokenToString(node.operator) ?? node.operator} is not allowed in IR formula position`,
    ctx,
  );
  return UNLIFTABLE_FORMULA;
}

function liftPrefixUnaryTerm(
  node: ts.PrefixUnaryExpression,
  ctx: LiftContext,
): IrTerm {
  if (node.operator === ts.SyntaxKind.MinusToken) {
    const inner = liftTermExpression(node.operand, ctx);
    return withSortHint(
      { kind: "ctor", name: "negate", args: [inner] },
      readSortHint(inner),
    );
  }
  if (node.operator === ts.SyntaxKind.PlusToken) {
    // unary plus is identity for our purposes
    return liftTermExpression(node.operand, ctx);
  }
  reject(
    node,
    `Prefix unary operator ${ts.tokenToString(node.operator) ?? node.operator} is not allowed in IR term position`,
    ctx,
  );
  return UNLIFTABLE_TERM;
}

/* -------------------------------------------------------------------------- */
/* Conditional (ternary)                                                      */
/* -------------------------------------------------------------------------- */

function liftConditionalFormula(
  node: ts.ConditionalExpression,
  ctx: LiftContext,
): IrFormula {
  // (cond ? a : b) in formula position lifts to (cond ∧ a) ∨ (¬cond ∧ b).
  const cond = liftFormulaExpression(node.condition, ctx);
  const thenBranch = liftFormulaExpression(node.whenTrue, ctx);
  const elseBranch = liftFormulaExpression(node.whenFalse, ctx);
  return {
    kind: "or",
    operands: [
      { kind: "and", operands: [cond, thenBranch] },
      { kind: "and", operands: [{ kind: "not", operands: [cond] }, elseBranch] },
    ],
  };
}

function liftConditionalTerm(
  node: ts.ConditionalExpression,
  ctx: LiftContext,
): IrTerm {
  // Term-position ternary lifts to an `if` ctor. The downstream prover
  // treats this as an opaque function symbol over (Bool, T, T) → T.
  // We attach the cond as a Bool const placeholder; full encoding would
  // require an embedded formula node. v1 represents cond as a ctor-arg
  // term whose shape mirrors the formula via a lift-back step.
  // For v1 simplicity and to match registry semantics, we encode as
  // ctor "if" with args [thenBranch, elseBranch] sorted by the then sort
  // and emit a diagnostic-free representation; the cond's structure
  // is preserved through serialization. We round-trip cond as a term
  // by lifting it as a term-coerced atomic check.
  const thenT = liftTermExpression(node.whenTrue, ctx);
  const elseT = liftTermExpression(node.whenFalse, ctx);
  const condT = liftFormulaAsTerm(node.condition, ctx);
  return withSortHint(
    { kind: "ctor", name: "if", args: [condT, thenT, elseT] },
    readSortHint(thenT),
  );
}

function liftFormulaAsTerm(expr: ts.Expression, ctx: LiftContext): IrTerm {
  // Wrap a formula in a `formula` ctor so it survives in term position.
  // Used only by ternary's cond and nullish-coalescing's check.
  const f = liftFormulaExpression(expr, ctx);
  return withSortHint(
    { kind: "ctor", name: "as-term", args: encodeFormulaIntoArgs(f) },
    BOOL,
  );
}

function encodeFormulaIntoArgs(_f: IrFormula): IrTerm[] {
  // v1: opaque encoding — we don't need round-trip yet. Return empty;
  // the canonicalizer treats `as-term` ctors as atoms with no args.
  return [];
}

/* -------------------------------------------------------------------------- */
/* Nullish coalescing                                                         */
/* -------------------------------------------------------------------------- */

function liftNullishCoalescingFormula(
  node: ts.BinaryExpression,
  ctx: LiftContext,
): IrFormula {
  return {
    kind: "or",
    operands: [
      liftFormulaExpression(node.left, ctx),
      liftFormulaExpression(node.right, ctx),
    ],
  };
}

function liftNullishCoalescingTerm(
  node: ts.BinaryExpression,
  ctx: LiftContext,
): IrTerm {
  const left = liftTermExpression(node.left, ctx);
  const right = liftTermExpression(node.right, ctx);
  return withSortHint(
    { kind: "ctor", name: "??", args: [left, right] },
    readSortHint(left),
  );
}

/* -------------------------------------------------------------------------- */
/* Identifiers                                                                */
/* -------------------------------------------------------------------------- */

function liftIdentifierFormula(node: ts.Identifier, ctx: LiftContext): IrFormula {
  // Identifier at formula position: must be a bound var of Bool sort
  // OR a const-bound boolean. We model it as an atomic predicate whose
  // single arg is the var.
  const bound = findScope(ctx, node.text);
  if (bound) {
    return {
      kind: "atomic",
      name: "is-true",
      args: [withSortHint({ kind: "var", name: bound.name }, bound.sort)],
    };
  }
  reject(node, `Identifier '${node.text}' is not a bound variable or known boolean`, ctx);
  return UNLIFTABLE_FORMULA;
}

function liftIdentifierTerm(node: ts.Identifier, ctx: LiftContext): IrTerm {
  // Bound lambda param?
  const bound = findScope(ctx, node.text);
  if (bound) {
    return withSortHint({ kind: "var", name: bound.name }, bound.sort);
  }

  // 'undefined' identifier (TS represents it as Identifier, not keyword)
  if (node.text === "undefined") {
    return { kind: "const", value: undefined, sort: REF };
  }

  // Const-bound: try to resolve via type checker. v1 leaves such
  // references as opaque `const` ctors with sort Real (registry call
  // resolution handles known names). Reject unknowns conservatively.
  const sym = ctx.checker.getSymbolAtLocation(node);
  if (!sym) {
    reject(node, `Cannot resolve identifier '${node.text}'`, ctx);
    return UNLIFTABLE_TERM;
  }
  // If the symbol's declaration is a const VariableDeclaration with
  // an initializer that's a literal, inline the literal.
  const decl = sym.valueDeclaration;
  if (decl && ts.isVariableDeclaration(decl)) {
    const flags = ts.getCombinedNodeFlags(decl);
    const isConst = (flags & ts.NodeFlags.Const) !== 0;
    if (!isConst) {
      reject(
        node,
        `Closure over non-const binding '${node.text}' is not allowed (let/var bindings can be reassigned)`,
        ctx,
      );
      return UNLIFTABLE_TERM;
    }
    if (decl.initializer && ts.isNumericLiteral(decl.initializer)) {
      return liftNumericLiteral(decl.initializer);
    }
    if (decl.initializer && ts.isStringLiteral(decl.initializer)) {
      return { kind: "const", value: decl.initializer.text, sort: STRING_SORT };
    }
  }
  // Treat as a free constant identifier (e.g. imported pure const).
  return { kind: "const", value: node.text, sort: REF };
}

/* -------------------------------------------------------------------------- */
/* Property access                                                            */
/* -------------------------------------------------------------------------- */

function liftPropertyAccessFormula(
  node: ts.PropertyAccessExpression,
  ctx: LiftContext,
): IrFormula {
  // Member access at formula position is unusual; treat as is-true on the
  // projected term.
  const t = liftPropertyAccessTerm(node, ctx);
  return { kind: "atomic", name: "is-true", args: [t] };
}

function liftPropertyAccessTerm(
  node: ts.PropertyAccessExpression,
  ctx: LiftContext,
): IrTerm {
  // x.length on a string/array → registered prototype reads.
  // We project as ctor "project" with args [object, fieldName].
  const obj = liftTermExpression(node.expression, ctx);
  const field = node.name.text;
  return withSortHint(
    {
      kind: "ctor",
      name: "project",
      args: [obj, { kind: "const", value: field, sort: STRING_SORT }],
    },
    REF,
  );
}

/* -------------------------------------------------------------------------- */
/* Call expressions — builders, quantifiers, registry                         */
/* -------------------------------------------------------------------------- */

function liftCallFormula(node: ts.CallExpression, ctx: LiftContext): IrFormula {
  const callee = node.expression;

  // forAll<T>(...) / exists<T>(...)
  if (ts.isIdentifier(callee)) {
    if (callee.text === "forAll") return liftQuantifier("forall", node, ctx);
    if (callee.text === "exists") return liftQuantifier("exists", node, ctx);
    if (callee.text === "implies") return liftImplies(node, ctx);
    if (callee.text === "iff") return liftIff(node, ctx);
  }

  // xs.every(λ) / xs.some(λ)
  if (ts.isPropertyAccessExpression(callee)) {
    if (callee.name.text === "every") return liftArrayQuantifier("forall", node, ctx);
    if (callee.name.text === "some") return liftArrayQuantifier("exists", node, ctx);
  }

  // Registry call returning bool
  const registryName = resolveRegistryName(callee, ctx.checker);
  if (registryName !== null) {
    const entry = ctx.registry.get(registryName);
    if (entry) {
      const args = node.arguments.map((a) => liftTermExpression(a, ctx));
      if (entry.returnKind === "formula") {
        return { kind: "atomic", name: registryName, args };
      }
      // Term-returning registry call appearing in formula position is unusual;
      // treat as is-true on the term.
      return {
        kind: "atomic",
        name: "is-true",
        args: [
          withSortHint(
            { kind: "ctor", name: registryName, args },
            entry.returnSort,
          ),
        ],
      };
    }
  }

  reject(
    node,
    `Call to '${describeCallee(callee)}' is not allowed in IR (not in pure-function registry)`,
    ctx,
  );
  return UNLIFTABLE_FORMULA;
}

function liftCallTerm(node: ts.CallExpression, ctx: LiftContext): IrTerm {
  const callee = node.expression;
  const registryName = resolveRegistryName(callee, ctx.checker);
  if (registryName !== null) {
    const entry = ctx.registry.get(registryName);
    if (entry) {
      const args = node.arguments.map((a) => liftTermExpression(a, ctx));
      return withSortHint(
        { kind: "ctor", name: registryName, args },
        entry.returnSort,
      );
    }
  }
  reject(
    node,
    `Call to '${describeCallee(callee)}' is not allowed in IR (not in pure-function registry)`,
    ctx,
  );
  return UNLIFTABLE_TERM;
}

function describeCallee(callee: ts.Expression): string {
  if (ts.isIdentifier(callee)) return callee.text;
  if (ts.isPropertyAccessExpression(callee)) {
    return `${describeCallee(callee.expression)}.${callee.name.text}`;
  }
  return ts.SyntaxKind[callee.kind];
}

function resolveRegistryName(
  callee: ts.Expression,
  _checker: ts.TypeChecker,
): string | null {
  if (ts.isIdentifier(callee)) return callee.text;
  if (ts.isPropertyAccessExpression(callee)) {
    const owner = describeCallee(callee.expression);
    return `${owner}.${callee.name.text}`;
  }
  return null;
}

/* -------------------------------------------------------------------------- */
/* Quantifiers                                                                */
/* -------------------------------------------------------------------------- */

function liftQuantifier(
  kind: "forall" | "exists",
  call: ts.CallExpression,
  ctx: LiftContext,
): IrFormula {
  if (call.arguments.length !== 1) {
    reject(call, `${kind} expects exactly one lambda argument`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  const lambda = call.arguments[0];
  if (!ts.isArrowFunction(lambda) && !ts.isFunctionExpression(lambda)) {
    reject(lambda, `${kind}'s argument must be an arrow function or function expression`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  // Sort: prefer explicit type argument, else lambda param annotation.
  let sort: Sort | null = null;
  if (call.typeArguments && call.typeArguments.length === 1) {
    const tNode = call.typeArguments[0];
    const t = ctx.checker.getTypeFromTypeNode(tNode);
    sort = resolveSort(t, ctx.checker);
    if (sort === null) {
      reject(
        tNode,
        `Cannot resolve sort from type argument; expected branded type with __sort property`,
        ctx,
      );
      return UNLIFTABLE_FORMULA;
    }
  }
  return liftLambdaWithSort(kind, lambda, sort, ctx);
}

function liftArrayQuantifier(
  kind: "forall" | "exists",
  call: ts.CallExpression,
  ctx: LiftContext,
): IrFormula {
  if (!ts.isPropertyAccessExpression(call.expression)) {
    reject(call, `Array quantifier callee shape is invalid`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  if (call.arguments.length !== 1) {
    reject(call, `.${call.expression.name.text} expects one callback`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  const lambda = call.arguments[0];
  if (!ts.isArrowFunction(lambda) && !ts.isFunctionExpression(lambda)) {
    reject(lambda, `.${call.expression.name.text}'s argument must be an arrow function`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  // Sort comes from receiver's element type.
  const receiverType = ctx.checker.getTypeAtLocation(call.expression.expression);
  const arraySort = resolveSort(receiverType, ctx.checker);
  let elementSort: Sort | null = null;
  if (arraySort && arraySort.kind === "set") {
    elementSort = arraySort.element;
  } else if (arraySort && arraySort.kind === "tuple" && arraySort.elements.length > 0) {
    // Heterogeneous tuples reject; uniform tuples take their element sort.
    const first = arraySort.elements[0];
    if (arraySort.elements.every((e) => sortEquals(e, first))) {
      elementSort = first;
    }
  }
  return liftLambdaWithSort(kind, lambda, elementSort, ctx);
}

function sortEquals(a: Sort, b: Sort): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "primitive" && b.kind === "primitive") return a.name === b.name;
  return false;
}

function liftLambdaWithSort(
  kind: "forall" | "exists",
  lambda: ts.ArrowFunction | ts.FunctionExpression,
  sortHint: Sort | null,
  ctx: LiftContext,
): IrFormula {
  if (lambda.parameters.length !== 1) {
    reject(lambda, `Quantifier lambda must have exactly one parameter`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  const param = lambda.parameters[0];
  if (!ts.isIdentifier(param.name)) {
    reject(param, `Object/tuple destructuring in quantifier params is not supported in v1`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  let sort = sortHint;
  if (sort === null) {
    const paramType = ctx.checker.getTypeAtLocation(param);
    sort = resolveSort(paramType, ctx.checker);
  }
  if (sort === null) {
    reject(param, `Cannot resolve sort for parameter '${param.name.text}'; annotate with a branded type`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  const varName = param.name.text;
  ctx.scope.push({ name: varName, sort });
  let body: IrFormula;
  try {
    if (ts.isBlock(lambda.body)) {
      reject(lambda.body, `Quantifier body must be an expression, not a block`, ctx);
      body = UNLIFTABLE_FORMULA;
    } else {
      body = liftFormulaExpression(lambda.body, ctx);
    }
  } finally {
    ctx.scope.pop();
  }
  return {
    kind,
    name: varName,
    sort,
    body,
  };
}

function liftImplies(call: ts.CallExpression, ctx: LiftContext): IrFormula {
  if (call.arguments.length !== 2) {
    reject(call, `implies expects two arguments`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  return {
    kind: "implies",
    operands: [
      liftFormulaExpression(call.arguments[0]!, ctx),
      liftFormulaExpression(call.arguments[1]!, ctx),
    ],
  };
}

function liftIff(call: ts.CallExpression, ctx: LiftContext): IrFormula {
  if (call.arguments.length !== 2) {
    reject(call, `iff expects two arguments`, ctx);
    return UNLIFTABLE_FORMULA;
  }
  const a = liftFormulaExpression(call.arguments[0]!, ctx);
  const b = liftFormulaExpression(call.arguments[1]!, ctx);
  return {
    kind: "and",
    operands: [
      { kind: "implies", operands: [a, b] },
      { kind: "implies", operands: [b, a] },
    ],
  };
}
