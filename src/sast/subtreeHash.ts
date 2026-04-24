import { createHash } from "crypto";

/**
 * Returns sha256(str) as hex. No canonicalization — just the raw text hash.
 * Matches nodes.subtree_hash = sha256(node.getText()).
 */
export function subtreeHash(str: string): string {
  return createHash("sha256").update(str).digest("hex");
}
