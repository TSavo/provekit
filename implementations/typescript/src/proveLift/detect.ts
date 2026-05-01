/**
 * Stage 1 of the prove-lift pipeline.
 *
 * Parse the input TS file, locate exactly one exported function,
 * extract its argument and return sorts. Refuse loudly on any of:
 *   - non-primitive parameter type
 *   - non-primitive return type
 *   - zero or multiple exported functions
 *   - exports that are not function declarations or function-typed
 *     `export const`s
 *
 * This is the only stage of v0 that is fully implemented in this run.
 * The downstream stages (Propose / Filter / Review / Mint) are stubs.
 *
 * Spec: docs/superpowers/specs/2026-04-30-provekit-lift-v0.md, Stage 1.
 */

import ts from "typescript";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  detectPrimitiveSort,
  typeToDiagnosticString,
  type LiftPrimitiveSort,
} from "./detectSort.js";
import {
  LiftError,
  makeDiagnostic,
  type LiftDiagnostic,
} from "./errors.js";

export interface FunctionParam {
  name: string;
  sort: LiftPrimitiveSort;
}

export interface FunctionShape {
  /** Exported function's name (used as the bridge symbol). */
  name: string;
  /** Absolute path of the input file. */
  filePath: string;
  /** Verbatim source of the input file (for prompt context). */
  sourceText: string;
  /** Verbatim source of the function declaration only. */
  functionSource: string;
  params: FunctionParam[];
  returnSort: LiftPrimitiveSort;
}

export interface DetectResult {
  shape: FunctionShape;
  /** Non-fatal diagnostics. v0 emits none today; reserved for v1 warnings. */
  diagnostics: LiftDiagnostic[];
}

export interface DetectOptions {
  /**
   * Override the file's text. Useful for tests that want to feed source
   * without writing to disk. When provided, filePath is treated as a
   * label only.
   */
  sourceTextOverride?: string;
}

export function detect(
  filePath: string,
  options: DetectOptions = {},
): DetectResult {
  const absPath = resolve(filePath);
  const sourceText =
    options.sourceTextOverride ?? readFileSync(absPath, "utf8");

  const compilerOptions: ts.CompilerOptions = {
    strict: true,
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    noEmit: true,
    skipLibCheck: true,
  };

  // Build an in-memory ts.Program where the input file is virtual.
  const host = createSingleFileHost(absPath, sourceText, compilerOptions);
  const program = ts.createProgram({
    rootNames: [absPath],
    options: compilerOptions,
    host,
  });
  const sourceFile = program.getSourceFile(absPath);
  if (!sourceFile) {
    // Defense-in-depth: createProgram should always populate the file
    // we just registered. If not, it's a real bug, not a user error.
    throw new Error(`detect: ts.Program could not load ${absPath}`);
  }
  const checker = program.getTypeChecker();

  const exports = collectExportedFunctions(sourceFile, checker);

  if (exports.length === 0) {
    throw new LiftError(
      makeDiagnostic(
        "no-exports",
        absPath,
        0,
        "lift requires exactly one exported function; found none.",
      ),
    );
  }
  if (exports.length > 1) {
    const names = exports.map((e) => e.name).join(", ");
    throw new LiftError(
      makeDiagnostic(
        "multiple-exports",
        absPath,
        0,
        `lift requires exactly one exported function; found ${exports.length} (${names}).`,
      ),
    );
  }

  const exp = exports[0]!;
  const lineOfDecl = sourceFile.getLineAndCharacterOfPosition(
    exp.declaration.getStart(sourceFile),
  ).line + 1;

  // Extract param sorts.
  const params: FunctionParam[] = [];
  for (const p of exp.parameters) {
    const paramType = checker.getTypeAtLocation(p.declaration);
    const sort = detectPrimitiveSort(paramType, checker);
    if (sort === null) {
      throw new LiftError(
        makeDiagnostic(
          "non-primitive-surface",
          absPath,
          lineOfDecl,
          `parameter ${p.name} of ${exp.name} has non-primitive type; lift v0 supports only number, string, boolean.`,
          typeToDiagnosticString(paramType, checker),
        ),
      );
    }
    params.push({ name: p.name, sort });
  }

  // Extract return sort.
  const returnType = checker.getReturnTypeOfSignature(exp.signature);
  const returnSort = detectPrimitiveSort(returnType, checker);
  if (returnSort === null) {
    throw new LiftError(
      makeDiagnostic(
        "non-primitive-surface",
        absPath,
        lineOfDecl,
        `return type of ${exp.name} is non-primitive; lift v0 supports only number, string, boolean.`,
        typeToDiagnosticString(returnType, checker),
      ),
    );
  }

  const functionSource = exp.declaration.getText(sourceFile);

  return {
    shape: {
      name: exp.name,
      filePath: absPath,
      sourceText,
      functionSource,
      params,
      returnSort,
    },
    diagnostics: [],
  };
}

// -----------------------------------------------------------------
// internal helpers
// -----------------------------------------------------------------

interface ExportedFunction {
  name: string;
  declaration: ts.Node;
  parameters: Array<{ name: string; declaration: ts.ParameterDeclaration }>;
  signature: ts.Signature;
}

function collectExportedFunctions(
  sourceFile: ts.SourceFile,
  checker: ts.TypeChecker,
): ExportedFunction[] {
  const out: ExportedFunction[] = [];

  for (const stmt of sourceFile.statements) {
    // Pattern A: `export function foo(...) { ... }`
    if (ts.isFunctionDeclaration(stmt) && hasExportModifier(stmt) && stmt.name) {
      const sig = checker.getSignatureFromDeclaration(stmt);
      if (!sig) continue;
      out.push({
        name: stmt.name.text,
        declaration: stmt,
        parameters: stmt.parameters.map((p) => ({
          name: getParamName(p),
          declaration: p,
        })),
        signature: sig,
      });
      continue;
    }

    // Pattern B: `export const foo = (...) => ...` or `export const foo = function(...) { ... }`
    if (ts.isVariableStatement(stmt) && hasExportModifier(stmt)) {
      for (const decl of stmt.declarationList.declarations) {
        if (!ts.isIdentifier(decl.name)) continue;
        if (!decl.initializer) continue;
        const init = decl.initializer;
        if (!ts.isArrowFunction(init) && !ts.isFunctionExpression(init)) continue;
        const sig = checker.getSignatureFromDeclaration(init);
        if (!sig) continue;
        out.push({
          name: decl.name.text,
          declaration: decl,
          parameters: init.parameters.map((p) => ({
            name: getParamName(p),
            declaration: p,
          })),
          signature: sig,
        });
      }
    }
  }

  return out;
}

function hasExportModifier(node: ts.Node): boolean {
  // Use canModifiers compatible with TS 5.x; ts.canHaveModifiers gates
  // ts.getModifiers which returns undefined when there are none.
  if (!ts.canHaveModifiers(node)) return false;
  const mods = ts.getModifiers(node);
  if (!mods) return false;
  return mods.some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
}

function getParamName(p: ts.ParameterDeclaration): string {
  if (ts.isIdentifier(p.name)) return p.name.text;
  // Destructured parameters fall outside v0's primitive contract;
  // detectPrimitiveSort will reject the type, so we only need a
  // human-readable label here.
  return p.name.getText();
}

function createSingleFileHost(
  filePath: string,
  sourceText: string,
  compilerOptions: ts.CompilerOptions,
): ts.CompilerHost {
  const baseHost = ts.createCompilerHost(compilerOptions, /*setParentNodes*/ true);
  return {
    ...baseHost,
    getSourceFile: (fileName, languageVersion, onError, shouldCreate) => {
      if (resolve(fileName) === filePath) {
        return ts.createSourceFile(
          fileName,
          sourceText,
          languageVersion,
          /*setParentNodes*/ true,
          ts.ScriptKind.TS,
        );
      }
      return baseHost.getSourceFile(fileName, languageVersion, onError, shouldCreate);
    },
    fileExists: (fileName) =>
      resolve(fileName) === filePath || baseHost.fileExists(fileName),
    readFile: (fileName) =>
      resolve(fileName) === filePath ? sourceText : baseHost.readFile(fileName),
    writeFile: () => {
      // Detect runs noEmit; never asked to write.
    },
  };
}
