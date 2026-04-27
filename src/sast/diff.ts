/**
 * Pre/post AST diff classification for diff-aware principle mining (hard-bug 1).
 *
 * Given two file contents (pre-fix and post-fix), computes a per-node
 * classification of what changed:
 *   - unchanged: a node with the same fingerprint exists at the corresponding
 *     position on both sides
 *   - modified: a pre-side node and a post-side node occupy "the same role"
 *     (same parent-fingerprint, same ordinal, same kind) but have different
 *     fingerprints — i.e., a sub-edit happened inside this region
 *   - added: a post-side node has no corresponding pre-side node
 *   - deleted: a pre-side node has no corresponding post-side node
 *
 * Algorithm (per advisor, 2026-04-27):
 *   1. Pair nodes by fingerprint match (any position). These are unchanged —
 *      whitespace/comment-invariant equality is exactly what we want.
 *   2. For unmatched nodes: pair by (parent-fingerprint, ordinal-among-siblings,
 *      kind). These are modified — the role is preserved, the contents differ.
 *   3. Remainder: pre-side → deleted, post-side → added.
 *
 * Why pair by fingerprint, not source position: any line-shifting edit (the
 * common case — `if (b===0) throw; <new line>` followed by the original code)
 * makes every node below the edit shift position. Position-based pairing
 * would misclassify the entire file as "deleted+added" instead of "unchanged".
 *
 * git is the canonical source of pre/post pairs: callers provide two refs
 * (Bug-N vs Bug-N-fix in mining context, HEAD vs working-tree in lint context,
 * pre-bundle vs post-bundle in fix-loop context) and resolve them to file
 * contents before invoking this module. The diff module doesn't care where
 * the contents came from.
 */

import { Project, type Node, type SourceFile } from "ts-morph";
import { nodeFingerprint } from "./fingerprint.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ChangeKind = "unchanged" | "modified" | "added" | "deleted";

export interface DiffEntry {
  changeKind: ChangeKind;
  /** Pre-side info; null for "added". */
  pre: NodeSnapshot | null;
  /** Post-side info; null for "deleted". */
  post: NodeSnapshot | null;
}

export interface NodeSnapshot {
  fingerprint: string;
  parentFingerprint: string | null;
  /** 0-based index among the parent's effective children (skipping trivia). */
  ordinal: number;
  kindName: string;
  line: number;
  column: number;
  start: number;
  end: number;
  /** First 120 chars of node text, single-line, for human inspection. */
  textPreview: string;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Compute a list of DiffEntry rows describing what changed between
 * `preSource` and `postSource`. Both are parsed as TypeScript by ts-morph
 * with the same in-memory project; `filename` is just a label.
 */
export function computeFileDiff(
  preSource: string,
  postSource: string,
  filename = "file.ts",
): DiffEntry[] {
  const project = new Project({ useInMemoryFileSystem: true });
  const preFile = project.createSourceFile(`pre/${filename}`, preSource);
  const postFile = project.createSourceFile(`post/${filename}`, postSource);

  const preNodes = collectSnapshots(preFile);
  const postNodes = collectSnapshots(postFile);

  return classify(preNodes, postNodes);
}

// ---------------------------------------------------------------------------
// Snapshot collection
// ---------------------------------------------------------------------------

interface InternalSnapshot extends NodeSnapshot {
  // None additional for now; kept for future extension (e.g., parent-of pointer).
}

function collectSnapshots(sf: SourceFile): InternalSnapshot[] {
  const out: InternalSnapshot[] = [];

  function walk(node: Node, parent: Node | null, ordinal: number): void {
    const start = node.getStart();
    const end = node.getEnd();
    const sf2 = node.getSourceFile();
    const pos = sf2.getLineAndColumnAtPos(start);

    const fingerprint = nodeFingerprint(node);
    const parentFingerprint = parent ? nodeFingerprint(parent) : null;

    out.push({
      fingerprint,
      parentFingerprint,
      ordinal,
      kindName: node.getKindName(),
      line: pos.line,
      column: pos.column,
      start,
      end,
      textPreview: previewOf(node.getText()),
    });

    // Effective children: skip trivia/comments, descend through SyntaxList.
    const children = effectiveChildren(node);
    for (let i = 0; i < children.length; i++) {
      walk(children[i]!, node, i);
    }
  }

  walk(sf, null, 0);
  return out;
}

/**
 * Mirror of fingerprint.ts's getEffectiveChildren — duplicated here to avoid
 * exporting an internal from fingerprint.ts. Keep these in sync.
 */
function effectiveChildren(node: Node): Node[] {
  const SKIP = new Set<number>([
    // SyntaxKind values for trivia/comment kinds. Importing SyntaxKind here
    // would couple us tightly; instead we filter by kind name. Cheap.
  ]);
  const result: Node[] = [];
  for (const c of node.getChildren()) {
    const name = c.getKindName();
    if (
      name === "WhitespaceTrivia" ||
      name === "NewLineTrivia" ||
      name === "SingleLineCommentTrivia" ||
      name === "MultiLineCommentTrivia" ||
      name === "JSDoc" ||
      name === "JSDocComment"
    ) {
      continue;
    }
    if (name === "SyntaxList") {
      result.push(...effectiveChildren(c));
    } else {
      result.push(c);
    }
  }
  return result;
}

function previewOf(text: string): string {
  const oneLine = text.replace(/\s+/g, " ").trim();
  return oneLine.length <= 120 ? oneLine : oneLine.slice(0, 117) + "...";
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

function classify(
  preNodes: InternalSnapshot[],
  postNodes: InternalSnapshot[],
): DiffEntry[] {
  const entries: DiffEntry[] = [];

  // Step 1: pair by fingerprint match (greedy, FIFO within each fingerprint).
  // A pre node with fingerprint F pairs with the first available post node
  // with the same F. Subtree-equality up to whitespace/comments → unchanged.
  const postByFp = new Map<string, InternalSnapshot[]>();
  for (const n of postNodes) {
    const list = postByFp.get(n.fingerprint) ?? [];
    list.push(n);
    postByFp.set(n.fingerprint, list);
  }

  const unmatchedPre: InternalSnapshot[] = [];
  const matchedPost = new Set<InternalSnapshot>();

  for (const p of preNodes) {
    const candidates = postByFp.get(p.fingerprint);
    if (candidates && candidates.length > 0) {
      const partner = candidates.shift()!;
      matchedPost.add(partner);
      entries.push({ changeKind: "unchanged", pre: p, post: partner });
    } else {
      unmatchedPre.push(p);
    }
  }

  const unmatchedPost = postNodes.filter((n) => !matchedPost.has(n));

  // Step 2: pair leftover pre↔post by (parent-fingerprint, ordinal, kind).
  // Same role under same parent shape, different content → modified.
  // Index unmatched post by the role key for O(1) lookup.
  const roleKey = (n: InternalSnapshot) =>
    `${n.parentFingerprint ?? "ROOT"}|${n.ordinal}|${n.kindName}`;

  const postByRole = new Map<string, InternalSnapshot[]>();
  for (const n of unmatchedPost) {
    const k = roleKey(n);
    const list = postByRole.get(k) ?? [];
    list.push(n);
    postByRole.set(k, list);
  }

  const matchedPostInStep2 = new Set<InternalSnapshot>();
  const stillUnmatchedPre: InternalSnapshot[] = [];
  for (const p of unmatchedPre) {
    const k = roleKey(p);
    const candidates = postByRole.get(k);
    if (candidates && candidates.length > 0) {
      const partner = candidates.shift()!;
      matchedPostInStep2.add(partner);
      entries.push({ changeKind: "modified", pre: p, post: partner });
    } else {
      stillUnmatchedPre.push(p);
    }
  }

  // Step 3: remainder.
  for (const p of stillUnmatchedPre) {
    entries.push({ changeKind: "deleted", pre: p, post: null });
  }
  for (const n of unmatchedPost) {
    if (matchedPostInStep2.has(n)) continue;
    entries.push({ changeKind: "added", pre: null, post: n });
  }

  return entries;
}

// ---------------------------------------------------------------------------
// Convenience accessors
// ---------------------------------------------------------------------------

export function summarize(entries: DiffEntry[]): {
  unchanged: number;
  modified: number;
  added: number;
  deleted: number;
} {
  const out = { unchanged: 0, modified: 0, added: 0, deleted: 0 };
  for (const e of entries) out[e.changeKind] += 1;
  return out;
}
