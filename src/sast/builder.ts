import { readFileSync } from "fs";
import { createHash } from "crypto";
import { Project } from "ts-morph";
import type { Node } from "ts-morph";
import { eq } from "drizzle-orm";
import type { Db } from "../db/index.js";
import { files, nodes, nodeChildren } from "./schema/index.js";
import { subtreeHash } from "./subtreeHash.js";
import { extractAllCapabilities } from "./capabilities/extractor.js";
import { extractDataFlow } from "./dataFlow.js";
import { extractDominance } from "./dominance.js";

export interface SASTBuildResult {
  fileId: number;
  rootNodeId: string;
  nodeCount: number;
  rebuilt: boolean; // false if content_hash matched existing — skipped
}

// Transaction type used inside db.transaction() callbacks
export type SastTx = Parameters<Parameters<Db["transaction"]>[0]>[0];

function sha256hex(str: string): string {
  return createHash("sha256").update(str).digest("hex");
}

/**
 * Compute a stable node id:
 *   sha256(`${fileId}:${kind}:${start}:${end}:${subtreeHash}`).slice(0, 16)
 *
 * Kind is included to disambiguate nodes that share the same span
 * (e.g., SourceFile, SyntaxList, and FunctionDeclaration can all span [0..N]).
 */
function nodeId(fileId: number, kind: number, start: number, end: number, hash: string): string {
  return sha256hex(`${fileId}:${kind}:${start}:${end}:${hash}`).slice(0, 16);
}

/**
 * Iterative DFS walk (pre-order) over all concrete children.
 * Inserts nodes + edges into the transaction.
 * Returns { rootNodeId, count, nodeIdByNode } where nodeIdByNode maps
 * ts-morph Node references to their computed node IDs for capability extractors.
 */
function walkIterative(
  tx: SastTx,
  fileId: number,
  root: Node,
): { rootNodeId: string; count: number; nodeIdByNode: Map<Node, string> } {
  interface Frame {
    node: Node;
    parentId: string | null;
    childOrder: number;
  }

  const stack: Frame[] = [{ node: root, parentId: null, childOrder: 0 }];
  let rootNodeId = "";
  let count = 0;
  const nodeIdByNode = new Map<Node, string>();

  while (stack.length > 0) {
    const { node, parentId, childOrder } = stack.pop()!;

    const start = node.getFullStart();
    const end = node.getEnd();
    const kind = node.getKind();
    const kindName = node.getKindName();
    const text = node.getFullText();
    const hash = subtreeHash(text);
    const id = nodeId(fileId, kind, start, end, hash);

    // Store mapping for capability extractors
    nodeIdByNode.set(node, id);

    // line/col from start position
    const pos = node.getSourceFile().getLineAndColumnAtPos(start);

    tx.insert(nodes).values({
      id,
      fileId,
      sourceStart: start,
      sourceEnd: end,
      sourceLine: pos.line,
      sourceCol: pos.column,
      subtreeHash: hash,
      kind: kindName,
    }).run();

    count += 1;

    if (parentId === null) {
      rootNodeId = id;
    } else {
      tx.insert(nodeChildren).values({
        parentId,
        childId: id,
        childOrder,
      }).run();
    }

    // Push children in reverse so leftmost child is popped first (pre-order)
    const children = node.getChildren();
    for (let i = children.length - 1; i >= 0; i--) {
      stack.push({ node: children[i], parentId: id, childOrder: i });
    }
  }

  return { rootNodeId, count, nodeIdByNode };
}

function buildInternal(db: Db, filePath: string, force: boolean): SASTBuildResult {
  // Read file bytes and compute content hash
  const bytes = readFileSync(filePath);
  const contentHash = sha256hex(bytes.toString("utf8"));

  // Check for existing row with matching content_hash
  const existing = db.select().from(files).where(eq(files.path, filePath)).get();
  if (!force && existing && existing.contentHash === contentHash) {
    // Cache hit: rootNodeId is stored directly on the files row (O(1), file-scoped)
    const nodeCount = db.select().from(nodes).where(eq(nodes.fileId, existing.id)).all().length;
    return {
      fileId: existing.id,
      rootNodeId: existing.rootNodeId,
      nodeCount,
      rebuilt: false,
    };
  }

  const source = bytes.toString("utf8");

  // Parse via ts-morph (in-memory, no tsconfig coupling)
  const project = new Project({ useInMemoryFileSystem: true });
  const sourceFile = project.createSourceFile(filePath, source);

  let fileId: number;
  let rootNodeId: string;
  let nodeCount: number;

  db.transaction((tx) => {
    // If there's an existing row (different hash, or force=true), delete it (cascades nodes + edges)
    if (existing) {
      tx.delete(files).where(eq(files.id, existing.id)).run();
    }

    const fileRow = tx
      .insert(files)
      .values({ path: filePath, contentHash, parsedAt: Date.now() })
      .returning()
      .get();

    fileId = fileRow.id;

    const { rootNodeId: rootId, count, nodeIdByNode } = walkIterative(tx, fileId, sourceFile);
    rootNodeId = rootId;
    nodeCount = count;

    // Store root node id on the files row for O(1) cache-hit lookup
    tx.update(files).set({ rootNodeId }).where(eq(files.id, fileId)).run();

    // Populate capability tables
    extractAllCapabilities(tx, sourceFile, nodeIdByNode);

    // Populate data-flow tables (def-use edges + transitive closure)
    extractDataFlow(tx, fileId, sourceFile, nodeIdByNode);

    // Populate dominance + post-dominance tables (CFG-based, per-function)
    extractDominance(tx, sourceFile, nodeIdByNode);
  });

  return {
    fileId: fileId!,
    rootNodeId: rootNodeId!,
    nodeCount: nodeCount!,
    rebuilt: true,
  };
}

export function buildSASTForFile(db: Db, filePath: string): SASTBuildResult {
  return buildInternal(db, filePath, false);
}

/**
 * Force a full re-parse and re-index of a file regardless of whether its
 * content_hash has changed. Used by the fix loop after overlay/mutation
 * edits that might leave the file bytes equal but the in-memory AST stale,
 * and as a clean entry point for "I know this needs to be re-indexed."
 *
 * Behavior: deletes the existing files row (cascading nodes + edges +
 * capability rows + data_flow + dominance via FK) and inserts fresh.
 * Returns rebuilt: true unconditionally.
 */
export function reindexFile(db: Db, filePath: string): SASTBuildResult {
  return buildInternal(db, filePath, true);
}
