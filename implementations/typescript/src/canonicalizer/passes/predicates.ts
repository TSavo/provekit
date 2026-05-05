/**
 * Pass 2: predicate canonicalization.
 *
 * Pre-condition: input has de Bruijn indices assigned (pass 1 done),
 *   predicate names may use host-language aliases.
 * Post-condition: predicate names are standard canonical names;
 *   equality/inequality arguments are sorted (smaller hash first);
 *   "constant prefers the right" normalization applied to <, ≤, >, ≥.
 *
 * Alias map (host alias → canonical name):
 *   "!=" | "notEqual" | "ne" | "not-equal" → "≠"
 *   "==" | "eq" | "equal" → "="
 *   "lt" | "lessThan" | "less-than" → "<"
 *   "lte" | "le" | "lessThanOrEqual" | "less-than-or-equal" → "≤"
 *   "gt" | "greaterThan" | "greater-than" → ">"
 *   "gte" | "ge" | "greaterThanOrEqual" | "greater-than-or-equal" → "≥"
 *   "kindOf" | "kind_of" → "kind-of"
 *   "dataFlowsTo" | "data_flows_to" → "data-flows-to"
 *   "postDominates" | "post_dominates" → "post-dominates"
 *   "onPath" | "on_path" → "on-path"
 *   "transitionFromTo" | "transition_from_to" → "transition-from-to"
 *   "true" → "true", "false" → "false"
 *   "∈" | "in" → "member"
 *   "⊆" | "subseteq" → "subset"
 *
 * Equality argument sorting: for "=" and "≠", args are sorted so that
 *   args[0] has the lexicographically smaller structural key. This
 *   makes `=(a,b)` and `=(b,a)` canonicalize identically.
 *
 * "Constants prefer the right" for ordered predicates: if args[0] is
 *   a const and args[1] is not, flip args and invert the predicate:
 *     5 < x → x > 5 becomes x > 5 → x > 5 (kept), no: 5 < x means
 *   args[0]=5(const), args[1]=x(var) → flip to x > 5.
 *   Actually we normalize to: var/non-const on left, const on right.
 *   const < var → var > const. But spec says "a < 5 and 5 > a both
 *   canonicalize to a < 5" (right operand is a constant when one exists).
 *   So: if args[0] is const and args[1] is not → flip + invert predicate.
 *
 * This pass operates on atomic formulas' predicate+args pairs directly.
 * It does NOT recurse into the formula tree — the caller handles recursion.
 */

import type { DeBruijnTerm } from "./deBruijn.js";
import type { CanonicalTerm, CanonicalSort, CanonicalPredicate } from "../ast.js";
import { canonicalizeSort } from "./sorts.js";

// -----------------------------------------------------------------------
// Alias map
// -----------------------------------------------------------------------

const ALIAS_MAP: Record<string, string> = {
  // equality
  "==": "=",
  eq: "=",
  equal: "=",
  // inequality
  "!=": "≠",
  notEqual: "≠",
  "not-equal": "≠",
  ne: "≠",
  // ordering
  lt: "<",
  lessThan: "<",
  "less-than": "<",
  lte: "≤",
  le: "≤",
  lessThanOrEqual: "≤",
  "less-than-or-equal": "≤",
  gt: ">",
  greaterThan: ">",
  "greater-than": ">",
  gte: "≥",
  ge: "≥",
  greaterThanOrEqual: "≥",
  "greater-than-or-equal": "≥",
  // membership
  "∈": "member",
  in: "member",
  "⊆": "subset",
  subseteq: "subset",
  // SAST
  kindOf: "kind-of",
  kind_of: "kind-of",
  dataFlowsTo: "data-flows-to",
  data_flows_to: "data-flows-to",
  dominates: "dominates",
  postDominates: "post-dominates",
  post_dominates: "post-dominates",
  onPath: "on-path",
  on_path: "on-path",
  transitionFromTo: "transition-from-to",
  transition_from_to: "transition-from-to",
};

const FLIP_PREDICATE: Record<string, string> = {
  "<": ">",
  ">": "<",
  "≤": "≥",
  "≥": "≤",
};

// -----------------------------------------------------------------------
// Term canonicalization (without de Bruijn — that's pass 1)
// -----------------------------------------------------------------------

export function canonicalizeTerm(term: DeBruijnTerm): CanonicalTerm {
  switch (term.kind) {
    case "var":
      return {
        kind: "var",
        index: term.deBruijn,
        sort: canonicalizeSort(term.sort),
      };
    case "const":
      return {
        kind: "const",
        sort: canonicalizeSort(term.sort),
        value: normalizeConstValue(term.value),
      };
    case "ctor":
      return {
        kind: "ctor",
        name: term.name,
        args: term.args.map(canonicalizeTerm),
        sort: canonicalizeSort(term.sort),
      };

    case "lambda":
      return {
        kind: "lambda",
        paramSort: canonicalizeSort(term.paramSort),
        body: canonicalizeTerm(term.body),
        sort: canonicalizeSort(term.sort),
      };

    case "let":
      return {
        kind: "let",
        bindings: term.bindings.map((b) => ({
          name: b.name,
          boundTerm: canonicalizeTerm(b.boundTerm),
        })),
        body: canonicalizeTerm(term.body),
        sort: canonicalizeSort(term.sort),
      };
  }
}

function normalizeConstValue(v: unknown): boolean | number | bigint | string | null {
  if (v === null || v === undefined) return null;
  if (typeof v === "boolean") return v;
  if (typeof v === "bigint") return v;
  if (typeof v === "number") {
    // Normalize -0 to 0 to avoid serialization ambiguity.
    return Object.is(v, -0) ? 0 : v;
  }
  if (typeof v === "string") return v;
  // Fallback: stringify; this covers e.g. symbol (shouldn't appear in practice)
  return String(v);
}

// -----------------------------------------------------------------------
// Structural term key for sorting (pure structural, no hashing)
// -----------------------------------------------------------------------

/**
 * Produce a stable structural key for a CanonicalTerm for use in
 * sorting equality/inequality arguments. The key is a JSON-style string
 * that reflects structure without depending on serialization of the full AST.
 */
export function termSortKey(term: CanonicalTerm): string {
  switch (term.kind) {
    case "var":
      return `var:${term.index}:${sortKey(term.sort)}`;
    case "const":
      return `const:${sortKey(term.sort)}:${stringifyConst(term.value)}`;
    case "ctor":
      return `ctor:${term.name}:${term.args.map(termSortKey).join(",")}`;

    case "lambda":
      return `lambda:${sortKey(term.paramSort)}:${termSortKey(term.body)}`;

    case "let":
      return `let:${term.bindings.map((b) => `${b.name}=${termSortKey(b.boundTerm)}`).join(",")}:${termSortKey(term.body)}`;
  }
}

function stringifyConst(value: unknown): string {
  // BigInt values appear in BV constants and out-of-safe-range Int
  // constants; JSON.stringify throws on bigint, so render those as a
  // disambiguated string. Other types go through JSON.stringify.
  if (typeof value === "bigint") return `"bigint:${value.toString()}"`;
  return JSON.stringify(value);
}

function sortKey(sort: CanonicalSort): string {
  switch (sort.kind) {
    case "primitive":
      return `P:${sort.name}`;
    case "bitvec":
      return `BV:${sort.width}`;
    case "set":
      return `S:${sortKey(sort.element)}`;
    case "tuple":
      return `T:${sort.elements.map(sortKey).join(",")}`;
    case "function":
      return `F:${sort.args.map(sortKey).join(",")}:${sortKey(sort.return)}`;
    case "dependent":
      return `D:${sort.name}:${sort.indexVar}:${sortKey(sort.indexSort)}`;
  }
}

// -----------------------------------------------------------------------
// Predicate normalization entry point
// -----------------------------------------------------------------------

export interface NormalizedAtomic {
  /** Canonical predicate name (formerly `predicate`; renamed to align
   * with the v1.1 IR-JSON grammar). */
  name: CanonicalPredicate;
  args: CanonicalTerm[];
}

/**
 * Canonicalize a predicate name and its already-canonicalized arguments.
 * Handles alias resolution, equality sorting, and "constants prefer right".
 */
export function canonicalizePredicate(
  predicate: string,
  args: CanonicalTerm[],
): NormalizedAtomic {
  // Step 1: resolve alias.
  const canonical: string = ALIAS_MAP[predicate] ?? predicate;

  // Step 2: predicate-specific argument normalization.
  if (canonical === "=" || canonical === "≠") {
    // Sort so args[0] has the smaller structural key.
    if (args.length === 2) {
      const [a, b] = args;
      const ak = termSortKey(a);
      const bk = termSortKey(b);
      if (ak > bk) {
        return { name: canonical, args: [b, a] };
      }
    }
    return { name: canonical, args };
  }

  if (canonical === "<" || canonical === "≤" || canonical === ">" || canonical === "≥") {
    // "Constants prefer the right": if args[0] is const and args[1] is not, flip.
    if (args.length === 2) {
      const [a, b] = args;
      if (a.kind === "const" && b.kind !== "const") {
        const flipped = FLIP_PREDICATE[canonical];
        if (flipped) {
          return { name: flipped, args: [b, a] };
        }
      }
    }
    return { name: canonical, args };
  }

  return { name: canonical, args };
}
