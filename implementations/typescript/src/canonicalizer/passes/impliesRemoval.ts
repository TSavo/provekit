/**
 * Pass 4: implies removal.
 *
 * Pre-condition: input is a CanonicalFolAst that may contain `implies` nodes
 *   (though post-predicates pass, implies nodes are still in the tree).
 *   NOTE: the CanonicalFolAst type does not define `implies` because after
 *   this pass it is gone. The input here is a pre-implies-removal form
 *   (the PreImpliesAst type below).
 * Post-condition: no `implies` nodes remain. Every `implies(a, c)` is
 *   rewritten to `or(not(a), c)`.
 *
 * This pass runs before NNF (pass 5). The `not` wrapping the antecedent
 * will be pushed inward by the NNF pass.
 */

import type { CanonicalFolAst, CanonicalSort } from "../ast.js";
import type { CanonicalTerm } from "../ast.js";

/**
 * Pre-NNF form: extends CanonicalFolAst with `implies`.
 * Used internally between passes 3 and 4.
 */
export type PreNnfAst =
  | { kind: "forall"; sort: CanonicalSort; body: PreNnfAst }
  | { kind: "exists"; sort: CanonicalSort; body: PreNnfAst }
  | { kind: "and"; operands: PreNnfAst[] }
  | { kind: "or"; operands: PreNnfAst[] }
  | { kind: "not"; operands: [PreNnfAst] }
  | { kind: "implies"; operands: [PreNnfAst, PreNnfAst] }
  | { kind: "atomic"; name: string; args: CanonicalTerm[] };

/**
 * Rewrite `implies(a, c)` → `or(not(a), c)` throughout the tree.
 */
export function removeImplies(ast: PreNnfAst): CanonicalFolAst {
  switch (ast.kind) {
    case "forall":
    case "exists":
      return { kind: ast.kind, sort: ast.sort, body: removeImplies(ast.body) };

    case "and":
      return { kind: "and", operands: ast.operands.map(removeImplies) };

    case "or":
      return { kind: "or", operands: ast.operands.map(removeImplies) };

    case "not":
      return { kind: "not", operands: [removeImplies(ast.operands[0])] };

    case "implies":
      // implies(a, c) → or(not(a), c)
      return {
        kind: "or",
        operands: [
          { kind: "not", operands: [removeImplies(ast.operands[0])] },
          removeImplies(ast.operands[1]),
        ],
      };

    case "atomic":
      return { kind: "atomic", name: ast.name, args: ast.args };
  }
}
