/**
 * IR -> Lean 4 translator.
 *
 * Parallel to src/ir/smt/. Z3/CVC5 produce SMT-LIB; Lean is the
 * proof-assistant leg of the cross-paradigm composition test. The
 * architectural claim (protocol/specs/2026-04-29-the-semantic-envelope.md):
 * SMT verdicts and Lean proofs both attach as leaves to the same IR CID.
 *
 * Public surface:
 *   - emitLean(formula): single Lean 4 expression (no preamble, no
 *     theorem wrapper). Useful when the caller assembles its own file.
 *   - emitLeanTheorem({ axioms, assertion, name? }): full Lean 4 source
 *     for the theorem statement, with axiom declarations for user sorts,
 *     ctors, and uninterpreted predicates. Output is the statement only —
 *     the proof body is `:= sorry` so the file parses; the proof is the
 *     next stage's job (the Action concatenates the user's proof file).
 *
 * Scope (per spec): plain Lean 4. Bool / Int / String / user-defined
 * primitive sorts only. Real / Set / tuple / function sorts throw a
 * structured `LeanUnsupportedError` rather than mistranslating. See
 * src/ir/lean/sorts.ts for the divergence list.
 *
 * Theorem naming: the theorem statement needs a name for the proof file
 * to reference. The caller may pass one explicitly; otherwise a stable
 * deterministic name is derived from the formula body (a sha256 prefix
 * of the rendered expression). Pure: identical formula -> identical name.
 */

import { createHash } from "crypto";
import type { IrFormula } from "../formulas.js";
import { emitFormula } from "./emit.js";
import {
  collectDeclarations,
  emitFunctionDeclarations,
  emitSortDeclarations,
} from "./declarations.js";

export { LeanUnsupportedError } from "./sorts.js";

/** Render a single formula as a Lean 4 expression (no preamble). */
export function emitLean(formula: IrFormula): string {
  return emitFormula(formula);
}

export interface LeanTheoremArgs {
  /** Axioms the kit supplies; emitted as `axiom <name> : ...` lines. */
  axioms: IrFormula[];
  /** The theorem statement. */
  assertion: IrFormula;
  /**
   * Theorem identifier. Must be a valid Lean identifier. If omitted, a
   * stable name `prop_<sha256-prefix>` is derived from the rendered
   * assertion so the same formula always yields the same theorem name.
   */
  name?: string;
}

export interface LeanTheoremOutput {
  /** Full Lean source for the theorem statement (with `sorry` proof). */
  source: string;
  /** Theorem identifier the source declares. */
  theoremName: string;
}

/**
 * Build the full Lean 4 source for a theorem. Layout:
 *
 *   -- ProvekIt: IR -> Lean 4 theorem statement
 *   axiom <UserSort> : Type
 *   axiom <ctor> : Arg -> Result
 *   axiom <pred> : Arg -> Prop
 *
 *   theorem <name> : <assertion-with-axiom-implications> :=
 *     sorry
 *
 * Axioms become hypotheses via implication chaining: an axiom A is folded
 * into the statement as `A -> <assertion>`. This keeps the theorem self-
 * contained and avoids relying on Lean's `axiom` declarations for content
 * (we use `axiom` only for opaque types and uninterpreted symbols, not
 * for asserted axioms — those are folded into the statement so the proof
 * gets them as hypotheses).
 */
export function emitLeanTheorem(args: LeanTheoremArgs): LeanTheoremOutput {
  const all = [...args.axioms, args.assertion];
  const decls = collectDeclarations(all);

  // Build the proposition: for each axiom A, prepend `A ->` to the
  // assertion. The proof gets the axioms as hypotheses.
  const assertionExpr = emitFormula(args.assertion);
  const axiomExprs = args.axioms.map((a) => emitFormula(a));
  const propText = axiomExprs.length === 0
    ? assertionExpr
    : axiomExprs.map((ax) => `(${ax})`).join(" → ") + " → " + assertionExpr;

  const theoremName = args.name ?? deriveTheoremName(propText);

  const lines: string[] = [];
  lines.push("-- ProvekIt: IR -> Lean 4 theorem statement");

  const sortLines = emitSortDeclarations(decls);
  if (sortLines.length > 0) {
    lines.push(...sortLines);
  }

  const funLines = emitFunctionDeclarations(decls);
  if (funLines.length > 0) {
    lines.push(...funLines);
  }

  if (sortLines.length > 0 || funLines.length > 0) {
    lines.push("");
  }

  lines.push(`theorem ${theoremName} : ${propText} := by`);
  lines.push("  sorry");

  return {
    source: lines.join("\n") + "\n",
    theoremName,
  };
}

function deriveTheoremName(rendered: string): string {
  const h = createHash("sha256").update(rendered, "utf-8").digest("hex");
  return `prop_${h.slice(0, 16)}`;
}
