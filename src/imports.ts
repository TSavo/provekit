/**
 * TypeScript import resolver.
 *
 * Resolves import/require statements to source file paths.
 * Depth-1 only: we resolve direct imports, not transitive.
 */

import Parser from "tree-sitter";
import { existsSync, readFileSync } from "fs";
import { resolve, dirname, join } from "path";

export interface ResolvedImport {
  specifier: string;  // the import string as written
  resolvedPath: string;  // absolute path to the source file
  source: string;  // file contents
}

/**
 * Find all import/require statements in a parsed AST and resolve
 * them to source file paths.
 */
export function resolveImports(
  tree: Parser.Tree,
  filePath: string
): ResolvedImport[] {
  const imports: ResolvedImport[] = [];
  const dir = dirname(filePath);
  const seen = new Set<string>();

  function visit(node: Parser.SyntaxNode): void {
    // ES imports: import { foo } from './bar'
    if (node.type === "import_statement") {
      const source = node.childForFieldName("source");
      if (source) {
        const specifier = stripQuotes(source.text);
        tryResolve(specifier, dir, imports, seen);
      }
    }

    // Dynamic import: import('./bar')
    if (node.type === "call_expression") {
      const fn = node.childForFieldName("function");
      if (fn?.text === "import") {
        const args = node.childForFieldName("arguments");
        const firstArg = args?.firstNamedChild;
        if (firstArg?.type === "string") {
          const specifier = stripQuotes(firstArg.text);
          tryResolve(specifier, dir, imports, seen);
        }
      }

      // require('./bar')
      if (fn?.type === "identifier" && fn.text === "require") {
        const args = node.childForFieldName("arguments");
        const firstArg = args?.firstNamedChild;
        if (firstArg?.type === "string") {
          const specifier = stripQuotes(firstArg.text);
          tryResolve(specifier, dir, imports, seen);
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

function tryResolve(
  specifier: string,
  fromDir: string,
  imports: ResolvedImport[],
  seen: Set<string>
): void {
  // Only resolve relative imports — skip node_modules
  if (!specifier.startsWith(".") && !specifier.startsWith("/")) return;

  const resolved = resolveSpecifier(specifier, fromDir);
  if (!resolved || seen.has(resolved)) return;

  seen.add(resolved);

  try {
    const source = readFileSync(resolved, "utf-8");
    imports.push({ specifier, resolvedPath: resolved, source });
  } catch {
    // File not readable — skip
  }
}

/**
 * Resolve a relative import specifier to an absolute file path.
 * Tries common TypeScript/JavaScript extensions.
 */
function resolveSpecifier(specifier: string, fromDir: string): string | null {
  const base = resolve(fromDir, specifier);

  // Try exact path first
  const extensions = ["", ".ts", ".tsx", ".js", ".jsx"];
  for (const ext of extensions) {
    const candidate = base + ext;
    if (existsSync(candidate)) return candidate;
  }

  // Try index file in directory
  const indexExtensions = [".ts", ".tsx", ".js", ".jsx"];
  for (const ext of indexExtensions) {
    const candidate = join(base, `index${ext}`);
    if (existsSync(candidate)) return candidate;
  }

  return null;
}

function stripQuotes(s: string): string {
  return s.replace(/^['"`]|['"`]$/g, "");
}
