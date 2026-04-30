import { Project, SyntaxKind } from "ts-morph";
import type { SmtBinding } from "../contracts.js";
import type { Z3Value } from "../z3/modelParser.js";

export interface SynthesizeArgs {
  functionSource: string;
  functionName: string;
  bindings: SmtBinding[];
  z3Model: Map<string, Z3Value>;
}

/**
 * Materialize a Z3Value into a JS runtime value.
 * Mirrors the logic in persistWitness.writeZ3Value but produces JS values
 * instead of writing to DB.
 */
export function materializeZ3Value(v: Z3Value): unknown {
  if (v.sort === "Real") {
    if (typeof v.value === "number") return v.value;
    if (v.value === "div_by_zero" || v.value === "nan") return NaN;
    if (v.value === "+infinity") return Infinity;
    if (v.value === "-infinity") return -Infinity;
  }
  if (v.sort === "Int") {
    const n = Number(v.value);
    if (Number.isSafeInteger(n)) return n;
    return v.value; // bigint
  }
  if (v.sort === "Bool") return v.value;
  if (v.sort === "String") return v.value;
  // Other — return raw string, best effort
  return "raw" in v ? v.raw : String(v);
}

interface ParamInfo {
  name: string;
  typeText: string;
}

function parseParams(functionSource: string, functionName: string): ParamInfo[] {
  const project = new Project({ useInMemoryFileSystem: true });
  const file = project.createSourceFile("input.ts", functionSource);

  // Try FunctionDeclaration first
  const funcDecl = file.getFunction(functionName);
  if (funcDecl) {
    return funcDecl.getParameters().map((p) => ({
      name: p.getName(),
      typeText: p.getTypeNode()?.getText() ?? "",
    }));
  }

  // Try VariableDeclaration + ArrowFunction / FunctionExpression
  const varDecl = file.getVariableDeclaration(functionName);
  if (varDecl) {
    const init = varDecl.getInitializer();
    if (init) {
      if (
        init.getKind() === SyntaxKind.ArrowFunction ||
        init.getKind() === SyntaxKind.FunctionExpression
      ) {
        const fn = init.asKindOrThrow(
          init.getKind() === SyntaxKind.ArrowFunction
            ? SyntaxKind.ArrowFunction
            : SyntaxKind.FunctionExpression,
        );
        return fn.getParameters().map((p) => ({
          name: p.getName(),
          typeText: p.getTypeNode()?.getText() ?? "",
        }));
      }
    }
  }

  throw new Error(
    `synthesizeInputs: function "${functionName}" not found in source. ` +
      `Checked FunctionDeclaration and VariableDeclaration (arrow/function expression).`,
  );
}

function defaultForType(typeText: string): unknown {
  if (typeText.includes("number")) return 0;
  if (typeText.includes("string")) return "";
  if (typeText.includes("boolean")) return false;
  return null;
}

export function synthesizeInputs(args: SynthesizeArgs): Record<string, unknown> {
  const { functionSource, functionName, bindings, z3Model } = args;

  const params = parseParams(functionSource, functionName);
  const result: Record<string, unknown> = {};

  for (const param of params) {
    const normalizedParam = param.name.replace(/\s+/g, "");

    // Find a binding whose source_expr (whitespace-normalized) matches this param name
    const binding = bindings.find(
      (b) => b.source_expr.replace(/\s+/g, "") === normalizedParam,
    );

    if (binding) {
      const z3val = z3Model.get(binding.smt_constant);
      if (z3val !== undefined) {
        result[param.name] = materializeZ3Value(z3val);
        continue;
      }
    }

    // Default by declared TypeScript type
    result[param.name] = defaultForType(param.typeText);
  }

  return result;
}
