import Parser from "tree-sitter";
import TypeScript from "tree-sitter-typescript";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { resolve, dirname, join, relative } from "path";
import { createHash } from "crypto";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { SignalRegistry } from "../signals";

export interface FileNode {
  path: string;
  relativePath: string;
  imports: string[];
  signalCount: number;
  hash: string;
}

export interface ParallelismGroup {
  depth: number;
  files: string[];
  signalCount: number;
}

export interface DependencyGraph {
  root: string;
  projectRoot: string;
  files: FileNode[];
  topologicalOrder: string[];
  parallelGroups: ParallelismGroup[];
  builtAt: string;
}

export interface DependencyInput {
  entryFilePath: string;
  signalRegistry: SignalRegistry;
  changedFiles?: string[];
}

export class DependencyPhase extends Phase<DependencyInput, DependencyGraph> {
  readonly name = "Dependency Graph";
  readonly phaseNumber = 1;

  execute(input: DependencyInput, options: PhaseOptions): PhaseResult<DependencyGraph> {
    const entryPath = resolve(input.entryFilePath);
    const visited = new Map<string, FileNode>();

    this.log("Building dependency graph...");
    this.detail(`Entry: ${entryPath}`);

    this.walk(entryPath, options.projectRoot, visited, input.signalRegistry);

    const files = [...visited.values()];
    this.detail(`Walked ${files.length} file${files.length === 1 ? "" : "s"}, ${files.reduce((n, f) => n + f.imports.length, 0)} import edges`);

    let topologicalOrder = this.topoSort(files);

    if (input.changedFiles && input.changedFiles.length > 0) {
      const changedSet = new Set(input.changedFiles.map((f) => resolve(f)));
      const affected = this.findAffected(files, changedSet);
      topologicalOrder = topologicalOrder.filter((p) => affected.has(p));
      this.detail(`Changed files: ${input.changedFiles.length}, affected: ${affected.size}`);

      if (topologicalOrder.length === 0 && files.length > 0) {
        this.detail("No changed files intersect the dependency graph — nothing to derive");
      }
    }

    this.detail(`Topological sort: ${topologicalOrder.length} files`);

    const parallelGroups = this.computeParallelGroups(files, topologicalOrder);
    for (const group of parallelGroups) {
      const fileNames = group.files.map((f) => relative(options.projectRoot, f));
      this.detail(`  Depth ${group.depth}: ${group.files.length} files, ${group.signalCount} signals [${fileNames.join(", ")}]`);
    }
    const maxParallelism = Math.max(...parallelGroups.map((g) => g.files.length), 0);
    this.detail(`Max parallelism: ${maxParallelism} files, ${parallelGroups.length} sequential groups`);

    const graph: DependencyGraph = {
      root: entryPath,
      projectRoot: options.projectRoot,
      files,
      topologicalOrder,
      parallelGroups,
      builtAt: new Date().toISOString(),
    };

    const outDir = join(options.projectRoot, ".neurallog");
    mkdirSync(outDir, { recursive: true });
    const graphPath = join(outDir, "graph.json");
    writeFileSync(graphPath, JSON.stringify(graph, null, 2));
    this.detail(`Graph written to ${relative(options.projectRoot, graphPath)}`);

    const totalSignals = files.reduce((n, f) => n + f.signalCount, 0);
    this.detail(`${totalSignals} signals across ${files.length} files`);
    this.detail(`Derivation order: ${topologicalOrder.map((p) => relative(options.projectRoot, p)).join(" -> ")}`);
    console.log();

    return { data: graph, writtenTo: graphPath };
  }

  private walk(
    filePath: string,
    projectRoot: string,
    visited: Map<string, FileNode>,
    signalRegistry: SignalRegistry
  ): void {
    if (visited.has(filePath)) return;

    let source: string;
    try {
      source = readFileSync(filePath, "utf-8");
    } catch {
      this.detail(`WARNING: Could not read ${relative(projectRoot, filePath)}, skipping`);
      return;
    }

    const hash = createHash("sha256").update(source).digest("hex");

    const parser = new Parser();
    parser.setLanguage(TypeScript.typescript);
    const tree = parser.parse(source);

    const imports = this.findImports(tree, filePath);
    const signals = signalRegistry.findAll(filePath, source, tree);

    const node: FileNode = {
      path: filePath,
      relativePath: relative(projectRoot, filePath),
      imports: imports.map((i) => i.resolvedPath),
      signalCount: signals.length,
      hash,
    };

    visited.set(filePath, node);

    for (const imp of imports) {
      this.walk(imp.resolvedPath, projectRoot, visited, signalRegistry);
    }
  }

  private findImports(tree: Parser.Tree, filePath: string): { specifier: string; resolvedPath: string }[] {
    const imports: { specifier: string; resolvedPath: string }[] = [];
    const dir = dirname(filePath);

    const visit = (node: Parser.SyntaxNode): void => {
      if (node.type === "import_statement") {
        const source = node.childForFieldName("source");
        if (source) {
          const specifier = source.text.replace(/^['"`]|['"`]$/g, "");
          const resolved = this.resolveSpecifier(specifier, dir);
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
            const resolved = this.resolveSpecifier(specifier, dir);
            if (resolved) imports.push({ specifier, resolvedPath: resolved });
          }
        }
      }

      for (const child of node.children) {
        visit(child);
      }
    };

    visit(tree.rootNode);
    return imports;
  }

  private resolveSpecifier(specifier: string, fromDir: string): string | null {
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

  private topoSort(files: FileNode[]): string[] {
    const pathToNode = new Map(files.map((f) => [f.path, f]));
    const visited = new Set<string>();
    const order: string[] = [];

    const dfs = (path: string): void => {
      if (visited.has(path)) return;
      visited.add(path);
      const node = pathToNode.get(path);
      if (node) {
        for (const imp of node.imports) dfs(imp);
      }
      order.push(path);
    };

    for (const file of files) dfs(file.path);
    return order;
  }

  private findAffected(files: FileNode[], changedSet: Set<string>): Set<string> {
    const dependents = new Map<string, string[]>();
    for (const file of files) {
      for (const imp of file.imports) {
        if (!dependents.has(imp)) dependents.set(imp, []);
        dependents.get(imp)!.push(file.path);
      }
    }

    const affected = new Set<string>();
    const queue = [...changedSet];
    while (queue.length > 0) {
      const current = queue.pop()!;
      if (affected.has(current)) continue;
      affected.add(current);
      const deps = dependents.get(current) || [];
      queue.push(...deps);
    }

    return affected;
  }

  private computeParallelGroups(files: FileNode[], topoOrder: string[]): ParallelismGroup[] {
    const fileMap = new Map(files.map((f) => [f.path, f]));
    const depths = new Map<string, number>();

    for (const path of topoOrder) {
      const node = fileMap.get(path);
      if (!node) {
        depths.set(path, 0);
        continue;
      }

      let maxDepDepth = -1;
      for (const imp of node.imports) {
        const depDepth = depths.get(imp);
        if (depDepth !== undefined && depDepth > maxDepDepth) {
          maxDepDepth = depDepth;
        }
      }
      depths.set(path, maxDepDepth + 1);
    }

    const groupMap = new Map<number, string[]>();
    for (const [path, depth] of depths) {
      if (!groupMap.has(depth)) groupMap.set(depth, []);
      groupMap.get(depth)!.push(path);
    }

    const groups: ParallelismGroup[] = [];
    for (const depth of [...groupMap.keys()].sort((a, b) => a - b)) {
      const groupFiles = groupMap.get(depth)!;
      const signalCount = groupFiles.reduce((n, f) => {
        const node = fileMap.get(f);
        return n + (node?.signalCount || 0);
      }, 0);
      groups.push({ depth, files: groupFiles, signalCount });
    }

    return groups;
  }
}
