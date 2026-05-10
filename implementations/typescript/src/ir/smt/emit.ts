/**
 * Recursive emission of an `IrFormula` to an SMT-LIB s-expression
 * string. Pure structural translation: no constraint reordering, no
 * simplification, no axiom generation. The output is the formula
 * verbatim in SMT-LIB form.
 *
 * Variable scoping: the IR's `IrFormulaLambda` carries a `varName`
 * that the quantifier builders mint as `_x0`, `_x1`, ...: already
 * unique by construction. Every `var` term inside the body refers to
 * the enclosing binder by that name. We emit binders as
 *   (forall ((_x0 Sort)) <body>)
 * and `var` references as the bare name, which is how SMT-LIB names
 * bound variables.
 *
 * If a name clash is observed (the same identifier used by two nested
 * binders), we rename inner binders with a "$<depth>" suffix. This is
 * defensive: the standard quantifier builders never produce clashes: * but a hand-rolled `IrFormula` could.
 */

import type {
  IrFormula,
  IrTerm,
  Sort,
} from "../formulas.js";
import { emitSort } from "./sorts.js";

/** Map from atomic-predicate name to SMT-LIB operator name. */
const PREDICATE_OPERATOR: Record<string, string> = {
  "=": "=",
  "≠": "distinct",
  "<": "<",
  "≤": "<=",
  ">": ">",
  "≥": ">=",
  // SMT-LIB BV theory predicates use their own names.
  bvult: "bvult",
  bvule: "bvule",
  bvugt: "bvugt",
  bvuge: "bvuge",
  bvslt: "bvslt",
  bvsle: "bvsle",
  bvsgt: "bvsgt",
  bvsge: "bvsge",
};

interface EmitContext {
  /** Stack of currently-bound variable names (innermost last). */
  binders: string[];
  /** Map of original binder name → emitted name when a clash forced rename. */
  rename: Map<string, string>;
}

function freshContext(): EmitContext {
  return { binders: [], rename: new Map() };
}

/** Public: emit a single formula. */
export function emitFormula(formula: IrFormula): string {
  return emitFormulaIn(formula, freshContext());
}

function emitFormulaIn(formula: IrFormula, ctx: EmitContext): string {
  switch (formula.kind) {
    case "forall":
    case "exists":
      return emitQuantifier(formula, ctx);

    case "and":
      if (formula.operands.length === 0) return "true";
      if (formula.operands.length === 1) {
        return emitFormulaIn(formula.operands[0]!, ctx);
      }
      return `(and ${formula.operands.map((c) => emitFormulaIn(c, ctx)).join(" ")})`;

    case "or":
      if (formula.operands.length === 0) return "false";
      if (formula.operands.length === 1) {
        return emitFormulaIn(formula.operands[0]!, ctx);
      }
      return `(or ${formula.operands.map((d) => emitFormulaIn(d, ctx)).join(" ")})`;

    case "not":
      return `(not ${emitFormulaIn(formula.operands[0]!, ctx)})`;

    case "implies":
      return `(=> ${emitFormulaIn(formula.operands[0]!, ctx)} ${emitFormulaIn(
        formula.operands[1]!,
        ctx,
      )})`;

    case "atomic":
      return emitAtomic(formula.name, formula.args, ctx);

    case "choice": {
      // Choice (εx. P(x)) in first-order = exists unique x. P(x)
      const varName = formula.varName;
      const sort = emitSort(formula.sort);
      const body = emitFormulaIn(formula.body, ctx);
      const varNameY = `${varName}_y`;
      const bodyY = body.replace(new RegExp(`\\b${varName}\\b`, "g"), varNameY);
      const uniqueBody = `(and ${body} (forall ((${varNameY} ${sort})) (=> ${bodyY} (= ${varNameY} ${varName}))))`;
      return `(exists ((${varName} ${sort})) ${uniqueBody})`;
    }
  }
}

function emitQuantifier(
  formula: Extract<IrFormula, { kind: "forall" | "exists" }>,
  ctx: EmitContext,
): string {
  const binder = formula.name;
  const emittedName = uniquifyBinder(binder, ctx);

  // Push the binder; remember whether a rename was needed.
  ctx.binders.push(emittedName);
  const prevRename = ctx.rename.get(binder);
  if (emittedName !== binder) {
    ctx.rename.set(binder, emittedName);
  }

  const body = emitFormulaIn(formula.body, ctx);

  // Pop and restore prior rename mapping.
  ctx.binders.pop();
  if (emittedName !== binder) {
    if (prevRename === undefined) {
      ctx.rename.delete(binder);
    } else {
      ctx.rename.set(binder, prevRename);
    }
  }

  const head = formula.kind === "forall" ? "forall" : "exists";
  return `(${head} ((${emittedName} ${emitSort(formula.sort)})) ${body})`;
}

/** Pick a fresh binder name not currently in scope. */
function uniquifyBinder(name: string, ctx: EmitContext): string {
  if (!ctx.binders.includes(name)) return name;
  let depth = ctx.binders.length;
  let candidate = `${name}$${depth}`;
  while (ctx.binders.includes(candidate)) {
    depth += 1;
    candidate = `${name}$${depth}`;
  }
  return candidate;
}

function emitAtomic(predicateName: string, args: IrTerm[], ctx: EmitContext): string {
  if (predicateName === "true") {
    if (args.length === 0) return "true";
    return emitTerm(args[0]!, ctx);
  }
  if (predicateName === "false") {
    if (args.length === 0) return "false";
    return `(not ${emitTerm(args[0]!, ctx)})`;
  }

  const op = PREDICATE_OPERATOR[predicateName];
  if (op !== undefined) {
    const argText = args.map((a) => emitTerm(a, ctx)).join(" ");
    return args.length === 0 ? `(${op})` : `(${op} ${argText})`;
  }

  // member / subset are NOT SMT-LIB base names. We render them as
  // uninterpreted predicate applications; declarations.ts emits the
  // matching `(declare-fun member ... Bool)` so the kit's axioms can
  // pin their meaning.
  // Default: treat as an uninterpreted predicate symbol.
  if (args.length === 0) return `(${predicateName})`;
  const argText = args.map((a) => emitTerm(a, ctx)).join(" ");
  return `(${predicateName} ${argText})`;
}

function emitTerm(term: IrTerm, ctx: EmitContext): string {
  switch (term.kind) {
    case "var": {
      const renamed = ctx.rename.get(term.name);
      return renamed ?? term.name;
    }
    case "const":
      return emitConst(term.value, term.sort);
    case "ctor": {
      // SMT-LIB indexed operator: extract takes its hi/lo as part of the
      // operator, not as ordinary arguments. The IR encodes the indices
      // as Int constants in args[0] and args[1] for self-description.
      if (term.name === "extract" && term.args.length === 3) {
        const hi = readBigIntConst(term.args[0]!, "extract hi");
        const lo = readBigIntConst(term.args[1]!, "extract lo");
        const inner = emitTerm(term.args[2]!, ctx);
        return `((_ extract ${hi.toString()} ${lo.toString()}) ${inner})`;
      }
      if (term.args.length === 0) return term.name;
      const args = term.args.map((a) => emitTerm(a, ctx)).join(" ");
      return `(${term.name} ${args})`;
    }

    case "lambda": {
      // SMT-LIB lambda (available in newer versions)
      const paramName = term.paramName;
      const paramSort = emitSort(term.paramSort);
      const body = emitTerm(term.body, ctx);
      return `(lambda ((${paramName} ${paramSort})) ${body})`;
    }

    case "let": {
      const bindings = term.bindings.map(b => {
        const name = b.name;
        const boundTerm = emitTerm(b.boundTerm, ctx);
        return `(${name} ${boundTerm})`;
      }).join(" ");
      const body = emitTerm(term.body, ctx);
      return `(let (${bindings}) ${body})`;
    }
  }
}

function readBigIntConst(t: IrTerm, label: string): bigint {
  if (t.kind !== "const") {
    throw new Error(`SMT emit: ${label} must be a constant term, got kind "${t.kind}"`);
  }
  if (typeof t.value === "bigint") return t.value;
  if (typeof t.value === "number" && Number.isInteger(t.value)) return BigInt(t.value);
  throw new Error(`SMT emit: ${label} must be an integer constant`);
}

/** Render a literal value in SMT-LIB syntax for its declared sort. */
function emitConst(value: unknown, sort: Sort): string {
  if (value === null || value === undefined) {
    throw new Error(
      `SMT emit: null/undefined constant has no SMT-LIB encoding (sort ${
        sort.kind === "primitive" ? sort.name : sort.kind
      }). The kit must model nullability as an explicit ctor.`,
    );
  }
  // BV literals: render as the SMT-LIB indexed numeral `(_ bv N W)`.
  // The sort carries the width; the value is the unsigned bit pattern.
  if (sort.kind === "bitvec") {
    let big: bigint;
    if (typeof value === "bigint") big = value;
    else if (typeof value === "number" && Number.isInteger(value)) big = BigInt(value);
    else {
      throw new Error(
        `SMT emit: BV constant must be an integer (got ${typeof value})`,
      );
    }
    const modulus = 1n << BigInt(sort.width);
    let normalized = big % modulus;
    if (normalized < 0n) normalized += modulus;
    return `(_ bv${normalized.toString()} ${sort.width})`;
  }
  if (typeof value === "boolean") {
    return value ? "true" : "false";
  }
  if (typeof value === "bigint") {
    return value < 0n ? `(- ${(-value).toString()})` : value.toString();
  }
  if (typeof value === "number") {
    if (sort.kind === "primitive" && sort.name === "Real") {
      return emitRealLiteral(value);
    }
    return emitIntLiteral(value);
  }
  if (typeof value === "string") {
    return `"${escapeSmtString(value)}"`;
  }
  // Fallback: stringify and quote.
  return `"${escapeSmtString(String(value))}"`;
}

function emitIntLiteral(n: number): string {
  if (!Number.isFinite(n)) {
    throw new Error(`SMT emit: cannot encode non-finite number ${n} as Int`);
  }
  const rounded = Math.trunc(n);
  if (rounded < 0) return `(- ${(-rounded).toString()})`;
  return rounded.toString();
}

function emitRealLiteral(n: number): string {
  if (!Number.isFinite(n)) {
    throw new Error(`SMT emit: cannot encode non-finite number ${n} as Real`);
  }
  if (Number.isInteger(n)) {
    if (n < 0) return `(- ${(-n).toFixed(1)})`;
    return n.toFixed(1);
  }
  if (n < 0) return `(- ${(-n).toString()})`;
  return n.toString();
}

/**
 * Escape a string for SMT-LIB string literal. SMT-LIB doubles
 * embedded quotes (`""` for one `"`).
 */
function escapeSmtString(s: string): string {
  return s.replace(/"/g, '""');
}
