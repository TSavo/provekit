/**
 * IR → CVC5-flavored SMT-LIB translator.
 *
 * Parallel to src/ir/smt/ (Z3-flavored). Most of QF_LIA / QF_LRA SMT-LIB
 * is identical between Z3 and CVC5; this module exists to (a) carry a
 * separate producer identity into the memento store so cross-solver
 * mementos differ at the producer-id level even when the bytes match,
 * and (b) own any solver-specific dialect quirks that emerge: logic
 * naming, options preamble, get-model invocation.
 *
 * Public surface mirrors src/ir/smt/index.ts:
 *   - emitCvc5SmtLib(formula): single formula s-expression (no preamble).
 *   - emitCvc5SmtLibProblem({ axioms, assertion, logic? }): full script.
 *
 * Dialect handling:
 *   - Logic: CVC5 historically rejected `(set-logic ALL)` in favor of
 *     `(set-logic ALL_SUPPORTED)` on older versions. Modern CVC5 accepts
 *     `ALL`, so we emit `ALL` like the Z3 path. If incompatibility ever
 *     surfaces, this is the single line to switch.
 *   - Options: CVC5 needs `(set-option :produce-models true)` BEFORE
 *     `(set-logic ...)` for `(get-model)` to work after sat. We always
 *     emit it so the produced script is uniformly model-capable.
 *   - Body emission: pure pass-through to the shared `emitFormula`.
 *     Atomic predicates, quantifiers, connectives all share SMT-LIB
 *     v2.6 syntax across solvers.
 */

import type { IrFormula } from "../formulas.js";
import { emitFormula } from "../smt/emit.js";
import {
  collectDeclarations,
  emitFunctionDeclarations,
  emitSortDeclarations,
} from "../smt/declarations.js";

/** Render a single formula as an SMT-LIB s-expression. */
export function emitCvc5SmtLib(formula: IrFormula): string {
  return emitFormula(formula);
}

export interface Cvc5ProblemArgs {
  /** Axioms the solver should assume (kit-supplied). */
  axioms: IrFormula[];
  /** The claim to verify. The script asserts its negation. */
  assertion: IrFormula;
  /** SMT-LIB logic name. Defaults to `"ALL"`. */
  logic?: string;
}

/**
 * Render a full CVC5-flavored SMT-LIB problem: options, set-logic,
 * declarations, axioms, the negated assertion, and `(check-sat)`. If
 * the solver returns `unsat`, the assertion is proven.
 *
 * Differences from src/ir/smt/index.ts:emitSmtLibProblem:
 *   - Emits `(set-option :produce-models true)` before `(set-logic ...)`
 *     so a downstream `(get-model)` after sat does not fail on CVC5.
 *   - Inserts a comment header tagging the script as CVC5-flavored so
 *     the byte-stream is distinct from the Z3 path, making it visually
 *     clear in mementos which dialect produced the verdict, even though
 *     the cache key is the IR not the bytes.
 */
export function emitCvc5SmtLibProblem(args: Cvc5ProblemArgs): string {
  const logic = args.logic ?? "ALL";
  const all = [...args.axioms, args.assertion];
  const decls = collectDeclarations(all);

  const lines: string[] = [];
  lines.push("; cvc5-flavored SMT-LIB script");
  lines.push("(set-option :produce-models true)");
  lines.push(`(set-logic ${logic})`);

  const sortLines = emitSortDeclarations(decls);
  if (sortLines.length > 0) {
    lines.push(...sortLines);
  }

  const funLines = emitFunctionDeclarations(decls);
  if (funLines.length > 0) {
    lines.push(...funLines);
  }

  if (args.axioms.length > 0) {
    lines.push("; --- axioms ---");
    for (const ax of args.axioms) {
      lines.push(`(assert ${emitFormula(ax)})`);
    }
  }

  lines.push("; --- assertion (negated for unsat-check) ---");
  lines.push(`(assert (not ${emitFormula(args.assertion)}))`);
  lines.push("(check-sat)");

  return lines.join("\n") + "\n";
}
