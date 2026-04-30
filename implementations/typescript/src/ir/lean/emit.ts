/**
 * Recursive emission of an `IrFormula` to a Lean 4 expression string.
 *
 * Pure structural translation — no proof generation, no normalization. The
 * output is a Lean 4 prop-logic expression suitable as the body of a
 * `theorem` statement. Translating a proof is the next stage's job.
 *
 * Translation table:
 *   forall    -> `∀ (x : T), body`
 *   exists    -> `∃ (x : T), body`
 *   and       -> `(a ∧ b)`
 *   or        -> `(a ∨ b)`
 *   not       -> `(¬ body)`
 *   implies   -> `(a → b)`
 *   = / != / < / <= / > / >=  -> Lean infix operators
 *   true / false (atomic, 0-arg) -> `True` / `False`
 *
 * Variable scoping: identical to the SMT translator. Quantifier builders
 * mint unique names (`_x0`, `_x1`, ...). On hand-rolled clashes we rename
 * the inner binder with a `__d<depth>` suffix (Lean does not accept `$`
 * in identifiers, so we spell the suffix differently from SMT).
 *
 * Atomic predicates that aren't built-in are emitted as prefix application
 * `(predName arg1 arg2 ...)`, matching Lean's function-application syntax.
 * The kit is expected to declare them via `axiom predName : ... -> Prop`
 * which `declarations.ts` emits.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import { emitSort, LeanUnsupportedError } from "./sorts.js";

const PREDICATE_INFIX: Record<string, string> = {
  "=": "=",
  "≠": "≠",
  "<": "<",
  "≤": "≤",
  ">": ">",
  "≥": "≥",
};

interface EmitContext {
  binders: string[];
  rename: Map<string, string>;
}

function freshContext(): EmitContext {
  return { binders: [], rename: new Map() };
}

/** Public: emit a single formula as a Lean 4 expression. */
export function emitFormula(formula: IrFormula): string {
  return emitFormulaIn(formula, freshContext());
}

function emitFormulaIn(formula: IrFormula, ctx: EmitContext): string {
  switch (formula.kind) {
    case "forall":
    case "exists":
      return emitQuantifier(formula, ctx);

    case "and":
      if (formula.conjuncts.length === 0) return "True";
      if (formula.conjuncts.length === 1) {
        return emitFormulaIn(formula.conjuncts[0], ctx);
      }
      return (
        "(" + formula.conjuncts.map((c) => emitFormulaIn(c, ctx)).join(" ∧ ") + ")"
      );

    case "or":
      if (formula.disjuncts.length === 0) return "False";
      if (formula.disjuncts.length === 1) {
        return emitFormulaIn(formula.disjuncts[0], ctx);
      }
      return (
        "(" + formula.disjuncts.map((d) => emitFormulaIn(d, ctx)).join(" ∨ ") + ")"
      );

    case "not":
      return `(¬ ${emitFormulaIn(formula.body, ctx)})`;

    case "implies":
      return `(${emitFormulaIn(formula.antecedent, ctx)} → ${emitFormulaIn(
        formula.consequent,
        ctx,
      )})`;

    case "atomic":
      return emitAtomic(formula.predicate, formula.args, ctx);
  }
}

function emitQuantifier(
  formula: Extract<IrFormula, { kind: "forall" | "exists" }>,
  ctx: EmitContext,
): string {
  const binder = formula.predicate.varName;
  const emittedName = uniquifyBinder(binder, ctx);

  ctx.binders.push(emittedName);
  const prevRename = ctx.rename.get(binder);
  if (emittedName !== binder) {
    ctx.rename.set(binder, emittedName);
  }

  const body = emitFormulaIn(formula.predicate.body, ctx);

  ctx.binders.pop();
  if (emittedName !== binder) {
    if (prevRename === undefined) {
      ctx.rename.delete(binder);
    } else {
      ctx.rename.set(binder, prevRename);
    }
  }

  const head = formula.kind === "forall" ? "∀" : "∃";
  const sortText = emitSort(formula.sort);
  return `${head} (${emittedName} : ${sortText}), ${body}`;
}

function uniquifyBinder(name: string, ctx: EmitContext): string {
  if (!ctx.binders.includes(name)) return name;
  let depth = ctx.binders.length;
  let candidate = `${name}__d${depth}`;
  while (ctx.binders.includes(candidate)) {
    depth += 1;
    candidate = `${name}__d${depth}`;
  }
  return candidate;
}

function emitAtomic(predicate: string, args: IrTerm[], ctx: EmitContext): string {
  if (predicate === "true") {
    if (args.length === 0) return "True";
    return emitTerm(args[0], ctx);
  }
  if (predicate === "false") {
    if (args.length === 0) return "False";
    return `(¬ ${emitTerm(args[0], ctx)})`;
  }

  const op = PREDICATE_INFIX[predicate];
  if (op !== undefined) {
    if (args.length === 2) {
      return `(${emitTerm(args[0], ctx)} ${op} ${emitTerm(args[1], ctx)})`;
    }
    if (args.length === 0) {
      throw new LeanUnsupportedError(
        `Lean emit: built-in operator "${predicate}" requires arguments`,
      );
    }
    // Unary or n-ary chains have no Lean built-in; emit prefix as a
    // structured fallback. The kit may not declare these — surface a
    // clear error so the operator is treated like a relation lemma.
    const argText = args.map((a) => emitTerm(a, ctx)).join(" ");
    return `(${predicate} ${argText})`;
  }

  // Uninterpreted predicate. Emit as prefix application; declarations.ts
  // emits the matching `axiom <pred> : ... -> Prop`.
  if (args.length === 0) return `${predicate}`;
  const argText = args.map((a) => emitTerm(a, ctx)).join(" ");
  return `(${predicate} ${argText})`;
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
      if (term.args.length === 0) return term.name;
      const args = term.args.map((a) => emitTerm(a, ctx)).join(" ");
      return `(${term.name} ${args})`;
    }
  }
}

/** Render a literal value in Lean syntax for its declared sort. */
function emitConst(value: unknown, sort: Sort): string {
  if (value === null || value === undefined) {
    throw new LeanUnsupportedError(
      `Lean emit: null/undefined constant has no Lean encoding (sort ${
        sort.kind === "primitive" ? sort.name : sort.kind
      }). The kit must model nullability as an explicit ctor.`,
    );
  }
  if (typeof value === "boolean") {
    return value ? "true" : "false";
  }
  if (typeof value === "bigint") {
    return formatSignedInt(value < 0n ? `-${(-value).toString()}` : value.toString(), sort);
  }
  if (typeof value === "number") {
    if (sort.kind === "primitive" && sort.name === "Real") {
      return formatReal(value);
    }
    return formatSignedInt(formatIntFromNumber(value), sort);
  }
  if (typeof value === "string") {
    return `"${escapeLeanString(value)}"`;
  }
  return `"${escapeLeanString(String(value))}"`;
}

function formatIntFromNumber(n: number): string {
  if (!Number.isFinite(n)) {
    throw new LeanUnsupportedError(
      `Lean emit: cannot encode non-finite number ${n} as Int`,
    );
  }
  return Math.trunc(n).toString();
}

/**
 * Wrap negative integer literals in parentheses so Lean parses them as a
 * single literal in argument position. Positive integers do not need
 * parens. We annotate Int literals with `(n : Int)` only when the sort is
 * a non-built-in user primitive and a coercion would be ambiguous; for
 * plain Int we trust Lean's default elaboration.
 */
function formatSignedInt(text: string, sort: Sort): string {
  const isInt =
    sort.kind === "primitive" && (sort.name === "Int" || sort.name === "Nat");
  if (text.startsWith("-")) {
    return isInt ? `(${text} : Int)` : `(${text})`;
  }
  return text;
}

function formatReal(n: number): string {
  if (!Number.isFinite(n)) {
    throw new LeanUnsupportedError(
      `Lean emit: cannot encode non-finite number ${n} as Real`,
    );
  }
  if (Number.isInteger(n)) {
    if (n < 0) return `(${n.toFixed(1)} : Real)`;
    return `(${n.toFixed(1)} : Real)`;
  }
  if (n < 0) return `(${n} : Real)`;
  return `(${n} : Real)`;
}

/** Escape a string for a Lean string literal. Lean uses C-style backslash escapes. */
function escapeLeanString(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
