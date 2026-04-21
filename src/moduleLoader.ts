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

export function collectTopLevelNames(source: string): string[] {
  const tree = parseFile(source);
  const names = new Set<string>();

  const addName = (node: Parser.SyntaxNode | null | undefined) => {
    if (!node) return;
    if (node.type === "identifier" || node.type === "property_identifier") {
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
