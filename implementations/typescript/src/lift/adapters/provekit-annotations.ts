/**
 * provekit-lift adapter: provekit-annotations.
 *
 * Reads `@provekit.target` and `@provekit.post` JSDoc annotations
 * placed on functions by the OpenAPI/Protobuf annotation injector
 * (`provekit-lift-openapi --annotate`).
 *
 * Each annotated function becomes a contract whose post-condition
 * is the lifted guarantee from the OpenAPI endpoint spec. These
 * contracts bridge to the canonical OpenAPI .proof contracts when
 * the linker resolves cross-kit call edges.
 *
 * Position: sits between generated client code and the verifier.
 * The annotation injector writes the JSDoc; this adapter reads it
 * and promotes it to signed contracts.
 */

import ts from "typescript";
import type {
  IrFormula,
  IrTerm,
} from "../../ir/formulas.js";
import { Ref } from "../../ir/sorts.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "provekit-annotations";

/** Parse a JSON string into an IrFormula, validating basic shape. */
function parseFormulaJson(raw: string): IrFormula | null {
  let obj: unknown;
  try {
    obj = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!obj || typeof obj !== "object") return null;
  const f = obj as Record<string, unknown>;
  const kind = f.kind as string | undefined;
  if (
    kind === "forall" ||
    kind === "exists" ||
    kind === "and" ||
    kind === "or" ||
    kind === "not" ||
    kind === "implies" ||
    kind === "atomic"
  ) {
    return f as unknown as IrFormula;
  }
  return null;
}

/**
 * Walk JSDoc comments on a function node looking for
 * `@provekit.target` and `@provekit.post` tags.
 */
function extractAnnotation(
  node: ts.Node,
): { targetName: string; post: IrFormula } | null {
  const tags = ts.getJSDocTags(node);

  let targetName = "";
  let post: IrFormula | null = null;

  for (const tag of tags) {
    const tagName = tag.tagName.text;
    const comment = typeof tag.comment === "string" ? tag.comment.trim() : "";

    if (tagName === "provekit") {
      if (comment.startsWith(".target ")) {
        targetName = comment.slice(".target ".length).trim();
      } else if (comment.startsWith(".post ")) {
        post = parseFormulaJson(comment.slice(".post ".length).trim());
      }
    }
  }

  if (!targetName || !post) return null;
  return { targetName, post };
}

/**
 * Get the function name from a function declaration or
 * arrow function assigned to a variable.
 */
function getFunctionName(node: ts.Node): string | null {
  if (ts.isFunctionDeclaration(node) && node.name) {
    return node.name.text;
  }
  if (ts.isFunctionExpression(node) && node.name) {
    return node.name.text;
  }
  if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name)) {
    return node.name.text;
  }
  if (ts.isMethodDeclaration(node) && ts.isIdentifier(node.name)) {
    return node.name.text;
  }
  return null;
}

function getParentFunctionName(node: ts.Node): string | null {
  let cur: ts.Node | undefined = node;
  while (cur) {
    const name = getFunctionName(cur);
    if (name) return name;
    cur = cur.parent;
  }
  return null;
}

/** Recursively find annotation-carrying nodes. */
function walkAnnotations(
  node: ts.Node,
  sourcePath: string,
  decls: ContractDecl[],
  warnings: AdapterWarning[],
  seen: { count: number },
): void {
  if (ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node) ||
      ts.isArrowFunction(node) || ts.isMethodDeclaration(node) ||
      ts.isVariableDeclaration(node) || ts.isExportAssignment(node) ||
      ts.isFunctionLike(node)) {

    const ann = extractAnnotation(node);
    if (ann) {
      const name = getFunctionName(node) ?? getParentFunctionName(node);
      if (name) {
        seen.count += 1;
        decls.push({
          name,
          outBinding: "out",
          sourcePath,
          adapter: ADAPTER,
          post: ann.post,
          targetContract: ann.targetName,
        });
      } else {
        warnings.push({
          adapter: ADAPTER,
          sourcePath,
          itemName: "<anonymous>",
          reason: `@provekit.target "${ann.targetName}" on unnamed function`,
        });
      }
    }
  }

  ts.forEachChild(node, (child) =>
    walkAnnotations(child, sourcePath, decls, warnings, seen),
  );
}

export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  const seen = { count: 0 };

  walkAnnotations(sourceFile, sourcePath, decls, warnings, seen);

  return { decls, seen: seen.count, lifted: decls.length, warnings };
}
