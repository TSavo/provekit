/**
 * Pass 6: AC-normalization for `and`/`or`.
 *
 * Pre-condition: input is in NNF (pass 5 done). No `implies` nodes.
 *   `not` wraps only atomics.
 * Post-condition:
 *   1. Flatten: and(and(p,q),r) → and(p,q,r). Recursively.
 *   2. Sort: operands sorted by their canonical bytes (serialize each
 *      subtree to a JSON-structural key, sort lexicographically).
 *   3. Deduplicate: remove adjacent equal operands after sorting.
 *   4. Identity removal:
 *      and(true, p) → p   and(false, ...) → false
 *      or(false, p) → p   or(true, ...)  → true
 *   5. Empty handling: and() → true  or() → false
 *
 * Sorting is done by a structural key derived from the node's JSON-like
 * structure (same key used for term sorting in pass 2, extended to full
 * AST nodes). This avoids a dependency on pass 7 (serialization) while
 * producing a deterministic, stable order.
 */

import type { CanonicalFolAst, CanonicalTerm, CanonicalSort } from "../ast.js";

// -----------------------------------------------------------------------
// Structural key for AC sorting
// -----------------------------------------------------------------------

/** Stable, deterministic structural key for a CanonicalFolAst. */
export function astSortKey(ast: CanonicalFolAst): string {
  switch (ast.kind) {
    case "forall":
      return `Q:A:${sortKeySort(ast.sort)}:${astSortKey(ast.body)}`;
    case "exists":
      return `Q:E:${sortKeySort(ast.sort)}:${astSortKey(ast.body)}`;
    case "and":
      return `C:&:${ast.operands.map(astSortKey).join("|")}`;
    case "or":
      return `C:|:${ast.operands.map(astSortKey).join("|")}`;
    case "not":
      return `C:!:${astSortKey(ast.operands[0])}`;
    case "atomic":
      return `A:${ast.name}:${ast.args.map(termKey).join(",")}`;
  }
}

function termKey(t: CanonicalTerm): string {
  switch (t.kind) {
    case "var":
      return `v${t.index}:${sortKeySort(t.sort)}`;
    case "const":
      return `c:${sortKeySort(t.sort)}:${stringifyConstValue(t.value)}`;
    case "ctor":
      return `k:${t.name}:${t.args.map(termKey).join(",")}`;
  }
}

function stringifyConstValue(value: unknown): string {
  if (typeof value === "bigint") return `"bigint:${value.toString()}"`;
  return JSON.stringify(value);
}

function sortKeySort(s: CanonicalSort): string {
  switch (s.kind) {
    case "primitive":
      return `P:${s.name}`;
    case "bitvec":
      return `BV:${s.width}`;
    case "set":
      return `S:${sortKeySort(s.element)}`;
    case "tuple":
      return `T:${s.elements.map(sortKeySort).join(",")}`;
    case "function":
      return `F:${s.domain.map(sortKeySort).join(",")}:${sortKeySort(s.range)}`;
  }
}

// -----------------------------------------------------------------------
// Identity constants as CanonicalFolAst
// -----------------------------------------------------------------------

const TRUE_ATOMIC: CanonicalFolAst = { kind: "atomic", name: "true", args: [] };
const FALSE_ATOMIC: CanonicalFolAst = { kind: "atomic", name: "false", args: [] };

function isTrue(ast: CanonicalFolAst): boolean {
  return ast.kind === "atomic" && ast.name === "true" && ast.args.length === 0;
}

function isFalse(ast: CanonicalFolAst): boolean {
  return ast.kind === "atomic" && ast.name === "false" && ast.args.length === 0;
}

// -----------------------------------------------------------------------
// AC normalization
// -----------------------------------------------------------------------

/**
 * AC-normalize the entire formula tree bottom-up.
 * Recursion ensures children are normalized before the parent.
 */
export function acNormalize(ast: CanonicalFolAst): CanonicalFolAst {
  switch (ast.kind) {
    case "forall":
    case "exists":
      return { kind: ast.kind, sort: ast.sort, body: acNormalize(ast.body) };

    case "not":
      return { kind: "not", operands: [acNormalize(ast.operands[0])] };

    case "atomic":
      return ast;

    case "and": {
      // Recursively normalize children first.
      const children = ast.operands.map(acNormalize);
      return normalizeAnd(children);
    }

    case "or": {
      const children = ast.operands.map(acNormalize);
      return normalizeOr(children);
    }
  }
}

function normalizeAnd(operands: CanonicalFolAst[]): CanonicalFolAst {
  // 1. Flatten nested `and`.
  const flat: CanonicalFolAst[] = [];
  for (const op of operands) {
    if (op.kind === "and") {
      flat.push(...op.operands);
    } else {
      flat.push(op);
    }
  }

  // 4. Identity removal: false absorbs; true is identity.
  if (flat.some(isFalse)) return FALSE_ATOMIC;
  const noTrue = flat.filter((op) => !isTrue(op));

  // 5. Empty: and() → true
  if (noTrue.length === 0) return TRUE_ATOMIC;
  if (noTrue.length === 1) return noTrue[0];

  // 2. Sort by structural key.
  const sorted = [...noTrue].sort((a, b) => {
    const ka = astSortKey(a);
    const kb = astSortKey(b);
    return ka < kb ? -1 : ka > kb ? 1 : 0;
  });

  // 3. Deduplicate adjacent equal entries.
  const deduped: CanonicalFolAst[] = [sorted[0]];
  for (let i = 1; i < sorted.length; i++) {
    if (astSortKey(sorted[i]) !== astSortKey(sorted[i - 1])) {
      deduped.push(sorted[i]);
    }
  }

  if (deduped.length === 1) return deduped[0]!;
  return { kind: "and", operands: deduped };
}

function normalizeOr(operands: CanonicalFolAst[]): CanonicalFolAst {
  // 1. Flatten nested `or`.
  const flat: CanonicalFolAst[] = [];
  for (const op of operands) {
    if (op.kind === "or") {
      flat.push(...op.operands);
    } else {
      flat.push(op);
    }
  }

  // 4. Identity removal: true absorbs; false is identity.
  if (flat.some(isTrue)) return TRUE_ATOMIC;
  const noFalse = flat.filter((op) => !isFalse(op));

  // 5. Empty: or() → false
  if (noFalse.length === 0) return FALSE_ATOMIC;
  if (noFalse.length === 1) return noFalse[0];

  // 2. Sort by structural key.
  const sorted = [...noFalse].sort((a, b) => {
    const ka = astSortKey(a);
    const kb = astSortKey(b);
    return ka < kb ? -1 : ka > kb ? 1 : 0;
  });

  // 3. Deduplicate adjacent equal entries.
  const deduped: CanonicalFolAst[] = [sorted[0]];
  for (let i = 1; i < sorted.length; i++) {
    if (astSortKey(sorted[i]) !== astSortKey(sorted[i - 1])) {
      deduped.push(sorted[i]);
    }
  }

  if (deduped.length === 1) return deduped[0]!;
  return { kind: "or", operands: deduped };
}
