import { readFileSync } from "fs";
import { dirname } from "path";
import Module from "module";
import Parser from "tree-sitter";
import { parseFile } from "./parser";

let tsLib: typeof import("typescript") | null = null;
function getTs(): typeof import("typescript") | null {
  if (tsLib) return tsLib;
  try {
    tsLib = require("typescript");
    return tsLib;
  } catch {
    return null;
  }
}

export function collectTransitiveSource(filePath: string, projectRoot: string, depth: number = 1): string {
  const seen = new Set<string>();
  const parts: string[] = [];

  const visit = (abs: string, remaining: number) => {
    if (seen.has(abs)) return;
    seen.add(abs);
    let src: string;
    try {
      src = readFileSync(abs, "utf-8");
    } catch {
      return;
    }
    parts.push(`// === ${abs.replace(projectRoot, "").replace(/^\/+/, "")} ===`);
    parts.push(src);

    if (remaining <= 0) return;

    const importRe = /(?:^|\n)\s*import\s+(?:.+?\s+from\s+)?["']([^"']+)["']/g;
    let m;
    while ((m = importRe.exec(src)) !== null) {
      const spec = m[1]!;
      if (!spec.startsWith(".")) continue;
      const candidates = [
        spec,
        spec + ".ts",
        spec + ".tsx",
        spec + ".js",
        spec + "/index.ts",
        spec + "/index.js",
      ];
      const dir = dirname(abs);
      for (const c of candidates) {
        try {
          const Module = require("module");
          const resolved = require("path").resolve(dir, c);
          const { existsSync } = require("fs");
          if (existsSync(resolved)) {
            visit(resolved, remaining - 1);
            break;
          }
        } catch {
          continue;
        }
      }
    }
  };

  visit(filePath, depth);
  return parts.join("\n");
}

export function collectTopLevelNames(source: string): string[] {
  const tree = parseFile(source);
  const names = new Set<string>();

  const addName = (node: Parser.SyntaxNode | null | undefined) => {
    if (!node) return;
    if (node.type === "identifier" || node.type === "property_identifier" || node.type === "type_identifier") {
      names.add(node.text);
    }
  };

  const visitTopLevel = (node: Parser.SyntaxNode) => {
    for (const child of node.namedChildren) {
      if (child.type === "export_statement") {
        const inner = child.firstNamedChild;
        if (inner) handleDeclaration(inner);
        continue;
      }
      handleDeclaration(child);
    }
  };

  const handleDeclaration = (node: Parser.SyntaxNode) => {
    switch (node.type) {
      case "function_declaration":
      case "generator_function_declaration":
      case "class_declaration":
      case "interface_declaration":
      case "type_alias_declaration":
      case "enum_declaration":
        addName(node.childForFieldName("name"));
        break;
      case "lexical_declaration":
      case "variable_declaration":
      case "variable_statement": {
        for (const decl of node.namedChildren) {
          if (decl.type === "variable_declarator") {
            addName(decl.childForFieldName("name"));
          }
        }
        break;
      }
    }
  };

  visitTopLevel(tree.rootNode);
  return [...names];
}

export function loadModuleWithPrivates(filePath: string, parentModule?: NodeModule): any {
  const ts = getTs();
  if (!ts) throw new Error("typescript not available");

  const source = readFileSync(filePath, "utf-8");
  const names = collectTopLevelNames(source);

  const exporterLines = names.map(
    (n) =>
      `try { if (typeof ${n} !== 'undefined') exports[${JSON.stringify(n)}] = ${n}; } catch {}`
  );
  const augmented = source + "\n\n" + exporterLines.join("\n") + "\n";

  const transpiled = ts.transpileModule(augmented, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
      allowJs: true,
      skipLibCheck: true,
      experimentalDecorators: true,
    },
    fileName: filePath,
  });

  if (transpiled.diagnostics && transpiled.diagnostics.length > 0) {
    const msgs = transpiled.diagnostics
      .slice(0, 3)
      .map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n"))
      .join("; ");
    if (!transpiled.outputText) {
      throw new Error(`transpile failed: ${msgs}`);
    }
  }

  const mod = new Module(filePath, parentModule) as any;
  mod.filename = filePath;
  mod.paths = (Module as any)._nodeModulePaths(dirname(filePath));

  mod._compile(transpiled.outputText, filePath);
  return mod.exports;
}
