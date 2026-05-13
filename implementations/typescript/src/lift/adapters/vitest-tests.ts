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
 * LAYERED LIFT (mirrors the Rust adapter):
 *
 *   Layer 0 (mechanical): each `expect(actual).matcher(expected)` lifts
 *   to one atomic memento named "<test>::<idx>". Operands must be in
 *   the v0 whitelist (identifier, numeric/string literal, single-arg
 *   call). Anything richer skips with a warning.
 *
 *   Layer 2 (structural): three patterns Layer 0 cannot reach.
 *     Pattern 1 - bounded loop:
 *       it("...", () => {
 *         for (let i = 0; i < N; i++) { expect(...).toBe(...); }
 *       });
 *     Lifts to: forall i:Int. (lo<=i AND i<hi) implies (atomic).
 *
 *     Pattern 2 - helper-function inlining:
 *       function checkPalindrome(s) { expect(s).toBe(...); }
 *       it("palindromes", () => {
 *         checkPalindrome("racecar");
 *         checkPalindrome("level");
 *       });
 *     Lifts to: one memento per call site, body = helper's expect with
 *     the formal parameter substituted by the literal argument.
 *
 *     Pattern 3 - characterization conjunction:
 *       it("sort_preserves", () => {
 *         expect(...).toBe(...);
 *         expect(...).toBeGreaterThan(...);
 *         expect(...).toEqual(...);
 *       });
 *     Lifts to: one memento "<test>" with body = and(...) of all
 *     liftable atoms. Triggered only when there are >=2 expect-calls
 *     and every top-level statement is an expect-call.
 *
 *   Dispatch: Layer 2 runs first; tests it claims are skipped by Layer
 *   0. Tests neither layer claims fall through to (future) Layer 3.
 *
 * Honest under-coverage beats polluting the lattice with unverifiable
 * atoms.
 */

import ts from "typescript";
import type { IrFormula, IrTerm, AtomicFormula } from "../../ir/formulas.js";
import { Int, String as StringSort } from "../../ir/sorts.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "vitest-tests";

/**
 * Top-level entry. Runs Layer 2 first; tests it claims are skipped by
 * Layer 0. Returns a single AdapterOutput merging both passes (the
 * adapter-level counts in `seen`, `lifted`, `warnings` aggregate both
 * layers; per-pattern detail is logged inside warnings only).
 */
export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  // Pass 1: Layer 2 patterns + claim set.
  const helpers = collectHelpers(sourceFile);
  const l2 = liftLayer2(sourceFile, sourcePath, helpers);

  // Pass 2: Layer 0 with claim-set filter.
  const l0 = liftLayer0(sourceFile, sourcePath, l2.claimedTests);

  return {
    decls: [...l2.decls, ...l0.decls],
    seen: l2.seen + l0.seen,
    lifted: l2.decls.length + l0.decls.length,
    warnings: [...l2.warnings, ...l0.warnings],
  };
}

/**
 * Layer 0: each top-level `expect(...).matcher(...)` lifts to its own
 * point-specific memento. `claimed` lists test names Layer 2 has
 * already taken ownership of; Layer 0 skips them entirely.
 */
function liftLayer0(
  sourceFile: ts.SourceFile,
  sourcePath: string,
  claimed: Set<string>,
): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  let seen = 0;

  const visit = (node: ts.Node): void => {
    const block = extractItBlock(node);
    if (block) {
      if (claimed.has(block.testName)) {
        return; // Layer 2 owns it.
      }
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
  // contracts of the SUT: they're scaffolding.)
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
 * chain: `expect(x).toBe(1)` is one candidate, not two.
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
  // Async / promise modifiers: skip in v0.
  for (const m of chain.modifiers) {
    if (PROMISE_MODIFIERS.has(m)) {
      return {
        kind: "skip",
        reason: `expect(...).${m}.<matcher>() is async; not liftable in v0`,
      };
    }
  }
  // toThrow: skip in v0 (negative-fact, function-valued actual).
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
  // Free-function call (zero-or-more args) and `Module.fn(...)` static
  // call. v0.6 relaxes v0's single-arg gate; every argument must itself
  // lift. Method calls (`recv.fn(...)`) are handled by the next clause.
  if (
    ts.isCallExpression(expr) &&
    (ts.isIdentifier(expr.expression) ||
      (ts.isPropertyAccessExpression(expr.expression) &&
        ts.isIdentifier(expr.expression.expression) &&
        ts.isIdentifier(expr.expression.name)))
  ) {
    const callee = expr.expression;
    let name: string;
    if (ts.isIdentifier(callee)) {
      name = callee.text;
    } else {
      // PropertyAccess; bare-identifier receiver only (no chains).
      const pa = callee as ts.PropertyAccessExpression;
      name = `${(pa.expression as ts.Identifier).text}.${pa.name.text}`;
    }
    const argTerms: IrTerm[] = [];
    for (const a of expr.arguments) {
      const lifted = liftOperand(a);
      if (lifted.kind === "skip") return lifted;
      argTerms.push(lifted.term);
    }
    return { kind: "ok", term: { kind: "ctor", name, args: argTerms } };
  }
  // v0.6: method call on operand. `recv.method(...args)` lifts as a
  // UFCS-style ctor where the receiver becomes the first argument.
  // Mirrors Rust v0.5 PR #55. We require both receiver and every
  // argument to themselves be liftable; this composes recursively
  // through nested chains.
  if (
    ts.isCallExpression(expr) &&
    ts.isPropertyAccessExpression(expr.expression) &&
    ts.isIdentifier(expr.expression.name) &&
    !expr.expression.questionDotToken
  ) {
    const recv = liftOperand(expr.expression.expression);
    if (recv.kind === "skip") return recv;
    const argTerms: IrTerm[] = [];
    for (const a of expr.arguments) {
      const lifted = liftOperand(a);
      if (lifted.kind === "skip") return lifted;
      argTerms.push(lifted.term);
    }
    return {
      kind: "ok",
      term: { kind: "ctor", name: expr.expression.name.text, args: [recv.term, ...argTerms] },
    };
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
  // v0.6: array literal `[a, b, c]` lifts to `Ctor("array", [a, b, c])`.
  // Mirrors Rust v0.5 (PR #55) `array` ctor naming. Every element
  // must itself lift; sparse / spread elements skip.
  if (ts.isArrayLiteralExpression(expr)) {
    const argTerms: IrTerm[] = [];
    for (const el of expr.elements) {
      if (ts.isOmittedExpression(el) || ts.isSpreadElement(el)) {
        return {
          kind: "skip",
          reason: "array literal with omitted or spread element is not in v0.6",
        };
      }
      const lifted = liftOperand(el);
      if (lifted.kind === "skip") return lifted;
      argTerms.push(lifted.term);
    }
    return { kind: "ok", term: { kind: "ctor", name: "array", args: argTerms } };
  }
  // v0.6: binary operators in operand position (`a + b`, `a - b`,
  // `a * b`, `a / b`, `a % b`). Lifts to `Ctor("<op>", [a, b])`. Both
  // operands must themselves lift. Comparison and logical operators
  // stay out of the term grammar; they live in formula position.
  if (ts.isBinaryExpression(expr)) {
    const opName = binaryOperatorCtorName(expr.operatorToken.kind);
    if (opName) {
      const left = liftOperand(expr.left);
      if (left.kind === "skip") return left;
      const right = liftOperand(expr.right);
      if (right.kind === "skip") return right;
      return {
        kind: "ok",
        term: { kind: "ctor", name: opName, args: [left.term, right.term] },
      };
    }
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
  // v0.6: ternary `cond ? a : b` is deliberately deferred. Lifting it
  // would require either a Ctor("ternary", [cond, a, b]) shape or
  // promotion to a formula-position if-then-else; both are out of
  // scope for this slice. Skip with a named reason so the report
  // taxonomy is searchable.
  if (ts.isConditionalExpression(expr)) {
    return {
      kind: "skip",
      reason: "ternary `cond ? a : b` operand is not lifted in v0.6",
    };
  }
  return {
    kind: "skip",
    reason:
      "operand shape not in v0.6 lift whitelist (no member chains, indexing, field access, closures, blocks, ranges, comparisons in operand position, or template literals)",
  };
}

// ---------------------------------------------------------------------------
// Layer 2: bounded loops, helper inlining, characterization conjunction.
// ---------------------------------------------------------------------------

interface Layer2Output {
  decls: ContractDecl[];
  warnings: AdapterWarning[];
  seen: number;
  claimedTests: Set<string>;
}

interface HelperDef {
  paramName: string;
  /** The single expect-call within the helper body. */
  expectCall: ts.CallExpression;
}

function collectHelpers(sf: ts.SourceFile): Map<string, HelperDef> {
  const map = new Map<string, HelperDef>();
  const visit = (n: ts.Node): void => {
    if (ts.isFunctionDeclaration(n) && n.name && n.body) {
      const def = helperDefFromFunction(n.parameters, n.body);
      if (def) map.set(n.name.text, def);
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return map;
}

function helperDefFromFunction(
  params: ts.NodeArray<ts.ParameterDeclaration>,
  body: ts.Block,
): HelperDef | null {
  if (params.length !== 1) return null;
  const p = params[0]!;
  if (!ts.isIdentifier(p.name)) return null;
  if (body.statements.length !== 1) return null;
  const stmt = body.statements[0]!;
  if (!ts.isExpressionStatement(stmt)) return null;
  if (!ts.isCallExpression(stmt.expression)) return null;
  if (!isExpectChain(stmt.expression)) return null;
  return { paramName: p.name.text, expectCall: stmt.expression };
}

function liftLayer2(
  sf: ts.SourceFile,
  sourcePath: string,
  helpers: Map<string, HelperDef>,
): Layer2Output {
  const out: Layer2Output = {
    decls: [],
    warnings: [],
    seen: 0,
    claimedTests: new Set(),
  };

  const visit = (n: ts.Node): void => {
    const block = extractItBlock(n);
    if (block) {
      classifyAndLift(block, sourcePath, helpers, out);
      return;
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

function classifyAndLift(
  block: ItBlock,
  sourcePath: string,
  helpers: Map<string, HelperDef>,
  out: Layer2Output,
): void {
  if (!ts.isBlock(block.body)) return;
  const stmts = block.body.statements;
  if (stmts.length === 0) return;

  // PATTERN 1: single `for` loop with a single-stmt body containing one
  // expect-call.
  if (stmts.length === 1) {
    const s = stmts[0]!;
    if (ts.isForStatement(s)) {
      classifyForLoop(s, block.testName, sourcePath, out);
      return;
    }
  }

  // PATTERN 2: every top-level statement is an ExpressionStatement whose
  // expression is a single-arg call to a known helper.
  const helperCalls = collectHelperCalls(stmts, helpers);
  if (helperCalls && helperCalls.length > 0) {
    classifyHelperInlining(helperCalls, helpers, block.testName, sourcePath, out);
    return;
  }

  // PATTERN 3: every top-level stmt is an expect-call AND >=2 of them.
  const expectStmts: ts.CallExpression[] = [];
  let allExpects = true;
  for (const s of stmts) {
    if (
      ts.isExpressionStatement(s) &&
      ts.isCallExpression(s.expression) &&
      isExpectChain(s.expression)
    ) {
      expectStmts.push(s.expression);
    } else {
      allExpects = false;
      break;
    }
  }
  if (allExpects && expectStmts.length >= 2) {
    classifyCharacterization(expectStmts, block.testName, sourcePath, out);
    return;
  }
}

function classifyForLoop(
  fs: ts.ForStatement,
  testName: string,
  sourcePath: string,
  out: Layer2Output,
): void {
  out.claimedTests.add(testName);
  out.seen += 1;

  // Recognize `for (let i = lit; i < lit; i++)` / `i <= lit` / `i += 1`.
  const init = fs.initializer;
  const cond = fs.condition;
  const inc = fs.incrementor;
  const body = fs.statement;

  let varName: string | null = null;
  let lo: number | null = null;
  let hi: number | null = null;
  let inclusive = false;

  if (
    init &&
    ts.isVariableDeclarationList(init) &&
    init.declarations.length === 1 &&
    ts.isIdentifier(init.declarations[0]!.name) &&
    init.declarations[0]!.initializer
  ) {
    varName = init.declarations[0]!.name.text;
    lo = literalNumber(init.declarations[0]!.initializer);
  }
  if (
    cond &&
    ts.isBinaryExpression(cond) &&
    ts.isIdentifier(cond.left) &&
    cond.left.text === varName
  ) {
    if (cond.operatorToken.kind === ts.SyntaxKind.LessThanToken) {
      hi = literalNumber(cond.right);
      inclusive = false;
    } else if (cond.operatorToken.kind === ts.SyntaxKind.LessThanEqualsToken) {
      hi = literalNumber(cond.right);
      inclusive = true;
    }
  }
  // Increment: i++, ++i, or i += 1. We don't differentiate; any
  // upward-by-one increment matches.
  let incOk = false;
  if (inc) {
    if (
      ts.isPostfixUnaryExpression(inc) &&
      inc.operator === ts.SyntaxKind.PlusPlusToken &&
      ts.isIdentifier(inc.operand) &&
      inc.operand.text === varName
    ) {
      incOk = true;
    } else if (
      ts.isPrefixUnaryExpression(inc) &&
      inc.operator === ts.SyntaxKind.PlusPlusToken &&
      ts.isIdentifier(inc.operand) &&
      inc.operand.text === varName
    ) {
      incOk = true;
    } else if (
      ts.isBinaryExpression(inc) &&
      inc.operatorToken.kind === ts.SyntaxKind.PlusEqualsToken &&
      ts.isIdentifier(inc.left) &&
      inc.left.text === varName &&
      literalNumber(inc.right) === 1
    ) {
      incOk = true;
    }
  }

  if (varName === null || lo === null || hi === null || !incOk) {
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason:
        "layer2 bounded-loop: only `for (let i = <lit>; i </<= <lit>; i++ | i+=1)` shape is liftable in v0",
    });
    return;
  }

  // Body must be a single expect-call (Block with one stmt OR a single
  // ExpressionStatement form).
  let bodyExpect: ts.CallExpression | null = null;
  let nestedFor = false;
  if (ts.isBlock(body)) {
    if (body.statements.length === 1) {
      const s = body.statements[0]!;
      if (
        ts.isExpressionStatement(s) &&
        ts.isCallExpression(s.expression) &&
        isExpectChain(s.expression)
      ) {
        bodyExpect = s.expression;
      } else if (ts.isForStatement(s)) {
        nestedFor = true;
      }
    }
  } else if (ts.isExpressionStatement(body)) {
    if (ts.isCallExpression(body.expression) && isExpectChain(body.expression)) {
      bodyExpect = body.expression;
    }
  } else if (ts.isForStatement(body)) {
    nestedFor = true;
  }

  if (nestedFor) {
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason: "layer2 bounded-loop: nested for-loop detected; deferred to Layer 2.5",
    });
    return;
  }
  if (!bodyExpect) {
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason: "layer2 bounded-loop: body is not a single expect-call",
    });
    return;
  }

  const r = liftExpect(bodyExpect);
  if (r.kind === "skip") {
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason: `layer2 bounded-loop: inner expect not liftable: ${r.reason}`,
    });
    return;
  }

  // Build forall i:Int. (lo<=i AND i</<=hi) implies (atomic).
  const varTerm: IrTerm = { kind: "var", name: varName };
  const lower: AtomicFormula = {
    kind: "atomic",
    name: "≥",
    args: [varTerm, { kind: "const", value: lo, sort: Int }],
  };
  const upper: AtomicFormula = {
    kind: "atomic",
    name: inclusive ? "≤" : "<",
    args: [varTerm, { kind: "const", value: hi, sort: Int }],
  };
  const antecedent: IrFormula = { kind: "and", operands: [lower, upper] };
  const implies: IrFormula = {
    kind: "implies",
    operands: [antecedent, r.formula],
  };
  const quantified: IrFormula = {
    kind: "forall",
    name: varName,
    sort: Int,
    body: implies,
  };

  out.decls.push({
    name: testName,
    outBinding: "out",
    sourcePath,
    adapter: ADAPTER,
    inv: quantified,
  });
}

interface HelperCall {
  helperName: string;
  arg: ts.Expression;
}

function collectHelperCalls(
  stmts: ts.NodeArray<ts.Statement>,
  helpers: Map<string, HelperDef>,
): HelperCall[] | null {
  const out: HelperCall[] = [];
  for (const s of stmts) {
    if (!ts.isExpressionStatement(s)) return null;
    if (!ts.isCallExpression(s.expression)) return null;
    const call = s.expression;
    if (!ts.isIdentifier(call.expression)) return null;
    const name = call.expression.text;
    if (!helpers.has(name)) return null;
    if (call.arguments.length !== 1) return null;
    out.push({ helperName: name, arg: call.arguments[0]! });
  }
  return out;
}

function classifyHelperInlining(
  calls: HelperCall[],
  helpers: Map<string, HelperDef>,
  testName: string,
  sourcePath: string,
  out: Layer2Output,
): void {
  out.claimedTests.add(testName);

  for (let i = 0; i < calls.length; i++) {
    const call = calls[i]!;
    const helper = helpers.get(call.helperName)!;
    out.seen += 1;
    const memento = `${testName}::call::${i}`;

    const argTerm = liftOperand(call.arg);
    if (argTerm.kind === "skip") {
      out.warnings.push({
        adapter: ADAPTER,
        sourcePath,
        itemName: memento,
        reason: `layer2 helper-inline: argument not liftable: ${argTerm.reason}`,
      });
      continue;
    }

    const helperResult = liftExpect(helper.expectCall);
    if (helperResult.kind === "skip") {
      out.warnings.push({
        adapter: ADAPTER,
        sourcePath,
        itemName: memento,
        reason: `layer2 helper-inline: helper \`${call.helperName}\` body not liftable: ${helperResult.reason}`,
      });
      continue;
    }
    const inlined = substVarInFormula(helperResult.formula, helper.paramName, argTerm.term);
    out.decls.push({
      name: memento,
      outBinding: "out",
      sourcePath,
      adapter: ADAPTER,
      inv: inlined,
    });
  }
}

function classifyCharacterization(
  expects: ts.CallExpression[],
  testName: string,
  sourcePath: string,
  out: Layer2Output,
): void {
  out.claimedTests.add(testName);
  out.seen += 1;

  const atoms: IrFormula[] = [];
  const skipped: string[] = [];
  for (let i = 0; i < expects.length; i++) {
    const r = liftExpect(expects[i]!);
    if (r.kind === "ok") atoms.push(r.formula);
    else skipped.push(`#${i}: ${r.reason}`);
  }
  if (atoms.length < 2) {
    // Release claim so Layer 0 can still try the individual asserts.
    out.claimedTests.delete(testName);
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason: `layer2 characterization: only ${atoms.length} of ${expects.length} expects were liftable; releasing to layer 0`,
    });
    return;
  }
  const conj: IrFormula = { kind: "and", operands: atoms };
  out.decls.push({
    name: testName,
    outBinding: "out",
    sourcePath,
    adapter: ADAPTER,
    inv: conj,
  });
  if (skipped.length > 0) {
    out.warnings.push({
      adapter: ADAPTER,
      sourcePath,
      itemName: testName,
      reason: `layer2 characterization: ${skipped.length} atoms skipped from conjunction: ${skipped.join("; ")}`,
    });
  }
}

/**
 * v0.6: map a binary operator token to a Ctor name when the operator
 * is liftable in term (operand) position. Returns null for operators
 * that don't belong here, like comparisons and short-circuit logicals,
 * which live in formula position rather than term position.
 */
function binaryOperatorCtorName(kind: ts.SyntaxKind): string | null {
  switch (kind) {
    case ts.SyntaxKind.PlusToken: return "+";
    case ts.SyntaxKind.MinusToken: return "-";
    case ts.SyntaxKind.AsteriskToken: return "*";
    case ts.SyntaxKind.SlashToken: return "/";
    case ts.SyntaxKind.PercentToken: return "%";
    default: return null;
  }
}

function literalNumber(expr: ts.Expression): number | null {
  if (ts.isNumericLiteral(expr)) return Number(expr.text);
  if (
    ts.isPrefixUnaryExpression(expr) &&
    expr.operator === ts.SyntaxKind.MinusToken &&
    ts.isNumericLiteral(expr.operand)
  ) {
    return -Number(expr.operand.text);
  }
  return null;
}

function substVarInFormula(f: IrFormula, formal: string, actual: IrTerm): IrFormula {
  if (f.kind === "atomic") {
    return {
      kind: "atomic",
      name: f.name,
      args: f.args.map((t) => substVarInTerm(t, formal, actual)),
    };
  }
  if (f.kind === "forall" || f.kind === "exists") {
    // quantifier: don't substitute under shadowing binder.
    if (f.name === formal) return f;
    return {
      kind: f.kind,
      name: f.name,
      sort: f.sort,
      body: substVarInFormula(f.body, formal, actual),
    };
  }
  // connective: and / or / not / implies
  if (f.kind === "and" || f.kind === "or" || f.kind === "not" || f.kind === "implies") {
    return {
      kind: f.kind,
      operands: f.operands.map((o: IrFormula) => substVarInFormula(o, formal, actual)),
    };
  }
  return f;
}

function substVarInTerm(t: IrTerm, formal: string, actual: IrTerm): IrTerm {
  if (t.kind === "var" && t.name === formal) return actual;
  if (t.kind === "ctor") {
    return { kind: "ctor", name: t.name, args: t.args.map((a) => substVarInTerm(a, formal, actual)) };
  }
  return t;
}
