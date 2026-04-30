/**
 * AST node fingerprinting for diff-aware principle mining (hard-bug 1).
 *
 * `nodeFingerprint(node)` produces a stable hash that:
 *   - Equals across whitespace-only differences (formatting changes)
 *   - Equals across comment-only differences
 *   - Differs when ANY of the node's identifier names, literal values,
 *     keywords, or structural shape change
 *   - Computes recursively: a parent's fingerprint depends on its children's
 *
 * Used by the harvest pipeline to pair AST nodes between pre-fix and
 * post-fix versions of a file. Same fingerprint at the same source
 * position → "unchanged". Different fingerprint at the same position →
 * "modified". Position absent in one side → "added" or "removed".
 *
 * Design note: git is the canonical diff oracle for ProveKit. Anywhere a
 * diff is needed (lint, harvest, fix-loop, future tools), the caller
 * provides a pair of git refs (HEAD vs working-tree, Bug-N vs Bug-N-fix,
 * pre-bundle vs post-bundle, etc.). This module computes fingerprints on
 * ASTs of the resolved file content; it doesn't care where the content
 * came from.
 *
 * Hash function: 64-bit FNV-1a folded into a hex string. Cheap, no
 * cryptographic strength needed (we're matching, not signing). Collisions
 * are statistically irrelevant at the per-file scale we operate on.
 */

import { Node, SyntaxKind } from "ts-morph";

// ---------------------------------------------------------------------------
// Hash primitive
// ---------------------------------------------------------------------------

/**
 * FNV-1a 32-bit hash, folded to hex string. The 64-bit variant requires
 * BigInt or careful split arithmetic that's easy to get wrong (an earlier
 * draft had carry-propagation bugs that produced systematic collisions).
 * 32-bit FNV-1a has acceptable collision rate at the per-file scale we
 * operate on; we're matching, not signing.
 */
function fnv1a32(input: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

// ---------------------------------------------------------------------------
// Token classification
// ---------------------------------------------------------------------------

/**
 * SyntaxKinds we ignore when computing a fingerprint. These are the
 * variations that should NOT change the hash:
 *   - Trivia (whitespace, line breaks)
 *   - Comments (single-line, multi-line, JSDoc)
 */
const SKIP_KINDS = new Set<number>([
  SyntaxKind.WhitespaceTrivia,
  SyntaxKind.NewLineTrivia,
  SyntaxKind.SingleLineCommentTrivia,
  SyntaxKind.MultiLineCommentTrivia,
  SyntaxKind.JSDoc,
  SyntaxKind.JSDocComment,
]);

/**
 * SyntaxKinds we descend through transparently when computing a
 * fingerprint. SyntaxList is the most common case: in ts-morph, statements
 * inside a Block live as children of an intermediate SyntaxList that
 * holds the comma/semicolon separators. Filtering SyntaxList out
 * altogether (as an earlier draft did) drops the actual statements; the
 * fingerprint of `Block { return a + 1 }` and `Block { return a + 2 }`
 * collapsed to the same Block(OpenBrace, CloseBrace) string. Descending
 * transparently flattens the wrapper while keeping its children.
 */
const TRANSPARENT_KINDS = new Set<number>([
  SyntaxKind.SyntaxList,
]);

function shouldSkip(node: Node): boolean {
  return SKIP_KINDS.has(node.getKind());
}

function isTransparent(node: Node): boolean {
  return TRANSPARENT_KINDS.has(node.getKind());
}

function getEffectiveChildren(node: Node): Node[] {
  const result: Node[] = [];
  for (const c of node.getChildren()) {
    if (shouldSkip(c)) continue;
    if (isTransparent(c)) {
      result.push(...getEffectiveChildren(c));
    } else {
      result.push(c);
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Compute the fingerprint of `node`. Recursive: descends through children,
 * skipping trivia/comment kinds (see SKIP_KINDS above). Leaf nodes hash
 * `<kind>:<text>`; internal nodes hash `<kind>(<child1>,<child2>,...)`.
 *
 * Two nodes with the same fingerprint are "structurally and lexically
 * equal up to whitespace/comment differences." The semantics correspond
 * to what a programmer means by "the same code" when ignoring formatting.
 */
export function nodeFingerprint(node: Node): string {
  return fnv1a32(buildPrint(node));
}

/**
 * Build the string that gets hashed. Exposed for debugging — production
 * callers should use nodeFingerprint() which returns the hex hash.
 */
export function buildPrint(node: Node): string {
  const kind = node.getKind();
  const children = getEffectiveChildren(node);
  if (children.length === 0) {
    // Leaf: include the literal token text. Identifier names, numeric
    // literal values, string literal values, keywords — all ride here.
    const text = node.getText().trim();
    return `${kind}:${text}`;
  }
  const childPrints = children.map(buildPrint).join(",");
  return `${kind}(${childPrints})`;
}
