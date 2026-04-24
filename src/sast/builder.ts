import { readFileSync } from "fs";
import { createHash } from "crypto";
import { Project } from "ts-morph";
import type { Node } from "ts-morph";
import { eq } from "drizzle-orm";
import type { Db } from "../db/index.js";
import { files, nodes, nodeChildren } from "./schema/index.js";
import { subtreeHash } from "./subtreeHash.js";

export interface SASTBuildResult {
  fileId: number;
  rootNodeId: string;
  nodeCount: number;
  rebuilt: boolean; // false if content_hash matched existing — skipped
}

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
 * Recursively walk all concrete children, inserting nodes + edges.
 * Returns node count inserted (including root).
 */
function walkNode(
  tx: Db,
  fileId: number,
  node: Node,
  parentId: string | null,
  childOrder: number,
  count: { value: number },
): string {
  const start = node.getFullStart();
  const end = node.getEnd();
  const kind = node.getKind();
  const text = node.getFullText();
  const hash = subtreeHash(text);
  const id = nodeId(fileId, kind, start, end, hash);

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
  }).run();

  count.value += 1;

  if (parentId !== null) {
    tx.insert(nodeChildren).values({
      parentId,
      childId: id,
      childOrder,
    }).run();
  }

  const children = node.getChildren();
  for (let i = 0; i < children.length; i++) {
    walkNode(tx, fileId, children[i], id, i, count);
  }

  return id;
}

export function buildSASTForFile(db: Db, filePath: string): SASTBuildResult {
  // Read file bytes and compute content hash
  const bytes = readFileSync(filePath);
  const contentHash = sha256hex(bytes.toString("utf8"));

  // Check for existing row with matching content_hash
  const existing = db.select().from(files).where(eq(files.path, filePath)).get();
  if (existing && existing.contentHash === contentHash) {
    // Count existing nodes for this file
    const existingNodes = db.select().from(nodes).where(eq(nodes.fileId, existing.id)).all();
    // Find root: node that is not a child
    const childIds = new Set(
      db.select({ childId: nodeChildren.childId }).from(nodeChildren).all().map((r) => r.childId),
    );
    const rootNode = existingNodes.find((n) => !childIds.has(n.id));
    return {
      fileId: existing.id,
      rootNodeId: rootNode?.id ?? existingNodes[0]?.id ?? "",
      nodeCount: existingNodes.length,
      rebuilt: false,
    };
  }

  const source = bytes.toString("utf8");

  // Parse via ts-morph (in-memory, no tsconfig coupling)
  const project = new Project({ useInMemoryFileSystem: true });
  const sourceFile = project.createSourceFile(filePath, source);

  let fileId: number;
  let rootNodeId: string;
  const count = { value: 0 };

  db.transaction((tx) => {
    // If there's an existing row with a different hash, delete it (cascades nodes + edges)
    if (existing) {
      tx.delete(files).where(eq(files.id, existing.id)).run();
    }

    const fileRow = tx
      .insert(files)
      .values({ path: filePath, contentHash, parsedAt: Date.now() })
      .returning()
      .get();

    fileId = fileRow.id;

    rootNodeId = walkNode(tx, fileId, sourceFile, null, 0, count);
  });

  return {
    fileId: fileId!,
    rootNodeId: rootNodeId!,
    nodeCount: count.value,
    rebuilt: true,
  };
}
