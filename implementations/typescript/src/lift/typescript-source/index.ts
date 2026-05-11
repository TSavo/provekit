import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { relative, resolve } from "node:path";
import ts from "typescript";

import { canonicalJsonString } from "../../claimEnvelope/canonicalize.js";
import { computeCid } from "../../canonicalizer/hash.js";
import type { IrFormula, IrTerm, Sort } from "../../ir/formulas.js";

export type TypeScriptSourceEffect =
  | { kind: "reads"; target: string }
  | { kind: "writes"; target: string }
  | { kind: "io" }
  | { kind: "panics" }
  | { kind: "unresolved_call"; name: string }
  | { kind: "opaque_loop"; loopCid: string };

export interface TypeScriptSourceRefusal {
  kind: string;
  function: string | null;
  line: number | null;
  reason: string;
}

export interface TypeScriptSourceDiagnostic {
  severity: "warning" | "error";
  message: string;
}

export interface FunctionContractMemento {
  schemaVersion: "1";
  kind: "function-contract";
  fnName: string;
  formals: string[];
  formalSorts: Sort[];
  returnSort: Sort;
  pre: IrFormula;
  post: IrFormula;
  bodyCid: string | null;
  effects: TypeScriptSourceEffect[];
  locus: { file: string; line: number; col: number };
  autoMintedMementos: unknown[];
}

export interface TypeScriptSourceLiftResult {
  declarations: FunctionContractMemento[];
  diagnostics: TypeScriptSourceDiagnostic[];
  opacityReport: unknown[];
  refusals: TypeScriptSourceRefusal[];
}

interface LiftedFunction {
  contract: FunctionContractMemento;
  bodyTerm: IrTerm;
}

interface FileLiftContext {
  modulePath: string;
  sourceFile: ts.SourceFile;
  checker: ts.TypeChecker;
  moduleVars: Set<string>;
  knownCallables: Set<string>;
  refusals: TypeScriptSourceRefusal[];
}

interface FunctionContext extends FileLiftContext {
  functionName: string;
  locals: Set<string>;
  effects: Map<string, TypeScriptSourceEffect>;
}

class UnsupportedSyntaxError extends Error {
  constructor(
    public readonly node: ts.Node,
    message: string,
  ) {
    super(message);
    this.name = "UnsupportedSyntaxError";
  }
}

const TRUE_FORMULA: IrFormula = { kind: "atomic", name: "true", args: [] };
const RETURN_VALUE: IrTerm = { kind: "var", name: "return_value" };
const SKIP_DIRS = new Set([
  "node_modules",
  ".git",
  "dist",
  "build",
  "target",
  "out",
  ".next",
  ".turbo",
  ".vite",
  "coverage",
]);
const SOURCE_EXTS = new Set([".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"]);

export function liftTypeScriptSourceText(
  sourceText: string,
  fileName = "input.ts",
): TypeScriptSourceLiftResult {
  const modulePath = normalizePath(fileName);
  const { program, sourceFile } = createProgramFromText(sourceText, modulePath);
  return liftSourceFile(sourceFile, program.getTypeChecker(), modulePath);
}

export function liftTypeScriptSourcePaths(
  workspaceRoot: string,
  sourcePaths: string[],
): TypeScriptSourceLiftResult {
  const root = resolve(workspaceRoot);
  const diagnostics: TypeScriptSourceDiagnostic[] = [];
  const refusals: TypeScriptSourceRefusal[] = [];
  const files: string[] = [];

  for (const sourcePath of sourcePaths) {
    const fullPath = resolve(root, sourcePath);
    if (!isInsideRoot(root, fullPath)) {
      diagnostics.push({ severity: "error", message: `path traversal rejected: ${sourcePath}` });
      refusals.push({
        kind: "path-traversal",
        function: null,
        line: null,
        reason: `path '${sourcePath}' escapes workspace root '${root}'`,
      });
      continue;
    }
    if (!existsSync(fullPath)) {
      diagnostics.push({ severity: "warning", message: `path not found: ${fullPath}` });
      continue;
    }
    const st = statSync(fullPath);
    if (st.isDirectory()) {
      files.push(...enumerateSourceFiles(fullPath));
    } else if (st.isFile() && isSourceFile(fullPath)) {
      files.push(fullPath);
    }
  }

  if (files.length === 0) {
    return { declarations: [], diagnostics, opacityReport: [], refusals };
  }

  const program = ts.createProgram(files, compilerOptions());
  const checker = program.getTypeChecker();
  const declarations: FunctionContractMemento[] = [];
  for (const file of files.sort()) {
    const sourceFile = program.getSourceFile(file);
    if (!sourceFile) {
      diagnostics.push({ severity: "error", message: `program did not load ${file}` });
      continue;
    }
    const modulePath = normalizePath(relative(root, file));
    const lifted = liftSourceFile(sourceFile, checker, modulePath);
    declarations.push(...lifted.declarations);
    diagnostics.push(...lifted.diagnostics);
    refusals.push(...lifted.refusals);
  }
  return { declarations, diagnostics, opacityReport: [], refusals };
}

export function functionContractCid(contract: FunctionContractMemento): string {
  return cidOfValue(contract);
}

export function compileTypeScriptSourceIr(
  declarations: readonly FunctionContractMemento[],
): string {
  const sourceUnit = declarations.find((d) => d.fnName.endsWith(":<source-unit>"));
  if (sourceUnit) {
    const term = postRhs(sourceUnit);
    if (isCtor(term, "ts:source-unit")) {
      const bytes = termArgs(term)[0];
      if (bytes && bytes.kind === "const" && typeof bytes.value === "string") {
        return bytes.value;
      }
    }
  }

  const statements: ts.Statement[] = [];
  for (const decl of declarations) {
    if (decl.fnName.endsWith(":<source-unit>")) continue;
    statements.push(compileFunctionContract(decl));
  }
  return printStatements(statements);
}

export interface TypeScriptSourceBodyCompileOptions {
  functionName?: string;
  formals?: readonly string[];
  formalSorts?: readonly Sort[];
  returnSort?: Sort;
}

export function compileTypeScriptSourceBodyIr(
  bodyTerm: IrTerm,
  options: TypeScriptSourceBodyCompileOptions = {},
): string {
  const formals = [...(options.formals ?? freeVariableNames(bodyTerm))];
  const formalSorts = formals.map((_, index) => options.formalSorts?.[index] ?? primitiveSort("Any"));
  const functionName = options.functionName ?? "lifted";
  const contract: FunctionContractMemento = {
    schemaVersion: "1",
    kind: "function-contract",
    fnName: `roundtrip.ts:${functionName}`,
    formals,
    formalSorts,
    returnSort: options.returnSort ?? primitiveSort("Any"),
    pre: TRUE_FORMULA,
    post: eqFormula(RETURN_VALUE, bodyTerm),
    bodyCid: null,
    effects: [],
    locus: { file: "roundtrip.ts", line: 1, col: 1 },
    autoMintedMementos: [],
  };
  return printStatements([compileFunctionContract(contract)]);
}

function printStatements(statements: ts.Statement[]): string {
  const sourceFile = ts.factory.updateSourceFile(
    ts.createSourceFile("roundtrip.ts", "", ts.ScriptTarget.ES2022, false, ts.ScriptKind.TS),
    statements,
  );
  return ts.createPrinter({ newLine: ts.NewLineKind.LineFeed }).printFile(sourceFile);
}

function liftSourceFile(
  sourceFile: ts.SourceFile,
  checker: ts.TypeChecker,
  modulePath: string,
): TypeScriptSourceLiftResult {
  const refusals: TypeScriptSourceRefusal[] = [];
  const context: FileLiftContext = {
    modulePath,
    sourceFile,
    checker,
    moduleVars: collectModuleVariables(sourceFile),
    knownCallables: collectKnownCallables(sourceFile),
    refusals,
  };
  const lifted: LiftedFunction[] = [];
  processStatements(sourceFile.statements, [], context, lifted);

  const declarations = lifted.map((item) => item.contract);
  if (lifted.length > 0) {
    declarations.unshift(sourceUnitContract(sourceFile.getFullText(), context, lifted));
  }

  return { declarations, diagnostics: [], opacityReport: [], refusals };
}

function processStatements(
  statements: ts.NodeArray<ts.Statement>,
  prefix: string[],
  context: FileLiftContext,
  lifted: LiftedFunction[],
): void {
  for (const stmt of statements) {
    if (ts.isFunctionDeclaration(stmt)) {
      if (stmt.body) liftFunctionLike(stmt, stmt.name, stmt.body, prefix, context, lifted);
      continue;
    }
    if (ts.isClassDeclaration(stmt)) {
      if (!stmt.name) {
        addRefusal(context, stmt, null, "anonymous classes are not handled");
        continue;
      }
      for (const member of stmt.members) {
        if (ts.isMethodDeclaration(member)) {
          const name = methodNameText(member.name);
          if (name && member.body) {
            liftFunctionLike(member, member.name, member.body, [...prefix, stmt.name.text], context, lifted);
          } else {
            addRefusal(context, member, null, "computed or bodyless methods are not handled");
          }
        } else if (!ts.isPropertyDeclaration(member) && !ts.isConstructorDeclaration(member)) {
          addRefusal(context, member, `${stmt.name.text}.<member>`, `class member ${syntaxKindName(member)} is not handled`);
        }
      }
      continue;
    }
    if (ts.isModuleDeclaration(stmt)) {
      const name = moduleNameText(stmt.name);
      if (stmt.body && ts.isModuleBlock(stmt.body)) {
        processStatements(stmt.body.statements, [...prefix, name], context, lifted);
      } else {
        addRefusal(context, stmt, name, "non-block namespace/module declarations are not handled");
      }
      continue;
    }
    if (ts.isVariableStatement(stmt)) {
      refuseArrowVariables(stmt, context);
      continue;
    }
    if (
      ts.isImportDeclaration(stmt) ||
      ts.isExportDeclaration(stmt) ||
      ts.isInterfaceDeclaration(stmt) ||
      ts.isTypeAliasDeclaration(stmt)
    ) {
      continue;
    }
    addRefusal(context, stmt, null, `top-level ${syntaxKindName(stmt)} is not handled`);
  }
}

function liftFunctionLike(
  node: ts.FunctionDeclaration | ts.MethodDeclaration,
  nameNode: ts.PropertyName | ts.Identifier | undefined,
  body: ts.Block,
  prefix: string[],
  fileContext: FileLiftContext,
  lifted: LiftedFunction[],
): void {
  const shortName = nameNode ? nameFromPropertyName(nameNode) : null;
  if (!shortName) {
    addRefusal(fileContext, node, null, "anonymous or computed function names are not handled");
    return;
  }
  const qualifiedName = [...prefix, shortName].join(".");
  const fnName = `${fileContext.modulePath}:${qualifiedName}`;

  const unsupportedShapeReason = unsupportedFunctionShapeReason(node);
  if (unsupportedShapeReason) {
    addRefusal(fileContext, node, fnName, unsupportedShapeReason);
    return;
  }

  try {
    const formals = node.parameters.map((param) => parameterName(param));
    const formalSorts = node.parameters.map((param) => sortFromTypeNode(param.type, fileContext.checker, param));
    const returnSort = sortFromTypeNode(node.type, fileContext.checker, node);
    const locals = new Set<string>(formals);
    const functionContext: FunctionContext = {
      ...fileContext,
      functionName: fnName,
      locals,
      effects: new Map(),
    };
    collectLocalDeclarations(body, locals);
    const bodyTerm = emitBlock(body, functionContext);
    const postTerm = singleReturnExpression(body)
      ? emitExpression(singleReturnExpression(body)!, functionContext)
      : bodyTerm;
    const line = lineOf(fileContext.sourceFile, node);
    const contract: FunctionContractMemento = {
      schemaVersion: "1",
      kind: "function-contract",
      fnName,
      formals,
      formalSorts,
      returnSort,
      pre: TRUE_FORMULA,
      post: eqFormula(RETURN_VALUE, postTerm),
      bodyCid: null,
      effects: sortEffects([...functionContext.effects.values()]),
      locus: { file: fileContext.modulePath, line, col: 1 },
      autoMintedMementos: [],
    };
    lifted.push({ contract, bodyTerm: postTerm });
  } catch (error) {
    if (error instanceof UnsupportedSyntaxError) {
      addRefusal(fileContext, error.node, fnName, error.message);
    } else {
      addRefusal(fileContext, node, fnName, (error as Error).message);
    }
  }
}

function emitBlock(block: ts.Block, context: FunctionContext): IrTerm {
  const terms = block.statements.map((stmt) => emitStatement(stmt, context));
  if (terms.length === 0) {
    throw new UnsupportedSyntaxError(block, "empty function bodies are not handled");
  }
  return seqTerm(terms);
}

function emitStatement(stmt: ts.Statement, context: FunctionContext): IrTerm {
  if (ts.isBlock(stmt)) return emitBlock(stmt, context);
  if (ts.isReturnStatement(stmt)) {
    return ctor("ts:return", stmt.expression ? emitExpression(stmt.expression, context) : unitConst());
  }
  if (ts.isVariableStatement(stmt)) {
    const terms: IrTerm[] = [];
    for (const declaration of stmt.declarationList.declarations) {
      if (!ts.isIdentifier(declaration.name)) {
        throw new UnsupportedSyntaxError(declaration.name, "destructuring variable declarations are not handled");
      }
      if (declaration.initializer && ts.isArrowFunction(declaration.initializer)) {
        throw new UnsupportedSyntaxError(declaration.initializer, "arrow functions are not handled");
      }
      context.locals.add(declaration.name.text);
      terms.push(ctor("ts:decl", stringConst(declaration.name.text), declaration.initializer ? emitExpression(declaration.initializer, context) : unitConst()));
    }
    return seqTerm(terms);
  }
  if (ts.isExpressionStatement(stmt)) return emitExpression(stmt.expression, context);
  if (ts.isIfStatement(stmt)) {
    const thenTerm = emitStatement(stmt.thenStatement, context);
    const elseTerm = stmt.elseStatement ? emitStatement(stmt.elseStatement, context) : seqTerm([]);
    return ctor("ts:if", emitExpression(stmt.expression, context), thenTerm, elseTerm);
  }
  if (ts.isWhileStatement(stmt)) {
    const loopTerm = ctor("ts:while", emitExpression(stmt.expression, context), emitStatement(stmt.statement, context));
    addEffect(context, { kind: "opaque_loop", loopCid: cidOfValue(loopTerm) });
    return loopTerm;
  }
  if (ts.isForStatement(stmt)) {
    const init = stmt.initializer ? emitForInitializer(stmt.initializer, context) : seqTerm([]);
    const cond = stmt.condition ? emitExpression(stmt.condition, context) : boolConst(true);
    const update = stmt.incrementor ? emitExpression(stmt.incrementor, context) : seqTerm([]);
    const loopTerm = ctor("ts:for", init, cond, update, emitStatement(stmt.statement, context));
    addEffect(context, { kind: "opaque_loop", loopCid: cidOfValue(loopTerm) });
    return loopTerm;
  }
  if (ts.isThrowStatement(stmt)) {
    addEffect(context, { kind: "panics" });
    return ctor("ts:throw", stmt.expression ? emitExpression(stmt.expression, context) : unitConst());
  }
  if (ts.isBreakStatement(stmt)) return ctor("ts:break", unitConst());
  if (ts.isContinueStatement(stmt)) return ctor("ts:continue", unitConst());
  throw new UnsupportedSyntaxError(stmt, `statement kind ${syntaxKindName(stmt)} is not handled`);
}

function emitForInitializer(init: ts.ForInitializer, context: FunctionContext): IrTerm {
  if (ts.isVariableDeclarationList(init)) {
    const terms: IrTerm[] = [];
    for (const declaration of init.declarations) {
      if (!ts.isIdentifier(declaration.name)) {
        throw new UnsupportedSyntaxError(declaration.name, "destructuring for initializers are not handled");
      }
      context.locals.add(declaration.name.text);
      terms.push(ctor("ts:decl", stringConst(declaration.name.text), declaration.initializer ? emitExpression(declaration.initializer, context) : unitConst()));
    }
    return seqTerm(terms);
  }
  return emitExpression(init, context);
}

function emitExpression(expr: ts.Expression, context: FunctionContext): IrTerm {
  if (ts.isParenthesizedExpression(expr)) return emitExpression(expr.expression, context);
  if (ts.isNumericLiteral(expr)) return numberConst(Number(expr.text));
  if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) return stringConst(expr.text);
  if (expr.kind === ts.SyntaxKind.TrueKeyword) return boolConst(true);
  if (expr.kind === ts.SyntaxKind.FalseKeyword) return boolConst(false);
  if (expr.kind === ts.SyntaxKind.NullKeyword) return nullConst();
  if (ts.isIdentifier(expr)) {
    if (context.moduleVars.has(expr.text) && !context.locals.has(expr.text)) {
      addEffect(context, { kind: "reads", target: moduleCell(context, expr.text) });
    }
    return { kind: "var", name: expr.text };
  }
  if (expr.kind === ts.SyntaxKind.ThisKeyword) return { kind: "var", name: "this" };
  if (ts.isBinaryExpression(expr)) return emitBinaryExpression(expr, context);
  if (ts.isPrefixUnaryExpression(expr)) return emitPrefixExpression(expr, context);
  if (ts.isPostfixUnaryExpression(expr)) return emitPostfixExpression(expr, context);
  if (ts.isConditionalExpression(expr)) {
    return ctor("ts:ite", emitExpression(expr.condition, context), emitExpression(expr.whenTrue, context), emitExpression(expr.whenFalse, context));
  }
  if (ts.isCallExpression(expr)) return emitCallExpression(expr, context);
  if (ts.isTypeOfExpression(expr)) return ctor("ts:typeof", emitExpression(expr.expression, context));
  if (ts.isPropertyAccessExpression(expr)) {
    if (expr.questionDotToken) {
      throw new UnsupportedSyntaxError(expr, "optional chaining is not handled");
    }
    return ctor("ts:member", emitExpression(expr.expression, context), stringConst(expr.name.text));
  }
  if (ts.isElementAccessExpression(expr)) {
    if (expr.questionDotToken) {
      throw new UnsupportedSyntaxError(expr, "optional chaining is not handled");
    }
    return ctor("ts:index", emitExpression(expr.expression, context), emitExpression(expr.argumentExpression, context));
  }
  if (ts.isNewExpression(expr)) {
    const args = expr.arguments ? expr.arguments.map((arg) => emitExpression(arg, context)) : [];
    return ctor("ts:new", emitExpression(expr.expression, context), argsTerm(args));
  }
  if (ts.isTemplateExpression(expr)) {
    throw new UnsupportedSyntaxError(expr, "template literals are not handled");
  }
  if (ts.isAsExpression(expr) || ts.isTypeAssertionExpression(expr) || ts.isSatisfiesExpression(expr)) {
    throw new UnsupportedSyntaxError(expr, "type assertions and satisfies expressions are not handled");
  }
  if (ts.isAwaitExpression(expr)) {
    throw new UnsupportedSyntaxError(expr, "await expressions are not handled");
  }
  if (ts.isArrowFunction(expr) || ts.isFunctionExpression(expr)) {
    throw new UnsupportedSyntaxError(expr, "function expressions are not handled");
  }
  throw new UnsupportedSyntaxError(expr, `expression kind ${syntaxKindName(expr)} is not handled`);
}

function emitBinaryExpression(expr: ts.BinaryExpression, context: FunctionContext): IrTerm {
  if (expr.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
    const value = emitExpression(expr.right, context);
    addWriteEffectForTarget(expr.left, context);
    return ctor("ts:assign", emitLValue(expr.left, context), value);
  }

  const op = binaryOperatorName(expr.operatorToken.kind);
  if (!op) {
    throw new UnsupportedSyntaxError(expr.operatorToken, `binary operator ${syntaxKindName(expr.operatorToken)} is not handled`);
  }
  return ctor(op, emitExpression(expr.left, context), emitExpression(expr.right, context));
}

function emitPrefixExpression(expr: ts.PrefixUnaryExpression, context: FunctionContext): IrTerm {
  const operand = emitExpression(expr.operand, context);
  switch (expr.operator) {
    case ts.SyntaxKind.ExclamationToken:
      return ctor("ts:not", operand);
    case ts.SyntaxKind.MinusToken:
      return ctor("ts:neg", operand);
    case ts.SyntaxKind.PlusToken:
      return ctor("ts:pos", operand);
    case ts.SyntaxKind.TildeToken:
      return ctor("ts:bitnot", operand);
    case ts.SyntaxKind.PlusPlusToken:
      addWriteEffectForTarget(expr.operand, context);
      return ctor("ts:preinc", emitLValue(expr.operand, context));
    case ts.SyntaxKind.MinusMinusToken:
      addWriteEffectForTarget(expr.operand, context);
      return ctor("ts:predec", emitLValue(expr.operand, context));
    default:
      throw new UnsupportedSyntaxError(expr, `prefix operator ${syntaxKindName(expr)} is not handled`);
  }
}

function emitPostfixExpression(expr: ts.PostfixUnaryExpression, context: FunctionContext): IrTerm {
  addWriteEffectForTarget(expr.operand, context);
  switch (expr.operator) {
    case ts.SyntaxKind.PlusPlusToken:
      return ctor("ts:postinc", emitLValue(expr.operand, context));
    case ts.SyntaxKind.MinusMinusToken:
      return ctor("ts:postdec", emitLValue(expr.operand, context));
    default:
      throw new UnsupportedSyntaxError(expr, `postfix operator ${syntaxKindName(expr)} is not handled`);
  }
}

function emitCallExpression(expr: ts.CallExpression, context: FunctionContext): IrTerm {
  if (expr.questionDotToken) {
    throw new UnsupportedSyntaxError(expr, "optional calls are not handled");
  }
  const calleeName = calleeText(expr.expression);
  for (const arg of expr.arguments) {
    if (ts.isSpreadElement(arg)) {
      throw new UnsupportedSyntaxError(arg, "spread call arguments are not handled");
    }
  }
  const args = expr.arguments.map((arg) => emitExpression(arg, context));
  if (isIoCallee(calleeName)) {
    addEffect(context, { kind: "io" });
  } else if (calleeName && !context.knownCallables.has(calleeName)) {
    addEffect(context, { kind: "unresolved_call", name: calleeName });
  }
  return ctor("ts:call", emitExpression(expr.expression, context), argsTerm(args));
}

function emitLValue(expr: ts.Expression, context: FunctionContext): IrTerm {
  if (ts.isIdentifier(expr)) return { kind: "var", name: expr.text };
  if (ts.isPropertyAccessExpression(expr)) return ctor("ts:member", emitExpression(expr.expression, context), stringConst(expr.name.text));
  if (ts.isElementAccessExpression(expr)) return ctor("ts:index", emitExpression(expr.expression, context), emitExpression(expr.argumentExpression, context));
  throw new UnsupportedSyntaxError(expr, `assignment target ${syntaxKindName(expr)} is not handled`);
}

function binaryOperatorName(kind: ts.SyntaxKind): string | null {
  switch (kind) {
    case ts.SyntaxKind.PlusToken: return "ts:add";
    case ts.SyntaxKind.MinusToken: return "ts:sub";
    case ts.SyntaxKind.AsteriskToken: return "ts:mul";
    case ts.SyntaxKind.SlashToken: return "ts:div";
    case ts.SyntaxKind.PercentToken: return "ts:mod";
    case ts.SyntaxKind.EqualsEqualsToken:
    case ts.SyntaxKind.EqualsEqualsEqualsToken: return "ts:eq";
    case ts.SyntaxKind.ExclamationEqualsToken:
    case ts.SyntaxKind.ExclamationEqualsEqualsToken: return "ts:ne";
    case ts.SyntaxKind.LessThanToken: return "ts:lt";
    case ts.SyntaxKind.LessThanEqualsToken: return "ts:le";
    case ts.SyntaxKind.GreaterThanToken: return "ts:gt";
    case ts.SyntaxKind.GreaterThanEqualsToken: return "ts:ge";
    case ts.SyntaxKind.AmpersandAmpersandToken: return "ts:and";
    case ts.SyntaxKind.BarBarToken: return "ts:or";
    case ts.SyntaxKind.QuestionQuestionToken: return "ts:nullish";
    case ts.SyntaxKind.AmpersandToken: return "ts:bitand";
    case ts.SyntaxKind.BarToken: return "ts:bitor";
    case ts.SyntaxKind.CaretToken: return "ts:bitxor";
    case ts.SyntaxKind.LessThanLessThanToken: return "ts:shl";
    case ts.SyntaxKind.GreaterThanGreaterThanToken: return "ts:shr";
    case ts.SyntaxKind.GreaterThanGreaterThanGreaterThanToken: return "ts:ushr";
    default: return null;
  }
}

function addWriteEffectForTarget(expr: ts.Expression, context: FunctionContext): void {
  const root = rootIdentifier(expr);
  if (root && context.moduleVars.has(root) && !context.locals.has(root)) {
    addEffect(context, { kind: "writes", target: moduleCell(context, root) });
  } else if (!root && (ts.isPropertyAccessExpression(expr) || ts.isElementAccessExpression(expr))) {
    addEffect(context, { kind: "writes", target: normalizeWhitespace(expr.getText(context.sourceFile)) });
  }
}

function addEffect(context: FunctionContext, effect: TypeScriptSourceEffect): void {
  context.effects.set(effectSortKey(effect), effect);
}

function sortEffects(effects: TypeScriptSourceEffect[]): TypeScriptSourceEffect[] {
  return effects.sort((a, b) => effectSortKey(a).localeCompare(effectSortKey(b)));
}

function effectSortKey(effect: TypeScriptSourceEffect): string {
  switch (effect.kind) {
    case "reads": return `0:reads:${effect.target}`;
    case "writes": return `1:writes:${effect.target}`;
    case "io": return "2:io";
    case "panics": return "4:panics";
    case "unresolved_call": return `5:unresolved:${effect.name}`;
    case "opaque_loop": return `6:opaque_loop:${effect.loopCid}`;
  }
}

function collectModuleVariables(sourceFile: ts.SourceFile): Set<string> {
  const vars = new Set<string>();
  const visitStatements = (statements: ts.NodeArray<ts.Statement>): void => {
    for (const stmt of statements) {
      if (ts.isVariableStatement(stmt)) {
        for (const decl of stmt.declarationList.declarations) {
          if (ts.isIdentifier(decl.name)) vars.add(decl.name.text);
        }
      } else if (ts.isModuleDeclaration(stmt) && stmt.body && ts.isModuleBlock(stmt.body)) {
        visitStatements(stmt.body.statements);
      }
    }
  };
  visitStatements(sourceFile.statements);
  return vars;
}

function collectKnownCallables(sourceFile: ts.SourceFile): Set<string> {
  const callables = new Set<string>();
  const visitStatements = (statements: ts.NodeArray<ts.Statement>): void => {
    for (const stmt of statements) {
      if (ts.isFunctionDeclaration(stmt) && stmt.name && stmt.body) callables.add(stmt.name.text);
      if (ts.isClassDeclaration(stmt)) {
        for (const member of stmt.members) {
          if (ts.isMethodDeclaration(member)) {
            const name = methodNameText(member.name);
            if (name) callables.add(name);
          }
        }
      }
      if (ts.isModuleDeclaration(stmt) && stmt.body && ts.isModuleBlock(stmt.body)) visitStatements(stmt.body.statements);
    }
  };
  visitStatements(sourceFile.statements);
  return callables;
}

function collectLocalDeclarations(node: ts.Node, locals: Set<string>): void {
  const visit = (child: ts.Node): void => {
    if (ts.isVariableDeclaration(child) && ts.isIdentifier(child.name)) locals.add(child.name.text);
    if (ts.isFunctionLike(child) && child !== node) return;
    ts.forEachChild(child, visit);
  };
  ts.forEachChild(node, visit);
}

function refuseArrowVariables(stmt: ts.VariableStatement, context: FileLiftContext): void {
  for (const decl of stmt.declarationList.declarations) {
    if (decl.initializer && ts.isArrowFunction(decl.initializer)) {
      const name = ts.isIdentifier(decl.name) ? decl.name.text : null;
      addRefusal(context, decl.initializer, name, "arrow functions are not handled by the typescript-source lifter");
    }
  }
}

function sourceUnitContract(sourceText: string, context: FileLiftContext, lifted: LiftedFunction[]): FunctionContractMemento {
  return {
    schemaVersion: "1",
    kind: "function-contract",
    fnName: `${context.modulePath}:<source-unit>`,
    formals: [],
    formalSorts: [],
    returnSort: primitiveSort("Unit"),
    pre: TRUE_FORMULA,
    post: eqFormula(RETURN_VALUE, ctor("ts:source-unit", stringConst(sourceText), seqTerm(lifted.map((f) => f.bodyTerm)))),
    bodyCid: null,
    effects: [],
    locus: { file: context.modulePath, line: 1, col: 1 },
    autoMintedMementos: [],
  };
}

function unsupportedFunctionShapeReason(node: ts.FunctionDeclaration | ts.MethodDeclaration): string | null {
  const decorators = ts.canHaveDecorators(node) ? ts.getDecorators(node) : undefined;
  if (node.modifiers?.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword)) {
    return "async function not supported";
  }
  if (node.asteriskToken) {
    return "generator function not supported";
  }
  if (decorators?.length) {
    return "decorated function not supported";
  }
  if (node.typeParameters?.length) {
    return "generic type parameters not supported";
  }
  for (const param of node.parameters) {
    if (param.dotDotDotToken) {
      return "rest parameters not supported";
    }
    if (param.initializer) {
      return "default parameters not supported";
    }
    if (!ts.isIdentifier(param.name)) {
      return "destructured parameters not supported";
    }
  }
  return null;
}

function parameterName(param: ts.ParameterDeclaration): string {
  if (!ts.isIdentifier(param.name)) {
    throw new UnsupportedSyntaxError(param.name, "destructuring parameters are not handled");
  }
  return param.name.text;
}

function sortFromTypeNode(node: ts.TypeNode | undefined, checker: ts.TypeChecker, at: ts.Node): Sort {
  if (!node) {
    const type = checker.getTypeAtLocation(at);
    const text = checker.typeToString(type);
    return sortFromTypeText(text, at);
  }
  return sortFromTypeText(node.getText(), node);
}

function sortFromTypeText(typeText: string, node: ts.Node): Sort {
  switch (typeText.trim()) {
    case "number": return primitiveSort("Number");
    case "boolean": return primitiveSort("Boolean");
    case "string": return primitiveSort("String");
    case "void": return primitiveSort("Unit");
    case "any": return primitiveSort("Any");
    case "unknown": return primitiveSort("Unknown");
    default:
      throw new UnsupportedSyntaxError(node, `type ${typeText} is not in the handled basic TypeScript slice`);
  }
}

function singleReturnExpression(block: ts.Block): ts.Expression | null {
  if (block.statements.length !== 1) return null;
  const only = block.statements[0]!;
  return ts.isReturnStatement(only) && only.expression ? only.expression : null;
}

function moduleCell(context: FileLiftContext, name: string): string {
  return `${context.modulePath}:${name}`;
}

function rootIdentifier(expr: ts.Expression): string | null {
  if (ts.isIdentifier(expr)) return expr.text;
  if (ts.isPropertyAccessExpression(expr)) return rootIdentifier(expr.expression);
  if (ts.isElementAccessExpression(expr)) return rootIdentifier(expr.expression);
  return null;
}

function calleeText(expr: ts.Expression): string | null {
  if (ts.isIdentifier(expr)) return expr.text;
  if (ts.isPropertyAccessExpression(expr)) {
    const left = calleeText(expr.expression);
    return left ? `${left}.${expr.name.text}` : expr.name.text;
  }
  return null;
}

function isIoCallee(callee: string | null): boolean {
  if (!callee) return false;
  return callee === "fetch" || callee.startsWith("console.") || callee === "fs" || callee.startsWith("fs.") || callee.startsWith("net.") || callee.startsWith("http.") || callee.startsWith("https.");
}

function addRefusal(
  context: FileLiftContext,
  node: ts.Node,
  functionName: string | null,
  reason: string,
): void {
  context.refusals.push({
    kind: syntaxKindName(node),
    function: functionName,
    line: lineOf(context.sourceFile, node),
    reason,
  });
}

function lineOf(sourceFile: ts.SourceFile, node: ts.Node): number {
  return sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
}

function syntaxKindName(node: ts.Node): string {
  return ts.SyntaxKind[node.kind] ?? String(node.kind);
}

function nameFromPropertyName(name: ts.PropertyName): string | null {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) return name.text;
  return null;
}

function methodNameText(name: ts.PropertyName): string | null {
  return nameFromPropertyName(name);
}

function moduleNameText(name: ts.ModuleName): string {
  return name.text;
}

function ctor(name: string, ...args: IrTerm[]): IrTerm {
  return { kind: "ctor", name, args };
}

function isCtor(term: IrTerm | undefined, name: string): boolean {
  return term?.kind === "ctor" && term.name === name;
}

function termArgs(term: IrTerm): IrTerm[] {
  if (term.kind !== "ctor") throw new Error(`expected ctor term, got ${term.kind}`);
  return term.args;
}

function freeVariableNames(term: IrTerm): string[] {
  const names = new Set<string>();
  const visit = (current: IrTerm, bound: Set<string>): void => {
    if (current.kind === "var") {
      if (current.name !== RETURN_VALUE.name && !bound.has(current.name)) names.add(current.name);
      return;
    }
    if (current.kind === "const") return;
    if (current.kind === "lambda") {
      visit(current.body, new Set([...bound, current.paramName]));
      return;
    }
    if (current.kind === "let") {
      const letBound = new Set(bound);
      for (const binding of current.bindings) {
        visit(binding.boundTerm, letBound);
        letBound.add(binding.name);
      }
      visit(current.body, letBound);
      return;
    }
    if (current.name === "ts:seq") {
      const seqBound = new Set(bound);
      for (const arg of current.args) {
        if (isCtor(arg, "ts:decl")) {
          visit(termArgs(arg)[1] ?? unitConst(), seqBound);
          const declared = stringValue(termArgs(arg)[0], "");
          if (declared) seqBound.add(declared);
        } else {
          visit(arg, seqBound);
        }
      }
      return;
    }
    for (const arg of current.args) visit(arg, bound);
  };
  visit(term, new Set());
  return [...names];
}

function seqTerm(args: IrTerm[]): IrTerm {
  return ctor("ts:seq", ...args);
}

function argsTerm(args: IrTerm[]): IrTerm {
  return ctor("ts:args", ...args);
}

function primitiveSort(name: string): Sort {
  return { kind: "primitive", name };
}

function numberConst(value: number): IrTerm {
  return { kind: "const", value, sort: primitiveSort("Number") };
}

function boolConst(value: boolean): IrTerm {
  return { kind: "const", value, sort: primitiveSort("Boolean") };
}

function stringConst(value: string): IrTerm {
  return { kind: "const", value, sort: primitiveSort("String") };
}

function nullConst(): IrTerm {
  return { kind: "const", value: null, sort: primitiveSort("Null") };
}

function unitConst(): IrTerm {
  return { kind: "const", value: null, sort: primitiveSort("Unit") };
}

function eqFormula(lhs: IrTerm, rhs: IrTerm): IrFormula {
  return { kind: "atomic", name: "=", args: [lhs, rhs] };
}

function postRhs(contract: FunctionContractMemento): IrTerm {
  const post = contract.post;
  if (post.kind === "atomic" && post.name === "=" && post.args.length === 2) return post.args[1]!;
  throw new Error(`contract ${contract.fnName} does not have a return_value equality postcondition`);
}

function cidOfValue(value: unknown): string {
  return computeCid(Buffer.from(canonicalJsonString(value), "utf8"));
}

function createProgramFromText(sourceText: string, fileName: string): { program: ts.Program; sourceFile: ts.SourceFile } {
  const sourceFile = ts.createSourceFile(fileName, sourceText, ts.ScriptTarget.ES2022, true, scriptKind(fileName));
  const options = compilerOptions();
  const host = ts.createCompilerHost(options, true);
  const originalGetSourceFile = host.getSourceFile.bind(host);
  const normalizedFileName = normalizePath(fileName);
  host.getSourceFile = (requested, languageVersion, onError, shouldCreateNewSourceFile) => {
    if (normalizePath(requested) === normalizedFileName) return sourceFile;
    return originalGetSourceFile(requested, languageVersion, onError, shouldCreateNewSourceFile);
  };
  host.readFile = (requested) => normalizePath(requested) === normalizedFileName ? sourceText : readFileIfExists(requested);
  host.fileExists = (requested) => normalizePath(requested) === normalizedFileName || existsSync(requested);
  const program = ts.createProgram([fileName], options, host);
  return { program, sourceFile };
}

function compilerOptions(): ts.CompilerOptions {
  return {
    allowJs: true,
    checkJs: false,
    module: ts.ModuleKind.CommonJS,
    noResolve: true,
    skipLibCheck: true,
    target: ts.ScriptTarget.ES2022,
  };
}

function readFileIfExists(path: string): string | undefined {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return undefined;
  }
}

function enumerateSourceFiles(root: string): string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir)) {
      const path = resolve(dir, entry);
      const st = statSync(path);
      if (st.isDirectory()) {
        if (!SKIP_DIRS.has(entry) && !entry.startsWith(".")) walk(path);
      } else if (st.isFile() && isSourceFile(path)) {
        out.push(path);
      }
    }
  };
  walk(root);
  return out;
}

function isSourceFile(path: string): boolean {
  if (path.endsWith(".d.ts")) return false;
  const lower = path.toLowerCase();
  return [...SOURCE_EXTS].some((ext) => lower.endsWith(ext));
}

function scriptKind(fileName: string): ts.ScriptKind {
  if (fileName.endsWith(".tsx")) return ts.ScriptKind.TSX;
  if (fileName.endsWith(".jsx")) return ts.ScriptKind.JSX;
  if (fileName.endsWith(".js")) return ts.ScriptKind.JS;
  return ts.ScriptKind.TS;
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

function normalizeWhitespace(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function isInsideRoot(root: string, fullPath: string): boolean {
  const rel = relative(root, fullPath);
  return rel === "" || (!rel.startsWith("..") && !resolve(rel).startsWith("/.."));
}

function compileFunctionContract(decl: FunctionContractMemento): ts.FunctionDeclaration {
  const name = sanitizeIdentifier(decl.fnName.split(":").pop()?.split(".").pop() || "lifted");
  const parameters = decl.formals.map((formal, index) => ts.factory.createParameterDeclaration(
    undefined,
    undefined,
    sanitizeIdentifier(formal),
    undefined,
    typeNodeForSort(decl.formalSorts[index]),
  ));
  return ts.factory.createFunctionDeclaration(
    undefined,
    undefined,
    name,
    undefined,
    parameters,
    typeNodeForSort(decl.returnSort),
    ts.factory.createBlock(emitStatementsFromTerm(postRhs(decl)), true),
  );
}

function emitStatementsFromTerm(term: IrTerm): ts.Statement[] {
  if (isCtor(term, "ts:seq")) return termArgs(term).flatMap(emitStatementsFromTerm);
  if (isCtor(term, "ts:return")) {
    const args = termArgs(term);
    return [ts.factory.createReturnStatement(args[0] ? expressionFromTerm(args[0]) : undefined)];
  }
  if (isCtor(term, "ts:decl")) {
    const args = termArgs(term);
    const name = stringValue(args[0], "tmp");
    return [ts.factory.createVariableStatement(undefined, ts.factory.createVariableDeclarationList([
      ts.factory.createVariableDeclaration(sanitizeIdentifier(name), undefined, undefined, expressionFromTerm(args[1])),
    ], ts.NodeFlags.Let))];
  }
  if (isCtor(term, "ts:assign")) return [ts.factory.createExpressionStatement(expressionFromTerm(term))];
  if (isCtor(term, "ts:throw")) return [ts.factory.createThrowStatement(expressionFromTerm(termArgs(term)[0]))];
  if (isCtor(term, "ts:if")) {
    const args = termArgs(term);
    return [ts.factory.createIfStatement(
      expressionFromTerm(args[0]),
      ts.factory.createBlock(emitStatementsFromTerm(args[1] ?? seqTerm([])), true),
      ts.factory.createBlock(emitStatementsFromTerm(args[2] ?? seqTerm([])), true),
    )];
  }
  if (isCtor(term, "ts:while")) {
    const args = termArgs(term);
    return [ts.factory.createWhileStatement(
      expressionFromTerm(args[0]),
      ts.factory.createBlock(emitStatementsFromTerm(args[1] ?? seqTerm([])), true),
    )];
  }
  return [ts.factory.createReturnStatement(expressionFromTerm(term))];
}

function expressionFromTerm(term: IrTerm | undefined): ts.Expression {
  if (!term) return ts.factory.createVoidZero();
  if (term.kind === "const") {
    if (typeof term.value === "number") return ts.factory.createNumericLiteral(String(term.value));
    if (typeof term.value === "boolean") return term.value ? ts.factory.createTrue() : ts.factory.createFalse();
    if (typeof term.value === "string") return ts.factory.createStringLiteral(term.value);
    return ts.factory.createNull();
  }
  if (term.kind === "var") return ts.factory.createIdentifier(sanitizeIdentifier(term.name));
  if (term.kind !== "ctor") throw new Error(`cannot compile term kind ${term.kind}`);
  const args = term.args;
  const binary = binaryTokenForCtor(term.name);
  if (binary !== null) return ts.factory.createBinaryExpression(expressionFromTerm(args[0]), binary, expressionFromTerm(args[1]));
  switch (term.name) {
    case "ts:not": return ts.factory.createPrefixUnaryExpression(ts.SyntaxKind.ExclamationToken, expressionFromTerm(args[0]));
    case "ts:neg": return ts.factory.createPrefixUnaryExpression(ts.SyntaxKind.MinusToken, expressionFromTerm(args[0]));
    case "ts:pos": return ts.factory.createPrefixUnaryExpression(ts.SyntaxKind.PlusToken, expressionFromTerm(args[0]));
    case "ts:bitnot": return ts.factory.createPrefixUnaryExpression(ts.SyntaxKind.TildeToken, expressionFromTerm(args[0]));
    case "ts:typeof": return ts.factory.createTypeOfExpression(expressionFromTerm(args[0]));
    case "ts:ite": return ts.factory.createConditionalExpression(expressionFromTerm(args[0]), ts.factory.createToken(ts.SyntaxKind.QuestionToken), expressionFromTerm(args[1]), ts.factory.createToken(ts.SyntaxKind.ColonToken), expressionFromTerm(args[2]));
    case "ts:assign": return ts.factory.createAssignment(expressionFromTerm(args[0]), expressionFromTerm(args[1]));
    case "ts:member": return ts.factory.createPropertyAccessExpression(expressionFromTerm(args[0]), stringValue(args[1], "field"));
    case "ts:index": return ts.factory.createElementAccessExpression(expressionFromTerm(args[0]), expressionFromTerm(args[1]));
    case "ts:call": return ts.factory.createCallExpression(expressionFromTerm(args[0]), undefined, args[1] && isCtor(args[1], "ts:args") ? termArgs(args[1]).map(expressionFromTerm) : []);
    case "ts:new": return ts.factory.createNewExpression(expressionFromTerm(args[0]), undefined, args[1] && isCtor(args[1], "ts:args") ? termArgs(args[1]).map(expressionFromTerm) : []);
    default: throw new Error(`cannot compile operation ${term.name}`);
  }
}

function binaryTokenForCtor(name: string): ts.BinaryOperator | null {
  switch (name) {
    case "ts:add": return ts.SyntaxKind.PlusToken;
    case "ts:sub": return ts.SyntaxKind.MinusToken;
    case "ts:mul": return ts.SyntaxKind.AsteriskToken;
    case "ts:div": return ts.SyntaxKind.SlashToken;
    case "ts:mod": return ts.SyntaxKind.PercentToken;
    case "ts:eq": return ts.SyntaxKind.EqualsEqualsEqualsToken;
    case "ts:ne": return ts.SyntaxKind.ExclamationEqualsEqualsToken;
    case "ts:lt": return ts.SyntaxKind.LessThanToken;
    case "ts:le": return ts.SyntaxKind.LessThanEqualsToken;
    case "ts:gt": return ts.SyntaxKind.GreaterThanToken;
    case "ts:ge": return ts.SyntaxKind.GreaterThanEqualsToken;
    case "ts:and": return ts.SyntaxKind.AmpersandAmpersandToken;
    case "ts:or": return ts.SyntaxKind.BarBarToken;
    case "ts:nullish": return ts.SyntaxKind.QuestionQuestionToken;
    case "ts:bitand": return ts.SyntaxKind.AmpersandToken;
    case "ts:bitor": return ts.SyntaxKind.BarToken;
    case "ts:bitxor": return ts.SyntaxKind.CaretToken;
    case "ts:shl": return ts.SyntaxKind.LessThanLessThanToken;
    case "ts:shr": return ts.SyntaxKind.GreaterThanGreaterThanToken;
    case "ts:ushr": return ts.SyntaxKind.GreaterThanGreaterThanGreaterThanToken;
    default: return null;
  }
}

function stringValue(term: IrTerm | undefined, fallback: string): string {
  return term && term.kind === "const" && typeof term.value === "string" ? term.value : fallback;
}

function typeNodeForSort(sort: Sort | undefined): ts.TypeNode {
  const name = sort?.kind === "primitive" ? sort.name : "Any";
  switch (name) {
    case "Number": return ts.factory.createKeywordTypeNode(ts.SyntaxKind.NumberKeyword);
    case "Boolean": return ts.factory.createKeywordTypeNode(ts.SyntaxKind.BooleanKeyword);
    case "String": return ts.factory.createKeywordTypeNode(ts.SyntaxKind.StringKeyword);
    case "Unit": return ts.factory.createKeywordTypeNode(ts.SyntaxKind.VoidKeyword);
    default: return ts.factory.createKeywordTypeNode(ts.SyntaxKind.AnyKeyword);
  }
}

function sanitizeIdentifier(name: string): string {
  const sanitized = name.replace(/[^A-Za-z0-9_$]/g, "_");
  return /^[A-Za-z_$]/.test(sanitized) ? sanitized : `_${sanitized}`;
}
