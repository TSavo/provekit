/**
 * Top-level canonicalization pipeline.
 *
 * Takes an IrFormula and runs all 8 passes in sequence:
 *   1. de Bruijn index replacement (pass 1)
 *   2. Predicate + sort + term canonicalization (passes 2+3, interleaved)
 *   3. implies removal (pass 4)
 *   4. NNF (pass 5)
 *   5. AC normalization (pass 6)
 *   6. Serialization (pass 7)
 *   7. Hash (pass 8)
 *
 * Each pass is a pure function. The pipeline is the composition.
 *
 * Note on pass interleaving: passes 2 (predicate), 3 (sort), and the
 * transition from DeBruijnFormula to PreNnfAst are collapsed into a
 * single recursive walk (`buildCanonicalAst`) for efficiency. The
 * spec permits this: "implementations may choose to compile multiple
 * passes into a single walk for performance."
 */

import type { IrFormula } from "./irFormula.js";
import type { CanonicalFolAst, CanonicalTerm } from "./ast.js";
import type { DeBruijnFormula, DeBruijnTerm } from "./passes/deBruijn.js";
import type { PreNnfAst } from "./passes/impliesRemoval.js";

import { applyDeBruijn } from "./passes/deBruijn.js";
import { canonicalizeSort } from "./passes/sorts.js";
import { canonicalizeTerm, canonicalizePredicate } from "./passes/predicates.js";
import { removeImplies } from "./passes/impliesRemoval.js";
import { toNnf } from "./passes/nnf.js";
import { acNormalize } from "./passes/acNormalize.js";
import { serializeCanonicalAst } from "./serialize.js";
import { computeCid } from "./hash.js";

// -----------------------------------------------------------------------
// Pass 1+2+3 collapsed: DeBruijnFormula → PreNnfAst
// -----------------------------------------------------------------------

/**
 * Convert a DeBruijnFormula (output of pass 1) into a PreNnfAst
 * (input to pass 4) by canonicalizing sorts, predicates, and terms.
 * This is passes 2 and 3 applied simultaneously during the tree walk.
 */
function buildPreNnfAst(formula: DeBruijnFormula): PreNnfAst {
  switch (formula.kind) {
    case "forall":
    case "exists":
      return {
        kind: formula.kind,
        sort: canonicalizeSort(formula.sort),
        body: buildPreNnfAst(formula.body),
      };

    case "and":
      return { kind: "and", operands: formula.conjuncts.map(buildPreNnfAst) };

    case "or":
      return { kind: "or", operands: formula.disjuncts.map(buildPreNnfAst) };

    case "not":
      return { kind: "not", operands: [buildPreNnfAst(formula.body)] };

    case "implies":
      return {
        kind: "implies",
        operands: [
          buildPreNnfAst(formula.antecedent),
          buildPreNnfAst(formula.consequent),
        ],
      };

    case "atomic": {
      // Canonicalize terms first (pass 2/3 on terms).
      const canonArgs: CanonicalTerm[] = formula.args.map(canonicalizeDeBruijnTerm);
      // Canonicalize predicate and possibly reorder args (pass 2).
      const { name, args } = canonicalizePredicate(formula.predicate, canonArgs);
      return { kind: "atomic", name, args };
    }
  }
}

function canonicalizeDeBruijnTerm(term: DeBruijnTerm): CanonicalTerm {
  // canonicalizeTerm in predicates.ts handles all three term kinds.
  return canonicalizeTerm(term);
}

// -----------------------------------------------------------------------
// Full pipeline
// -----------------------------------------------------------------------

/**
 * Run all 8 passes and return the canonical FOL AST.
 * The AST is in NNF, AC-normalized, with de Bruijn indices and
 * canonical sort/predicate names.
 */
export function formulaToCanonicalAst(formula: IrFormula): CanonicalFolAst {
  // Pass 1: de Bruijn index replacement.
  const deBruijn = applyDeBruijn(formula);

  // Passes 2+3: predicate + sort canonicalization → PreNnfAst.
  const preNnf = buildPreNnfAst(deBruijn);

  // Pass 4: implies removal.
  const noImplies = removeImplies(preNnf);

  // Pass 5: NNF.
  const nnf = toNnf(noImplies);

  // Pass 6: AC normalization.
  const canonical = acNormalize(nnf);

  return canonical;
}

/**
 * Serialize and hash a formula to its propertyHash.
 * Returns a self-identifying string of the form
 * `"blake3-512:" + hex(BLAKE3_512(bytes))` (139 chars).
 */
export function propertyHashFromFormula(formula: IrFormula): string {
  const ast = formulaToCanonicalAst(formula);
  const bytes = serializeCanonicalAst(ast);
  return computeCid(bytes);
}

/**
 * Serialize a canonical AST to its propertyHash directly.
 * Useful for testing individual passes.
 */
export function propertyHashFromAst(ast: CanonicalFolAst): string {
  const bytes = serializeCanonicalAst(ast);
  return computeCid(bytes);
}
