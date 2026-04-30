/**
 * Reference implementations of the channel-2 integration interfaces.
 *
 * Each class is a thin wrapper that delegates to the existing module-scoped
 * code. The wrappers exist so the public surface has a documented, named
 * shape an integrator can use as a default — and so future implementations
 * can be slotted in without changing the orchestrator.
 *
 * If a wrapper appears trivial, that is intentional. The wrapper layer is
 * the seam; the existing modules are the implementation. When a second
 * implementation arrives, the wrappers absorb the dispatch and the seam
 * stays put.
 */

import { execFileSync } from "child_process";
import { readFileSync, writeFileSync } from "fs";
import { join, relative } from "path";

import {
  buildSASTForFile,
  reindexFile,
  type SASTBuildResult,
} from "../../sast/builder.js";
import { openDb, type Db } from "../../db/index.js";
import {
  applyPatchToOverlay,
  closeOverlay,
} from "../../fix/overlay.js";
import { openOverlay } from "../../fix/stages/openOverlay.js";
import {
  detectTestRunner,
  getTestRunner,
} from "../../fix/testRunners/index.js";

import type {
  Parser,
  AstGraph,
  AstNode,
  AstEdge,
  Substrate,
  NodeId,
  SubstrateQueryByPosition,
  Sandbox,
  RunResult,
  BuildTestRunner,
  TestTarget,
  SourcePatternScanner,
  PatternQuery,
  ScanContext,
  PatchApplicator,
  ApplyPatchResult,
} from "../interfaces.js";

import type {
  OverlayHandle,
  CodePatch,
  BugLocus,
} from "../../fix/types.js";

import { Project, type Node } from "ts-morph";
import { eq, and, lte, gte } from "drizzle-orm";
import { nodes, files } from "../../sast/schema/index.js";

// ---------------------------------------------------------------------------
// ReferenceTsMorphParser
// ---------------------------------------------------------------------------

/**
 * Parses TypeScript / JavaScript via ts-morph and returns a typed AstGraph.
 *
 * This wrapper does NOT use the SQLite substrate; it produces a pure in-memory
 * graph. When stages need substrate-backed queries (capability tables,
 * dataflow, dominance), they go through the existing builder + Db path
 * directly — that's the SubstrateImpl below. Parser is the cross-language
 * boundary.
 */
export class ReferenceTsMorphParser implements Parser {
  readonly name = "ts-morph";

  parse(source: string, filePath: string): AstGraph {
    const project = new Project({ useInMemoryFileSystem: true });
    const sourceFile = project.createSourceFile(filePath, source);

    const astNodes: AstNode[] = [];
    const astEdges: AstEdge[] = [];

    interface Frame {
      node: Node;
      parentId: string | null;
      childOrder: number;
    }

    const stack: Frame[] = [{ node: sourceFile, parentId: null, childOrder: 0 }];
    let counter = 0;
    let rootId = "";

    while (stack.length > 0) {
      const { node, parentId, childOrder } = stack.pop()!;
      const id = `n${counter++}`;
      const start = node.getFullStart();
      const end = node.getEnd();
      const pos = node.getSourceFile().getLineAndColumnAtPos(start);

      astNodes.push({
        id,
        kind: node.getKindName(),
        start,
        end,
        line: pos.line,
        column: pos.column,
      });

      if (parentId === null) {
        rootId = id;
      } else {
        astEdges.push({ parentId, childId: id, childOrder });
      }

      const children = node.getChildren();
      for (let i = children.length - 1; i >= 0; i--) {
        stack.push({ node: children[i], parentId: id, childOrder: i });
      }
    }

    return {
      filePath,
      rootId,
      nodes: astNodes,
      edges: astEdges,
    };
  }
}

// ---------------------------------------------------------------------------
// ReferenceSqliteSubstrate
// ---------------------------------------------------------------------------

/**
 * Wraps the existing SQLite substrate. The substrate is populated by
 * `buildSASTForFile`; the methods here cover the v1 contract:
 *   - addNode/addEdge: single-row inserts (used by tests + bespoke
 *     integrators bypassing buildSASTForFile)
 *   - query: position lookup, returns the smallest node spanning the
 *     position (matching what locate.ts does today via SQL).
 */
export class ReferenceSqliteSubstrate implements Substrate {
  readonly name = "sqlite";
  constructor(private readonly db: Db) {}

  /**
   * Convenience: populate the substrate from a file on disk via the existing
   * builder. Returns the SAST build result.
   */
  ingestFile(filePath: string): SASTBuildResult {
    return buildSASTForFile(this.db, filePath);
  }

  reindexFile(filePath: string): SASTBuildResult {
    return reindexFile(this.db, filePath);
  }

  addNode(node: AstNode): NodeId {
    // Look up file row by absolute path; create a stub file row when missing.
    // The reference path is for integrators not going through buildSASTForFile.
    const filePath = (node as AstNode & { filePath?: string }).filePath ?? "<unknown>";
    let fileRow = this.db.select().from(files).where(eq(files.path, filePath)).get();
    if (!fileRow) {
      fileRow = this.db
        .insert(files)
        .values({ path: filePath, contentHash: "manual", parsedAt: Date.now() })
        .returning()
        .get();
    }
    this.db
      .insert(nodes)
      .values({
        id: node.id,
        fileId: fileRow.id,
        sourceStart: node.start,
        sourceEnd: node.end,
        sourceLine: node.line,
        sourceCol: node.column,
        subtreeHash: "manual",
        kind: node.kind,
      })
      .run();
    return node.id;
  }

  addEdge(from: NodeId, to: NodeId, slot: string): void {
    // The reference substrate models edges via node_children with a
    // child_order int. We use the slot string as the textual order key when
    // it is numeric, else 0 (the slot label is preserved on the parent's
    // chain by ordering of inserts in v1).
    const childOrder = Number.isFinite(Number(slot)) ? Number(slot) : 0;
    // node_children import is intentionally lazy-loaded to keep this file
    // free of additional schema imports at module load.
    const { nodeChildren } = require("../../sast/schema/index.js");
    this.db.insert(nodeChildren).values({ parentId: from, childId: to, childOrder }).run();
  }

  query(q: SubstrateQueryByPosition): NodeId | null {
    const fileRow = this.db.select().from(files).where(eq(files.path, q.filePath)).get();
    if (!fileRow) return null;
    // Smallest node whose span covers the line.
    const candidates = this.db
      .select()
      .from(nodes)
      .where(
        and(eq(nodes.fileId, fileRow.id), lte(nodes.sourceLine, q.line)),
      )
      .all();
    if (candidates.length === 0) return null;
    // Prefer the most specific (smallest span) covering the line.
    let best: typeof candidates[number] | null = null;
    for (const c of candidates) {
      if (c.sourceLine !== q.line) continue;
      if (!best || c.sourceEnd - c.sourceStart < best.sourceEnd - best.sourceStart) {
        best = c;
      }
    }
    return best?.id ?? null;
  }
}

// ---------------------------------------------------------------------------
// ReferenceGitWorktreeSandbox
// ---------------------------------------------------------------------------

/**
 * Wraps the existing git-worktree sandbox.
 *
 * The legacy `openOverlay` is locus-driven (it expects a BugLocus and
 * pre-indexes the locus file). For the v1 Sandbox interface we accept an
 * optional `locusFile` to drive the same path; without one we fabricate a
 * BugLocus pointing at the project root so `openOverlay` can still resolve
 * the repo and create the worktree.
 */
export class ReferenceGitWorktreeSandbox implements Sandbox {
  readonly name = "git-worktree";

  async open(projectRoot: string, opts?: { locusFile?: string }): Promise<OverlayHandle> {
    const locus: BugLocus = {
      file: opts?.locusFile ?? join(projectRoot, "package.json"),
      line: 1,
      confidence: 1,
      primaryNode: "root",
      containingFunction: "root",
      relatedFunctions: [],
      dataFlowAncestors: [],
      dataFlowDescendants: [],
      dominanceRegion: [],
      postDominanceRegion: [],
    };
    // db is unused by openOverlay's worktree logic itself; pass the sast db
    // it will create. openOverlay opens a fresh scratch sast db internally.
    const stubDbHandle: Db = openDb(":memory:");
    return openOverlay({ locus, db: stubDbHandle });
  }

  async apply(handle: OverlayHandle, patch: CodePatch): Promise<void> {
    applyPatchToOverlay(handle, patch);
  }

  runCommand(handle: OverlayHandle, cmd: string[]): RunResult {
    if (cmd.length === 0) {
      return { exitCode: 0, stdout: "", stderr: "" };
    }
    try {
      const stdout = execFileSync(cmd[0], cmd.slice(1), {
        cwd: handle.worktreePath,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      });
      return { exitCode: 0, stdout, stderr: "" };
    } catch (err: unknown) {
      const e = err as { status?: number; stdout?: string; stderr?: string; message?: string };
      return {
        exitCode: e.status ?? 1,
        stdout: typeof e.stdout === "string" ? e.stdout : "",
        stderr: typeof e.stderr === "string" ? e.stderr : (e.message ?? "unknown error"),
      };
    }
  }

  async close(handle: OverlayHandle): Promise<void> {
    await closeOverlay(handle);
  }
}

// ---------------------------------------------------------------------------
// ReferenceVitestPnpmRunner
// ---------------------------------------------------------------------------

/**
 * Wraps the existing test-runner registry. Auto-detects via the registry's
 * descriptor.detect() ordering; vitest scores 1.0 when the project has a
 * vitest.config; jest/mocha/node-test descriptors fall through.
 */
export class ReferenceVitestPnpmRunner implements BuildTestRunner {
  readonly name = "vitest+registry";

  run(sandbox: OverlayHandle, target: TestTarget): RunResult {
    const desc = target.runnerName
      ? getTestRunner(target.runnerName) ?? detectTestRunner(sandbox.worktreePath)
      : detectTestRunner(sandbox.worktreePath);
    if (!desc || desc.name === "none") {
      return {
        exitCode: 0,
        stdout: "",
        stderr: `[reference-runner] no test runner detected for ${sandbox.worktreePath}`,
      };
    }
    let bin: string;
    try {
      bin = desc.resolveRunnerBinary(sandbox.worktreePath);
    } catch (err) {
      return {
        exitCode: 1,
        stdout: "",
        stderr: err instanceof Error ? err.message : String(err),
      };
    }
    // Full-suite invocation skips the test-file argument; we approximate by
    // passing an empty filename to the runner. Most runners (vitest, jest)
    // treat an empty positional as "run everything" when paired with a
    // permissive include glob.
    const argv = desc.invocation(target.testFilePath ?? "");
    try {
      const stdout = execFileSync(bin, argv, {
        cwd: sandbox.worktreePath,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      });
      return { exitCode: 0, stdout, stderr: "" };
    } catch (err: unknown) {
      const e = err as { status?: number; stdout?: string; stderr?: string; message?: string };
      return {
        exitCode: e.status ?? 1,
        stdout: typeof e.stdout === "string" ? e.stdout : "",
        stderr: typeof e.stderr === "string" ? e.stderr : (e.message ?? ""),
      };
    }
  }
}

// ---------------------------------------------------------------------------
// ReferenceTypeScriptPatternScanner
// ---------------------------------------------------------------------------

/**
 * The asc(/desc( regex extracted from pathChecker, exposed through the
 * SourcePatternScanner contract. The pathChecker itself continues to call
 * the regex inline (Part A of the architecture stays untouched); this
 * wrapper is the explicit, swappable cross-language seam.
 *
 * v1 supports two patterns:
 *   - "order/asc-or-desc": matches /\basc\(/ or /\bdesc\(/ within the line
 *     window. Returns true when at least one is present.
 *   - "order/asc": matches /\basc\(/ unambiguously (i.e., no desc in window).
 *   - "order/desc": matches /\bdesc\(/ unambiguously.
 */
export class ReferenceTypeScriptPatternScanner implements SourcePatternScanner {
  readonly name = "ts-source-regex";

  scan(ctx: ScanContext, query: PatternQuery): boolean {
    const lines = ctx.source.split(/\r?\n/);
    const idx = ctx.line - 1;
    if (idx < 0 || idx >= lines.length) return false;
    const win = ctx.windowLines ?? 2;
    const start = Math.max(0, idx - win);
    const stop = Math.min(lines.length, idx + win + 1);
    const window = lines.slice(start, stop).join("\n");
    const hasAsc = /\basc\(/.test(window);
    const hasDesc = /\bdesc\(/.test(window);
    switch (query.kind) {
      case "order/asc-or-desc":
        return hasAsc || hasDesc;
      case "order/asc":
        return hasAsc && !hasDesc;
      case "order/desc":
        return hasDesc && !hasAsc;
      default:
        return false;
    }
  }
}

// ---------------------------------------------------------------------------
// ReferenceGitApplyApplicator
// ---------------------------------------------------------------------------

/**
 * Writes the patch's file edits into the sandbox tree, then `git add -N`
 * for any new files so subsequent `git diff` includes them. The legacy
 * applyPatchToOverlay handles the writes; this wrapper layers the
 * git-add-N step on top so the resulting diff is a complete record of
 * the change.
 */
export class ReferenceGitApplyApplicator implements PatchApplicator {
  readonly name = "git-apply";

  async apply(sandbox: OverlayHandle, patch: CodePatch): Promise<ApplyPatchResult> {
    const written: string[] = [];
    try {
      for (const edit of patch.fileEdits) {
        const abs = join(sandbox.worktreePath, edit.file);
        writeFileSync(abs, edit.newContent, "utf-8");
        sandbox.modifiedFiles.add(edit.file);
        written.push(edit.file);
      }
      // Stage new files so diffs see them. Untracked files only.
      try {
        const untracked = execFileSync(
          "git",
          ["ls-files", "--others", "--exclude-standard"],
          { cwd: sandbox.worktreePath, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
        )
          .split("\n")
          .filter(Boolean);
        if (untracked.length > 0) {
          execFileSync("git", ["add", "-N", ...untracked], {
            cwd: sandbox.worktreePath,
            stdio: ["pipe", "pipe", "pipe"],
          });
        }
      } catch {
        // Non-fatal: diff still works for tracked files.
      }
      return { ok: true, filesWritten: written };
    } catch (err) {
      return {
        ok: false,
        filesWritten: written,
        detail: err instanceof Error ? err.message : String(err),
      };
    }
  }
}

// ---------------------------------------------------------------------------
// Default factory — assembles a StageDependencies bundle of references.
// ---------------------------------------------------------------------------

/**
 * Convenience factory: returns one of every reference implementation, keyed
 * by interface name. Integrators can use it as a starting point and override
 * single fields without re-implementing the bundle.
 */
export function createReferenceImplementations(db?: Db): {
  parser: ReferenceTsMorphParser;
  substrate?: ReferenceSqliteSubstrate;
  sandbox: ReferenceGitWorktreeSandbox;
  testRunner: ReferenceVitestPnpmRunner;
  scanner: ReferenceTypeScriptPatternScanner;
  applicator: ReferenceGitApplyApplicator;
} {
  return {
    parser: new ReferenceTsMorphParser(),
    substrate: db ? new ReferenceSqliteSubstrate(db) : undefined,
    sandbox: new ReferenceGitWorktreeSandbox(),
    testRunner: new ReferenceVitestPnpmRunner(),
    scanner: new ReferenceTypeScriptPatternScanner(),
    applicator: new ReferenceGitApplyApplicator(),
  };
}
