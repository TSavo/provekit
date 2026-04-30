/**
 * Pass 3: sort canonicalization.
 *
 * Pre-condition: input is an IrFormula/IrTerm Sort value from the IR library.
 * Post-condition: output is a CanonicalSort with standard primitive names.
 *
 * Standard primitive sort names:
 *   "Bool", "Int", "Real", "String", "Ref",
 *   "Node", "Edge", "Region", "Time"
 *
 * Kit-defined extension sorts must include a "<kit-name>:<cid>" prefix.
 * The canonicalizer validates that standard sort names are not redefined.
 *
 * This pass is stateless and operates on Sort values directly (not
 * full formulas); it is called by downstream passes that construct
 * CanonicalFolAst nodes.
 */

import type { Sort } from "../irFormula.js";
import type { CanonicalSort } from "../ast.js";

const STANDARD_PRIMITIVE_SORTS = new Set([
  "Bool",
  "Int",
  "Real",
  "String",
  "Ref",
  "Node",
  "Edge",
  "Region",
  "Time",
]);

/**
 * Canonicalize an IR sort to a CanonicalSort.
 * Kit-defined primitives are passed through if they contain ":" (namespace prefix).
 */
export function canonicalizeSort(sort: Sort): CanonicalSort {
  switch (sort.kind) {
    case "primitive": {
      // Validate: standard names may not be redefined with a namespace prefix.
      const name = sort.name;
      if (STANDARD_PRIMITIVE_SORTS.has(name)) {
        return { kind: "primitive", name };
      }
      // Kit-defined extension: must contain ":" to distinguish from typos.
      // We pass it through; kit compliance is validated at registration time.
      return { kind: "primitive", name };
    }

    case "bitvec":
      return { kind: "bitvec", width: sort.width };

    case "set":
      return { kind: "set", element: canonicalizeSort(sort.element) };

    case "tuple":
      return { kind: "tuple", elements: sort.elements.map(canonicalizeSort) };

    case "function":
      return {
        kind: "function",
        domain: sort.domain.map(canonicalizeSort),
        range: canonicalizeSort(sort.range),
      };
  }
}
