/**
 * IR → SMT-LIB translator.
 *
 * Public surface:
 *   - `emitSmtLib(formula)`: render a single IrFormula as the SMT-LIB
 *     s-expression string for the formula body. No preamble, no
 *     declarations, no `assert` / `check-sat`. Useful for embedding
 *     into a larger problem the caller assembles.
 *   - `emitSmtLibProblem({ axioms, assertion, logic? })`: render a
 *     full SMT-LIB script with sort + function declarations, the
 *     axioms asserted, and the negated assertion + `(check-sat)`.
 *
 * Scope discipline (per spec): the translator MINTS SMT-LIB. It does
 * NOT invoke a solver, parse solver output, or interpret verdicts.
 * Running the script is the prover stage's job.
 */

import type { IrFormula } from "../formulas.js";
import { emitFormula } from "./emit.js";
import {
  collectDeclarations,
  emitFunctionDeclarations,
  emitSortDeclarations,
} from "./declarations.js";

/** Render a single formula as an SMT-LIB s-expression. */
export function emitSmtLib(formula: IrFormula): string {
  return emitFormula(formula);
}

export interface SmtProblemArgs {
  /** Axioms the solver should assume (kit-supplied). */
  axioms: IrFormula[];
  /** The claim to verify. The script asserts its negation. */
  assertion: IrFormula;
  /** SMT-LIB logic name. Defaults to `"ALL"`. */
  logic?: string;
}

/**
 * Render a full SMT-LIB problem: preamble, declarations, axioms, the
 * negated assertion, and `(check-sat)`. If the solver returns `unsat`,
 * the assertion is proven.
 */
export function emitSmtLibProblem(args: SmtProblemArgs): string {
  const logic = args.logic ?? "ALL";
  const all = [...args.axioms, args.assertion];
  const decls = collectDeclarations(all);

  const lines: string[] = [];
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
