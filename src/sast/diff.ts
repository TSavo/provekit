/**
 * Pre/post AST diff classification for diff-aware principle mining (hard-bug 1).
 *
 * Given two file contents (pre-fix and post-fix), computes a per-node
 * classification of what changed:
 *   - unchanged: a node with the same fingerprint exists somewhere in
 *     the other tree — its subtree is preserved (whitespace/comments
 *     ignored)
 *   - modified: paired structurally to a counterpart in the other tree
 *     by the top-down ordinal walk; same role, different content
 *   - added: a post-side node with no pre-side counterpart
 *   - deleted: a pre-side node with no post-side counterpart
 *
 * Hybrid algorithm (2026-04-27, v2 after the ancestor-cascade false
 * positive surfaced by the OR-unchanged-with-unrelated-fix negative
 * test):
 *
 *   Step 1 (flat fingerprint pair):
 *     For every pre node, if the same fingerprint exists somewhere on
 *     the post side and isn't already taken, pair them as `unchanged`
 *     and mark the entire subtree on both sides as matched. This catches
 *     subtrees that survived the fix even if they were rewrapped (the
 *     OR-chain extension shape's load-bearing case).
 *
 *   Step 2 (top-down structural pair):
 *     Starting from SourceFile↔SourceFile (always paired regardless of
 *     fingerprint), recurse into children. Skip children already matched
 *     in step 1. Pair the remaining children by their position among
 *     the unmatched children, preferring matching `kindName`. Each
 *     such pair is `modified`; recurse into them. Leftovers are
 *     `deleted` (pre-only) or `added` (post-only) — and crucially their
 *     entire subtree carries that label, not just the root.
 *
 * Why hybrid: pure flat (v1) cascaded ancestor changes — any unrelated
 * edit anywhere in the file made the whole ancestor chain "deleted +
 * added", whose post-side ranges enclosed unrelated unchanged subtrees
 * and produced false positives in `was_replaced_by_addition`. Pure
 * top-down (no flat) misses the load-bearing OR-chain reachability:
 * pre `(a||b)` becomes a deep grandchild of post `((a||b)||c)`, so the
 * top-level recursion never tries to pair them. Combining both: the
 * flat pass catches subtree preservation; the top-down pass classifies
 * everything else honestly.
 *
 * git is the canonical source of pre/post pairs: callers provide two
 * refs (Bug-N vs Bug-N-fix in mining context, HEAD vs working-tree in
 * lint context, pre-bundle vs post-bundle in fix-loop context) and
 * resolve them to file contents before invoking this module.
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
  return classify(preFile, postFile);
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

interface NodeRecord {
  node: Node;
  parent: Node | null;
  ordinal: number;
  kindName: string;
  fingerprint: string;
  start: number;
  end: number;
  line: number;
  column: number;
  textPreview: string;
}

function effectiveChildren(node: Node): Node[] {
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

function buildIndex(sf: SourceFile): {
  records: Map<Node, NodeRecord>;
  byFingerprint: Map<string, Node[]>;
  root: Node;
} {
  const records = new Map<Node, NodeRecord>();
  const byFingerprint = new Map<string, Node[]>();

  function walk(node: Node, parent: Node | null, ordinal: number): void {
    const start = node.getFullStart();
    const end = node.getEnd();
    const sf2 = node.getSourceFile();
    const pos = sf2.getLineAndColumnAtPos(node.getStart());
    const fp = nodeFingerprint(node);
    records.set(node, {
      node,
      parent,
      ordinal,
      kindName: node.getKindName(),
      fingerprint: fp,
      start,
      end,
      line: pos.line,
      column: pos.column,
      textPreview: previewOf(node.getText()),
    });
    const list = byFingerprint.get(fp) ?? [];
    list.push(node);
    byFingerprint.set(fp, list);

    const children = effectiveChildren(node);
    for (let i = 0; i < children.length; i++) {
      walk(children[i]!, node, i);
    }
  }
  walk(sf, null, 0);
  return { records, byFingerprint, root: sf };
}

function snapshotOf(rec: NodeRecord, parentFp: string | null): NodeSnapshot {
  return {
    fingerprint: rec.fingerprint,
    parentFingerprint: parentFp,
    ordinal: rec.ordinal,
    kindName: rec.kindName,
    line: rec.line,
    column: rec.column,
    start: rec.start,
    end: rec.end,
    textPreview: rec.textPreview,
  };
}

function classify(preFile: SourceFile, postFile: SourceFile): DiffEntry[] {
  const pre = buildIndex(preFile);
  const post = buildIndex(postFile);
  const matchedPre = new Set<Node>();
  const matchedPost = new Set<Node>();
  const entries: DiffEntry[] = [];

  // Step 1: flat fingerprint pairing. For each pre node not already
  // matched (its ancestor's subtree may have already been claimed),
  // pair with the first available same-fingerprint post node and
  // recursively mark the whole subtree as unchanged on both sides.
  function markSubtreeMatched(p: Node, q: Node): void {
    matchedPre.add(p);
    matchedPost.add(q);
    const pc = effectiveChildren(p);
    const qc = effectiveChildren(q);
    // Same fingerprint ⇒ structurally identical effective-children sequence.
    for (let i = 0; i < Math.min(pc.length, qc.length); i++) {
      markSubtreeMatched(pc[i]!, qc[i]!);
    }
  }

  function emitSubtreeUnchanged(p: Node, q: Node): void {
    const preRec = pre.records.get(p)!;
    const postRec = post.records.get(q)!;
    entries.push({
      changeKind: "unchanged",
      pre: snapshotOf(preRec, preRec.parent ? pre.records.get(preRec.parent)!.fingerprint : null),
      post: snapshotOf(postRec, postRec.parent ? post.records.get(postRec.parent)!.fingerprint : null),
    });
    const pc = effectiveChildren(p);
    const qc = effectiveChildren(q);
    for (let i = 0; i < Math.min(pc.length, qc.length); i++) {
      emitSubtreeUnchanged(pc[i]!, qc[i]!);
    }
  }

  // Walk pre in pre-order; for each unmatched node, try to pair by fingerprint.
  function flatPair(p: Node): void {
    if (matchedPre.has(p)) return;
    const candidates = post.byFingerprint.get(pre.records.get(p)!.fingerprint);
    if (candidates) {
      for (const q of candidates) {
        if (matchedPost.has(q)) continue;
        // Found a match. Mark subtree, emit unchanged entries.
        markSubtreeMatched(p, q);
        emitSubtreeUnchanged(p, q);
        return; // children are now matched; don't recurse manually
      }
    }
    // No match; recurse into children to look for deeper matches.
    for (const c of effectiveChildren(p)) flatPair(c);
  }
  flatPair(pre.root);

  // Step 2: top-down structural pair from the SourceFile pair. SourceFile
  // is always considered paired (regardless of whether step 1 caught it).
  // For each modified pair, walk the unmatched children and pair them by
  // ordinal-among-unmatched + kind. Recurse into modified pairs.
  function emitDeleted(p: Node): void {
    const preRec = pre.records.get(p)!;
    entries.push({
      changeKind: "deleted",
      pre: snapshotOf(preRec, preRec.parent ? pre.records.get(preRec.parent)!.fingerprint : null),
      post: null,
    });
    matchedPre.add(p);
    for (const c of effectiveChildren(p)) {
      if (!matchedPre.has(c)) emitDeleted(c);
    }
  }
  function emitAdded(q: Node): void {
    const postRec = post.records.get(q)!;
    entries.push({
      changeKind: "added",
      pre: null,
      post: snapshotOf(postRec, postRec.parent ? post.records.get(postRec.parent)!.fingerprint : null),
    });
    matchedPost.add(q);
    for (const c of effectiveChildren(q)) {
      if (!matchedPost.has(c)) emitAdded(c);
    }
  }
  function emitModified(p: Node, q: Node): void {
    const preRec = pre.records.get(p)!;
    const postRec = post.records.get(q)!;
    entries.push({
      changeKind: "modified",
      pre: snapshotOf(preRec, preRec.parent ? pre.records.get(preRec.parent)!.fingerprint : null),
      post: snapshotOf(postRec, postRec.parent ? post.records.get(postRec.parent)!.fingerprint : null),
    });
    matchedPre.add(p);
    matchedPost.add(q);
  }

  function recurse(p: Node, q: Node): void {
    // If the pair was already matched in step 1, the subtree is unchanged.
    // Skip — entries are already emitted.
    if (matchedPre.has(p) && matchedPost.has(q) && pre.records.get(p)!.fingerprint === post.records.get(q)!.fingerprint) {
      return;
    }
    // Modified pair. Emit (if not already emitted by being a step-1 match,
    // but step 1 matches have equal fingerprints, so this branch is reached
    // only when they differ).
    emitModified(p, q);

    const pc = effectiveChildren(p).filter((c) => !matchedPre.has(c));
    const qc = effectiveChildren(q).filter((c) => !matchedPost.has(c));

    // Pair by position among unmatched children. Greedy with kind-aware
    // skip: if at index i the kinds differ, look ahead for the first
    // kind-matching position on either side and skip the unmatched.
    let i = 0, j = 0;
    while (i < pc.length && j < qc.length) {
      const pn = pc[i]!;
      const qn = qc[j]!;
      const pk = pn.getKindName();
      const qk = qn.getKindName();
      if (pk === qk) {
        recurse(pn, qn);
        i++; j++;
        continue;
      }
      // Kinds differ. Decide which to skip by scanning ahead.
      const qIdx = qc.findIndex((cc, ci) => ci >= j && cc.getKindName() === pk);
      const pIdx = pc.findIndex((cc, ci) => ci >= i && cc.getKindName() === qk);
      const qDist = qIdx === -1 ? Infinity : qIdx - j;
      const pDist = pIdx === -1 ? Infinity : pIdx - i;
      if (qDist <= pDist) {
        // pn has a match further ahead in q; emit q[j] as added, advance j.
        emitAdded(qn);
        j++;
      } else {
        emitDeleted(pn);
        i++;
      }
    }
    while (i < pc.length) emitDeleted(pc[i++]!);
    while (j < qc.length) emitAdded(qc[j++]!);
  }

  // SourceFile root: if step 1 matched it already, recurse will see
  // them as unchanged and return immediately. Otherwise it'll mark
  // SourceFile as modified and walk down.
  recurse(pre.root, post.root);

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
