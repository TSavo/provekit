/**
 * Phase 1: Dependency Graph
 *
 * Input:  source file path(s)
 * Output: .neurallog/graph.json (immutable)
 *
 * Resolves imports, builds the dependency tree, determines
 * topological order for derivation. Writes once, read by Phase 2.
 */

import Parser from "tree-sitter";
import TypeScript from "tree-sitter-typescript";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { resolve, dirname, join, relative } from "path";

export interface FileNode {
  path: string;
  relativePath: string;
  imports: string[];        // resolved absolute paths of direct imports
  logStatements: number;    // count of log statements found
  hash: string;             // md5 of file contents
}

export interface DependencyGraph {
  root: string;             // the file that was analyzed
  projectRoot: string;
  files: FileNode[];
  topologicalOrder: string[];  // derivation order: leaves first, root last
  builtAt: string;
}

export function buildDependencyGraph(
  entryFilePath: string,
  projectRoot: string
): DependencyGraph {
  const entryPath = resolve(entryFilePath);
  const visited = new Map<string, FileNode>();

  console.log("Phase 1: Building dependency graph...");
  console.log(`  Entry: ${entryPath}`);

  walk(entryPath, projectRoot, visited);

  const files = [...visited.values()];
  console.log(`  Walked ${files.length} file${files.length === 1 ? "" : "s"}, ${files.reduce((n, f) => n + f.imports.length, 0)} import edges`);

  const topologicalOrder = topoSort(files);
  console.log(`  Topological sort: ${topologicalOrder.length} files, no cycles detected`);

  const graph: DependencyGraph = {
    root: entryPath,
    projectRoot,
    files,
    topologicalOrder,
    builtAt: new Date().toISOString(),
  };

  const outDir = join(projectRoot, ".neurallog");
  mkdirSync(outDir, { recursive: true });
  const graphPath = join(outDir, "graph.json");
  writeFileSync(graphPath, JSON.stringify(graph, null, 2));
  console.log(`  Graph written to ${relative(projectRoot, graphPath)}`);

  const totalLogStatements = files.reduce((n, f) => n + f.logStatements, 0);
  console.log(`  ${totalLogStatements} log statements across ${files.length} files`);
  console.log(`  Derivation order: ${topologicalOrder.map(p => relative(projectRoot, p)).join(" → ")}`);
  console.log();

  return graph;
}

function walk(
  filePath: string,
  projectRoot: string,
  visited: Map<string, FileNode>
): void {
  if (visited.has(filePath)) return;

  let source: string;
  try {
    source = readFileSync(filePath, "utf-8");
  } catch {
    console.log(`  WARNING: Could not read ${relative(projectRoot, filePath)}, skipping`);
    return;
  }

  const { createHash } = require("crypto");
  const hash = createHash("md5").update(source).digest("hex");

  const parser = new Parser();
  parser.setLanguage(TypeScript.typescript);
  const tree = parser.parse(source);

  const imports = findImports(tree, filePath);
  const logCount = countLogStatements(tree);

  const node: FileNode = {
    path: filePath,
    relativePath: relative(projectRoot, filePath),
    imports: imports.map((i) => i.resolvedPath),
    logStatements: logCount,
    hash,
  };

  visited.set(filePath, node);

  // Recurse into imports (depth-1 for now, but the graph supports deeper)
  for (const imp of imports) {
    walk(imp.resolvedPath, projectRoot, visited);
  }
}

interface ImportRef {
  specifier: string;
  resolvedPath: string;
}

function findImports(tree: Parser.Tree, filePath: string): ImportRef[] {
  const imports: ImportRef[] = [];
  const dir = dirname(filePath);

  function visit(node: Parser.SyntaxNode): void {
    if (node.type === "import_statement") {
      const source = node.childForFieldName("source");
      if (source) {
        const specifier = source.text.replace(/^['"`]|['"`]$/g, "");
        const resolved = resolveSpecifier(specifier, dir);
        if (resolved) imports.push({ specifier, resolvedPath: resolved });
      }
    }

    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn?.type === "identifier" && fn.text === "require") {
        const args = node.childForFieldName("arguments");
        const firstArg = args?.firstNamedChild;
        if (firstArg?.type === "string") {
          const specifier = firstArg.text.replace(/^['"`]|['"`]$/g, "");
          const resolved = resolveSpecifier(specifier, dir);
          if (resolved) imports.push({ specifier, resolvedPath: resolved });
        }
      }
    }

    for (const child of node.children) {
      visit(child);
    }
  }

  visit(tree.rootNode);
  return imports;
}

function resolveSpecifier(specifier: string, fromDir: string): string | null {
  if (!specifier.startsWith(".") && !specifier.startsWith("/")) return null;

  const base = resolve(fromDir, specifier);
  const extensions = ["", ".ts", ".tsx", ".js", ".jsx"];

  for (const ext of extensions) {
    const candidate = base + ext;
    if (existsSync(candidate)) return candidate;
  }

  for (const ext of [".ts", ".tsx", ".js", ".jsx"]) {
    const candidate = join(base, `index${ext}`);
    if (existsSync(candidate)) return candidate;
  }

  return null;
}

const LOG_OBJECTS = new Set(["console", "logger", "log"]);
const LOG_METHODS = new Set(["log", "info", "debug", "warn", "error", "trace"]);

function countLogStatements(tree: Parser.Tree): number {
  let count = 0;

  function visit(node: Parser.SyntaxNode): void {
    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn?.type === "member_expression") {
        const obj = fn.childForFieldName("object");
        const prop = fn.childForFieldName("property");
        if (obj && prop && LOG_OBJECTS.has(obj.text) && LOG_METHODS.has(prop.text)) {
          count++;
        }
      }
    }
    for (const child of node.children) {
      visit(child);
    }
  }

  visit(tree.rootNode);
  return count;
}

function topoSort(files: FileNode[]): string[] {
  const pathToNode = new Map(files.map((f) => [f.path, f]));
  const visited = new Set<string>();
  const order: string[] = [];

  function dfs(path: string): void {
    if (visited.has(path)) return;
    visited.add(path);

    const node = pathToNode.get(path);
    if (node) {
      for (const imp of node.imports) {
        dfs(imp);
      }
    }
    order.push(path);
  }

  for (const file of files) {
    dfs(file.path);
  }

  return order;
}
