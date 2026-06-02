import { existsSync, readFileSync, realpathSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import ts from "typescript";

import { canonicalJsonString } from "../../claimEnvelope/canonicalize.js";
import { computeCid } from "../../canonicalizer/hash.js";
import type { IrFormula, IrTerm, Sort, VarTerm } from "../../ir/formulas.js";
import { decodeProofEnvelope } from "../../proofEnvelope/index.js";

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
  panicLoci?: TypeScriptSourcePanicLocus[];
  locus: { file: string; line: number; col: number };
  autoMintedMementos: unknown[];
}

export interface TypeScriptSourcePanicLocus {
  effectKind: "concept:panic-freedom";
  callee: "concept:panic-freedom.leaf.runtime-failure-site";
  subkind: "explicit-throw";
  argTerm: IrTerm;
  file: string;
  line: number;
  col: number;
}

export interface TypeScriptLibrarySugarBindingEntry {
  kind: "library-sugar-binding-entry";
  target_language: "typescript";
  target_library_tag: string;
  concept_name: string;
  source_function_name: string;
  param_names: string[];
  param_types: string[];
  return_type: string;
  term_shape: Record<string, unknown> | null;
  term_shape_cid: string | null;
  signature_shape_cid: string;
  loss_record_contribution: { form: string; value: { entries: unknown[] } };
  observed_dimension?: string;
  body_source: {
    file: string;
    span: { start_line: number; start_col: number; end_line: number; end_col: number };
    source_cid: string;
    body_text: string;
    ast_template: unknown;
    template_cid: string;
    param_names: string[];
  };
}

export interface TypeScriptLibraryRefusalMemento {
  kind: "refusal-memento";
  target_language: "typescript";
  surface: string;
  concept: string;
  reason: string;
  would_close_with_cluster: string;
}

export interface TypeScriptLibraryBindingsLiftResult extends TypeScriptSourceLiftResult {
  libraryBindings: TypeScriptLibrarySugarBindingEntry[];
  libraryRefusals: TypeScriptLibraryRefusalMemento[];
}

export interface TypeScriptSourceLiftResult {
  declarations: FunctionContractMemento[];
  diagnostics: TypeScriptSourceDiagnostic[];
  opacityReport: unknown[];
  refusals: TypeScriptSourceRefusal[];
}

export interface TypeScriptBindingTemplate {
  concept_name?: unknown;
  library_tag?: unknown;
  family?: unknown;
  ast_template?: unknown;
  template_cid?: unknown;
  param_names?: unknown;
  contract_cid?: unknown;
  target_proof_cid?: unknown;
}

export interface TypeScriptRecognizeParams {
  project_root: string;
  source_paths: string[];
  /// Direct templates are kept for focused kit tests. The manifest/RPC path
  /// resolves templates inside the kit from project source/proof context.
  binding_templates?: TypeScriptBindingTemplate[];
}

export interface TypeScriptRecognizeTag {
  file: string;
  span: { start_line: number; start_col: number; end_line: number; end_col: number };
  function_name: string;
  concept_name: unknown;
  library_tag: unknown;
  family: unknown;
  template_cid: string;
  contract_cid: unknown;
  target_proof_cid: unknown;
  match_tier: "exact";
  param_bindings: { index: number; source_text: string }[];
}

export interface TypeScriptRecognizeResponse {
  tags: TypeScriptRecognizeTag[];
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
  panicLoci: TypeScriptSourcePanicLocus[];
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
const RETURN_VALUE: VarTerm = { kind: "var", name: "return_value" };
const RUNTIME_FAILURE_SITE_CONCEPT = "concept:panic-freedom.leaf.runtime-failure-site";
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
const PROOF_FILE_RE = /^blake3-512:[0-9a-f]{128}\.proof$/i;

export function liftTypeScriptSourceText(
  sourceText: string,
  fileName = "input.ts",
): TypeScriptSourceLiftResult {
  const modulePath = normalizePath(fileName);
  const { program, sourceFile } = createProgramFromText(sourceText, modulePath);
  return liftSourceFile(sourceFile, program.getTypeChecker(), modulePath);
}

export function liftTypeScriptLibraryBindingsText(
  sourceText: string,
  fileName = "input.ts",
): TypeScriptLibraryBindingsLiftResult {
  const modulePath = normalizePath(fileName);
  const { program, sourceFile } = createProgramFromText(sourceText, modulePath);
  return liftLibraryBindingsSourceFile(sourceFile, program.getTypeChecker(), modulePath);
}

export function liftTypeScriptSourcePaths(
  workspaceRoot: string,
  sourcePaths: string[],
): TypeScriptSourceLiftResult {
  const { root, files, diagnostics, refusals } = collectWorkspaceSourceFiles(workspaceRoot, sourcePaths);

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

export function liftTypeScriptLibraryBindingsPaths(
  workspaceRoot: string,
  sourcePaths: string[],
): TypeScriptLibraryBindingsLiftResult {
  const { root, files, diagnostics, refusals } = collectWorkspaceSourceFiles(workspaceRoot, sourcePaths);

  if (files.length === 0) {
    return { declarations: [], diagnostics, opacityReport: [], refusals, libraryBindings: [], libraryRefusals: [] };
  }

  const program = ts.createProgram(files, compilerOptions());
  const checker = program.getTypeChecker();
  const libraryBindings: TypeScriptLibrarySugarBindingEntry[] = [];
  const libraryRefusals: TypeScriptLibraryRefusalMemento[] = [];
  for (const file of files.sort()) {
    const sourceFile = program.getSourceFile(file);
    if (!sourceFile) {
      diagnostics.push({ severity: "error", message: `program did not load ${file}` });
      continue;
    }
    const modulePath = normalizePath(relative(root, file));
    const lifted = liftLibraryBindingsSourceFile(sourceFile, checker, modulePath);
    libraryBindings.push(...lifted.libraryBindings);
    libraryRefusals.push(...lifted.libraryRefusals);
    diagnostics.push(...lifted.diagnostics);
    refusals.push(...lifted.refusals);
  }
  return { declarations: [], diagnostics, opacityReport: [], refusals, libraryBindings, libraryRefusals };
}

export function recognizeTypeScriptSources(params: TypeScriptRecognizeParams): TypeScriptRecognizeResponse {
  if (!params.project_root) throw new Error("missing `project_root`");
  if (!Array.isArray(params.source_paths)) throw new Error("missing `source_paths` array");
  const root = resolve(params.project_root);
  const suppliedTemplates = params.binding_templates ?? [];
  const selfResolvedBindings = liftTypeScriptLibraryBindingsPaths(root, params.source_paths).libraryBindings;
  const sugarTemplateFiles = new Set(selfResolvedBindings.map((entry) => entry.body_source.file));
  const templatePool = [
    ...suppliedTemplates,
    ...selfResolvedBindings.map(bindingTemplateFromSugarEntry),
    ...loadBindingTemplatesForProject(root),
  ];
  const bindingsByCid = bindingTemplatesByCid(templatePool);
  const tags: TypeScriptRecognizeTag[] = [];

  for (const sourcePath of params.source_paths) {
    const fullPath = resolve(root, sourcePath);
    if (!isInsideRoot(root, fullPath) || !existsSync(fullPath)) continue;
    const st = statSync(fullPath);
    const files = st.isDirectory() ? enumerateSourceFiles(fullPath) : st.isFile() && isSourceFile(fullPath) ? [fullPath] : [];
    for (const file of files.sort()) {
      const sourceText = readFileSync(file, "utf8");
      const relPath = normalizePath(relative(root, file));
      if (sugarTemplateFiles.has(relPath)) continue;
      tags.push(...recognizeTypeScriptSourcesText(sourceText, relPath, [...bindingsByCid.values()]).tags);
    }
  }

  return { tags };
}

function bindingTemplateFromSugarEntry(entry: TypeScriptLibrarySugarBindingEntry): TypeScriptBindingTemplate {
  return {
    concept_name: entry.concept_name,
    library_tag: entry.target_library_tag,
    family: (entry as unknown as { family?: unknown }).family,
    ast_template: entry.body_source.ast_template,
    template_cid: entry.body_source.template_cid,
    param_names: entry.body_source.param_names,
    contract_cid: (entry as unknown as { contract_cid?: unknown }).contract_cid ?? null,
    target_proof_cid: (entry as unknown as { target_proof_cid?: unknown }).target_proof_cid ?? null,
  };
}

function loadBindingTemplatesForProject(projectRoot: string): TypeScriptBindingTemplate[] {
  const templates: TypeScriptBindingTemplate[] = [];
  for (const proofPath of resolveDependencyProofPaths(projectRoot)) {
    templates.push(...bindingTemplatesFromProof(proofPath));
  }
  return templates;
}

function bindingTemplatesFromProof(proofPath: string): TypeScriptBindingTemplate[] {
  const proofBytes = readFileSync(proofPath);
  const proofCid = computeCid(proofBytes);
  const catalog = decodeProofEnvelope(new Uint8Array(proofBytes));
  const templates: TypeScriptBindingTemplate[] = [];

  for (const memberBytes of catalog.members.values()) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(Buffer.from(memberBytes).toString("utf8"));
    } catch {
      continue;
    }
    const body = isRecord(parsed) && isRecord(parsed.body) ? parsed.body : parsed;
    const template = bindingTemplateFromProofSugarEntry(body, proofCid);
    if (template) templates.push(template);
  }

  return templates;
}

function bindingTemplateFromProofSugarEntry(raw: unknown, proofCid: string): TypeScriptBindingTemplate | null {
  if (!isRecord(raw) || raw.kind !== "library-sugar-binding-entry") return null;
  const targetLanguage = raw.target_language;
  if (typeof targetLanguage === "string" && targetLanguage !== "typescript") return null;
  if (typeof raw.concept_name !== "string" || raw.concept_name.length === 0) return null;
  const bodySource = isRecord(raw.body_source) ? raw.body_source : null;
  if (!bodySource || bodySource.ast_template === undefined || bodySource.ast_template === null) return null;

  const templateCid = typeof bodySource.template_cid === "string" && bodySource.template_cid.length > 0
    ? bodySource.template_cid
    : cidOfValue(bodySource.ast_template);

  return {
    concept_name: raw.concept_name,
    library_tag: typeof raw.target_library_tag === "string" ? raw.target_library_tag : null,
    family: raw.family ?? null,
    ast_template: bodySource.ast_template,
    template_cid: templateCid,
    param_names: stringArray(bodySource.param_names) ?? stringArray(raw.param_names) ?? [],
    contract_cid: typeof raw.contract_cid === "string" ? raw.contract_cid : null,
    target_proof_cid: proofCid,
  };
}

function resolveDependencyProofPaths(projectRoot: string): string[] {
  const root = resolve(projectRoot);
  const proofPaths = new Set<string>();
  const visitedNodeModules = new Set<string>();
  const visitedPackages = new Set<string>();
  walkNodeModules(join(root, "node_modules"), { proofPaths, visitedNodeModules, visitedPackages });
  return [...proofPaths].sort();
}

interface DependencyProofWalkState {
  proofPaths: Set<string>;
  visitedNodeModules: Set<string>;
  visitedPackages: Set<string>;
}

function walkNodeModules(nodeModulesDir: string, state: DependencyProofWalkState): void {
  const realNodeModules = realDirectoryPath(nodeModulesDir);
  if (realNodeModules === null || state.visitedNodeModules.has(realNodeModules)) return;
  state.visitedNodeModules.add(realNodeModules);

  let entries;
  try {
    entries = readdirSync(realNodeModules, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue;
    const entryPath = join(realNodeModules, entry.name);
    if (entry.name.startsWith("@")) {
      walkScopedPackages(entryPath, state);
      continue;
    }
    collectPackage(entryPath, state);
  }
}

function walkScopedPackages(scopeDir: string, state: DependencyProofWalkState): void {
  const realScopeDir = realDirectoryPath(scopeDir);
  if (realScopeDir === null) return;

  let entries;
  try {
    entries = readdirSync(realScopeDir, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    collectPackage(join(realScopeDir, entry.name), state);
  }
}

function collectPackage(packageDir: string, state: DependencyProofWalkState): void {
  const realPackageDir = realDirectoryPath(packageDir);
  if (realPackageDir === null || state.visitedPackages.has(realPackageDir)) return;
  state.visitedPackages.add(realPackageDir);

  collectPackageProofFiles(realPackageDir, state.proofPaths);
  walkNodeModules(join(realPackageDir, "node_modules"), state);
}

function collectPackageProofFiles(packageDir: string, proofPaths: Set<string>): void {
  let entries;
  try {
    entries = readdirSync(packageDir, { withFileTypes: true });
  } catch {
    return;
  }

  for (const entry of entries) {
    const entryPath = join(packageDir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === ".git") continue;
      collectPackageProofFiles(entryPath, proofPaths);
      continue;
    }
    if (entry.isFile() && PROOF_FILE_RE.test(entry.name)) {
      proofPaths.add(entryPath);
    }
  }
}

function realDirectoryPath(candidate: string): string | null {
  try {
    const realPath = realpathSync(candidate);
    return statSync(realPath).isDirectory() ? realPath : null;
  } catch {
    return null;
  }
}

function stringArray(raw: unknown): string[] | null {
  if (!Array.isArray(raw)) return null;
  const out = raw.filter((item): item is string => typeof item === "string");
  return out.length === raw.length ? out : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function recognizeTypeScriptSourcesText(
  sourceText: string,
  fileName = "input.ts",
  bindingTemplates: TypeScriptBindingTemplate[] = [],
): TypeScriptRecognizeResponse {
  const modulePath = normalizePath(fileName);
  const sourceFile = ts.createSourceFile(modulePath, sourceText, ts.ScriptTarget.ES2022, true, scriptKind(modulePath));
  const bindingsByCid = bindingTemplatesByCid(bindingTemplates);
  const tags: TypeScriptRecognizeTag[] = [];

  const visit = (node: ts.Node): void => {
    if (isRecognizableFunctionLike(node)) {
      const tag = recognizeFunctionLike(sourceFile, modulePath, node, bindingsByCid);
      if (tag) tags.push(tag);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  return { tags };
}

type RecognizableFunctionLike = ts.FunctionDeclaration | ts.MethodDeclaration;

function bindingTemplatesByCid(bindingTemplates: readonly TypeScriptBindingTemplate[]): Map<string, TypeScriptBindingTemplate> {
  const out = new Map<string, TypeScriptBindingTemplate>();
  for (const binding of bindingTemplates) {
    if (!binding || typeof binding !== "object") continue;
    const cid = binding.template_cid;
    if (typeof cid === "string" && cid.length > 0) out.set(cid, binding);
  }
  return out;
}

function isRecognizableFunctionLike(node: ts.Node): node is RecognizableFunctionLike {
  if (ts.canHaveDecorators(node) && (ts.getDecorators(node) ?? []).some(isSugarBindDecorator)) {
    return false;
  }
  if (ts.isFunctionDeclaration(node)) return !!node.name && !!node.body;
  if (ts.isMethodDeclaration(node)) return !!methodNameText(node.name) && !!node.body;
  return false;
}

function isSugarBindDecorator(decorator: ts.Decorator): boolean {
  const expr = decorator.expression;
  if (!ts.isCallExpression(expr)) return false;
  const callee = expr.expression;
  return ts.isPropertyAccessExpression(callee)
    && callee.name.text === "bind"
    && ts.isIdentifier(callee.expression)
    && callee.expression.text === "sugar";
}

function recognizeFunctionLike(
  sourceFile: ts.SourceFile,
  modulePath: string,
  node: RecognizableFunctionLike,
  bindingsByCid: ReadonlyMap<string, TypeScriptBindingTemplate>,
): TypeScriptRecognizeTag | null {
  if (!node.body) return null;
  const paramNames = node.parameters.map((param) => parameterName(param));
  const astTemplate = functionBodyAstTemplate(node.body, paramNames);
  const templateCid = cidOfValue(astTemplate);
  const binding = bindingsByCid.get(templateCid);
  if (!binding) return null;

  return {
    file: modulePath,
    span: locatorForNode(node, sourceFile),
    function_name: recognizableFunctionName(node),
    concept_name: binding.concept_name ?? null,
    library_tag: binding.library_tag ?? null,
    family: binding.family ?? null,
    template_cid: templateCid,
    contract_cid: binding.contract_cid ?? null,
    target_proof_cid: binding.target_proof_cid ?? null,
    match_tier: "exact",
    param_bindings: paramNames.map((name, index) => ({ index: index + 1, source_text: name })),
  };
}

function recognizableFunctionName(node: RecognizableFunctionLike): string {
  if (ts.isFunctionDeclaration(node)) return node.name?.text ?? "";
  return methodNameText(node.name) ?? "";
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

function liftLibraryBindingsSourceFile(
  sourceFile: ts.SourceFile,
  checker: ts.TypeChecker,
  modulePath: string,
): TypeScriptLibraryBindingsLiftResult {
  const libraryBindings: TypeScriptLibrarySugarBindingEntry[] = [];
  const libraryRefusals: TypeScriptLibraryRefusalMemento[] = [];

  const fileContext: FileLiftContext = {
    modulePath,
    sourceFile,
    checker,
    moduleVars: new Set(),
    knownCallables: new Set(),
    refusals: [],
  };

  const visit = (node: ts.Node): void => {
    if (ts.isFunctionDeclaration(node) && node.name && node.body) {
      const binding = libraryBindingEntryForFunction(node, sourceFile, modulePath, fileContext);
      if (binding) libraryBindings.push(binding);
    }
    if (ts.isClassDeclaration(node)) {
      const refusal = libraryRefusalEntryForClass(node, modulePath);
      if (refusal) libraryRefusals.push(refusal);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  return { declarations: [], diagnostics: [], opacityReport: [], refusals: [], libraryBindings, libraryRefusals };
}

function libraryBindingEntryForFunction(
  node: ts.FunctionDeclaration,
  sourceFile: ts.SourceFile,
  modulePath: string,
  fileContext: FileLiftContext,
): TypeScriptLibrarySugarBindingEntry | null {
  const binding = sugarBindingArgs(node);
  if (!binding) return null;
  const paramNames = node.parameters.map((param) => parameterName(param));
  const paramTypes = node.parameters.map((param) => param.type?.getText(sourceFile) ?? "unknown");
  const returnType = node.type?.getText(sourceFile) ?? "unknown";
  const signatureShape = {
    param_names: paramNames,
    param_types: paramTypes,
    return_type: returnType,
  };

  // Attempt to lift the body term using existing machinery, bypassing the decorator
  // guard since decorated functions are intentional for library sugar bindings.
  let termShape: Record<string, unknown> | null = null;
  let termShapeCid: string | null = null;
  const body = node.body;
  if (body) {
    try {
      const formals = node.parameters.map((param) => parameterName(param));
      const functionContext: FunctionContext = {
        ...fileContext,
        functionName: `${modulePath}:${node.name?.text ?? "unknown"}`,
        locals: new Set<string>(formals),
        effects: new Map(),
        panicLoci: [],
      };
      collectLocalDeclarations(body, functionContext.locals);
      const rawBodyTerm = singleReturnExpression(body)
        ? emitExpression(singleReturnExpression(body)!, functionContext)
        : emitBlock(body, functionContext);
      termShape = rawBodyTerm as unknown as Record<string, unknown>;
      termShapeCid = cidOfValue(termShape);
    } catch {
      // Body lifting failed; term_shape remains null and loss_record_contribution tracks the debt.
    }
  }

  const span = locatorForNode(node, sourceFile);
  const spanText = sourceFile.getFullText().slice(node.getStart(sourceFile), node.end);
  // Substrate-honest body capture (mirrors the Java lifter): the
  // `@sugar.bind(...)` decorator + signature + braces are presentation/sugar.
  // The lifter has already read the decorator (concept/library/family/version)
  // and the signature (param_names/param_types/return_type) into typed fields.
  // body_text carries only the remaining substance: the statements between the
  // function block's outermost `{` and matching `}`, with original indentation
  // preserved (the realizer does its own indentation pass). When no block body
  // is present, body_text is empty and the entry contributes no template.
  const bodyText = extractFunctionBodyStatements(body, sourceFile);
  const astTemplate = functionBodyAstTemplate(body, paramNames);
  const entry: TypeScriptLibrarySugarBindingEntry = {
    kind: "library-sugar-binding-entry",
    target_language: "typescript",
    target_library_tag: binding.library,
    concept_name: binding.concept,
    source_function_name: node.name?.text ?? "",
    param_names: paramNames,
    param_types: paramTypes,
    return_type: returnType,
    term_shape: termShape,
    term_shape_cid: termShapeCid,
    signature_shape_cid: cidOfValue(signatureShape),
    loss_record_contribution: {
      form: "literal",
      value: { entries: binding.loss },
    },
    body_source: {
      file: modulePath,
      span,
      source_cid: computeCid(Buffer.from(spanText, "utf8")),
      body_text: bodyText,
      ast_template: astTemplate,
      template_cid: cidOfValue(astTemplate),
      param_names: paramNames,
    },
  };
  if (binding.observed_dimension !== null) entry.observed_dimension = binding.observed_dimension;
  // #1357 / #1355: surface optional family + version pins on the binding
  // entry. Absent on the @sugar.bind decorator means absent in emitted JSON
  // (NOT empty strings; null/missing is the substrate signal for
  // "this axis floats"). Parallel to walk_rpc's rust-side emission.
  if (binding.family !== null) (entry as unknown as Record<string, unknown>).family = binding.family;
  if (binding.version !== null) (entry as unknown as Record<string, unknown>).library_version = binding.version;
  return entry;
}

/**
 * Extract only the statements inside a function's block body: the text between
 * the block's outermost `{` and matching `}`, dedented to column 0. The
 * decorator + signature + braces are sugar already captured as typed fields;
 * body_text carries what the function DOES. Returns "" when there is no block
 * body (so the binding entry contributes no emission template).
 *
 * Dedent rationale: the shim source indents method bodies (e.g. 2 spaces inside
 * the function block). That source-level indentation is presentation, not
 * substance; the realizer's functionSource / emission template owns body
 * indentation and the materialize indent pass adds the carrier's column on top.
 * Capturing body_text at column 0 keeps it byte-equal to the canonical
 * de-indented emission template the central body-templates JSON shipped before
 * it was deleted, so realized output is indented exactly once (not once per
 * source-nesting level). Without this, a 2-space carrier produced a 6-space body
 * instead of the expected 4.
 */
function extractFunctionBodyStatements(
  body: ts.Block | undefined,
  sourceFile: ts.SourceFile,
): string {
  if (!body || !ts.isBlock(body)) return "";
  const full = sourceFile.getFullText();
  // body.getStart()/getEnd() bound the block including its braces; slice the
  // interior. getStart() points at the `{`, getEnd() one past the `}`.
  const interiorStart = body.getStart(sourceFile) + 1;
  const interiorEnd = body.getEnd() - 1;
  if (interiorEnd <= interiorStart) return "";
  // Strip a single leading newline (after `{`) and trailing whitespace before
  // `}` so the captured text is the statement block without the brace lines.
  let text = full.slice(interiorStart, interiorEnd);
  text = text.replace(/^\r?\n/, "").replace(/\s+$/, "");
  return dedentCommonIndent(text);
}

/**
 * Strip the longest whitespace prefix shared by every non-blank line. Blank
 * lines are ignored when computing the common prefix and are left untouched.
 * The common prefix is matched character-for-character (tabs and spaces
 * literal), so mixed indentation degrades gracefully to a shorter prefix.
 */
function dedentCommonIndent(text: string): string {
  const lines = text.split("\n");
  let common: string | null = null;
  for (const line of lines) {
    if (line.trim() === "") continue;
    const indent = line.slice(0, line.length - line.trimStart().length);
    if (common === null) {
      common = indent;
      continue;
    }
    let i = 0;
    const max = Math.min(common.length, indent.length);
    while (i < max && common[i] === indent[i]) i += 1;
    common = common.slice(0, i);
    if (common === "") break;
  }
  if (!common) return text;
  const prefix = common;
  return lines
    .map((line) => (line.startsWith(prefix) ? line.slice(prefix.length) : line))
    .join("\n");
}

function functionBodyAstTemplate(body: ts.Block | undefined, params: readonly string[]): unknown {
  return {
    kind: "block",
    stmts: body ? body.statements.flatMap((stmt) => stmtToAstTemplates(stmt, params)) : [],
  };
}

function stmtToAstTemplates(stmt: ts.Statement, params: readonly string[]): unknown[] {
  if (ts.isVariableStatement(stmt)) {
    return stmt.declarationList.declarations.map((decl) => ({
      kind: "let",
      pat: bindingNameToAstTemplate(decl.name, params),
      init: decl.initializer ? exprToAstTemplate(decl.initializer, params) : null,
    }));
  }
  if (ts.isExpressionStatement(stmt)) {
    return [{
      kind: "expr_stmt",
      expr: exprToAstTemplate(stmt.expression, params),
      trailing_semi: true,
    }];
  }
  if (ts.isReturnStatement(stmt)) {
    return [{
      kind: "return",
      expr: stmt.expression ? exprToAstTemplate(stmt.expression, params) : null,
    }];
  }
  if (ts.isBlock(stmt)) {
    return [functionBodyAstTemplate(stmt, params)];
  }
  return [otherAstTemplate(stmt)];
}

function exprToAstTemplate(expr: ts.Expression, params: readonly string[]): unknown {
  if (ts.isIdentifier(expr)) return identifierAstTemplate(expr.text, params);
  if (expr.kind === ts.SyntaxKind.ThisKeyword) return { kind: "ident", name: "this" };
  if (expr.kind === ts.SyntaxKind.NullKeyword) return { kind: "lit", ty: "null", value: null };
  if (expr.kind === ts.SyntaxKind.TrueKeyword) return { kind: "lit", ty: "bool", value: true };
  if (expr.kind === ts.SyntaxKind.FalseKeyword) return { kind: "lit", ty: "bool", value: false };
  if (ts.isStringLiteralLike(expr)) return { kind: "lit", ty: "str", value: expr.text };
  if (ts.isNumericLiteral(expr)) return { kind: "lit", ty: "number", value: Number(expr.text) };
  if (ts.isParenthesizedExpression(expr)) return exprToAstTemplate(expr.expression, params);
  if (ts.isArrayLiteralExpression(expr)) {
    return { kind: "array", elems: expr.elements.map((element) => exprToAstTemplate(element as ts.Expression, params)) };
  }
  if (ts.isObjectLiteralExpression(expr)) {
    return {
      kind: "object",
      fields: expr.properties.map((prop) => objectPropertyAstTemplate(prop, params)),
    };
  }
  if (ts.isCallExpression(expr)) {
    const args = expr.arguments.map((arg) => exprToAstTemplate(arg, params));
    if (ts.isPropertyAccessExpression(expr.expression)) {
      return {
        kind: "method_call",
        receiver: exprToAstTemplate(expr.expression.expression, params),
        method: expr.expression.name.text,
        args,
      };
    }
    return {
      kind: "call",
      func: exprToAstTemplate(expr.expression, params),
      args,
    };
  }
  if (ts.isPropertyAccessExpression(expr)) {
    const field = fieldAstTemplateIfParamRoot(expr, params);
    if (field) return field;
    const segments = propertyAccessSegments(expr);
    return segments ? { kind: "path", segments } : {
      kind: "field",
      base: exprToAstTemplate(expr.expression, params),
      member: expr.name.text,
    };
  }
  if (ts.isElementAccessExpression(expr)) {
    return {
      kind: "index",
      base: exprToAstTemplate(expr.expression, params),
      index: expr.argumentExpression ? exprToAstTemplate(expr.argumentExpression, params) : null,
    };
  }
  if (ts.isBinaryExpression(expr)) {
    const op = binaryOpTemplateName(expr.operatorToken.kind);
    return op
      ? {
          kind: "binary",
          op,
          left: exprToAstTemplate(expr.left, params),
          right: exprToAstTemplate(expr.right, params),
        }
      : otherAstTemplate(expr);
  }
  if (ts.isPrefixUnaryExpression(expr)) {
    return {
      kind: "unary",
      op: ts.SyntaxKind[expr.operator] ?? String(expr.operator),
      expr: exprToAstTemplate(expr.operand, params),
    };
  }
  if (ts.isAsExpression(expr) || ts.isTypeAssertionExpression(expr) || ts.isNonNullExpression(expr)) {
    return exprToAstTemplate(expr.expression, params);
  }
  return otherAstTemplate(expr);
}

function objectPropertyAstTemplate(prop: ts.ObjectLiteralElementLike, params: readonly string[]): unknown {
  if (ts.isPropertyAssignment(prop)) {
    return {
      kind: "field",
      name: propertyNameText(prop.name),
      value: exprToAstTemplate(prop.initializer, params),
    };
  }
  if (ts.isShorthandPropertyAssignment(prop)) {
    return {
      kind: "field",
      name: prop.name.text,
      value: identifierAstTemplate(prop.name.text, params),
    };
  }
  return otherAstTemplate(prop);
}

function bindingNameToAstTemplate(name: ts.BindingName, params: readonly string[]): unknown {
  if (ts.isIdentifier(name)) {
    if (paramIndex(name.text, params) > 0) return { kind: "param_ref", index: paramIndex(name.text, params) };
    return { kind: "binding", name: name.text };
  }
  return {
    kind: "pat_tuple",
    elems: name.elements.map((element) => {
      if (ts.isOmittedExpression(element)) return { kind: "wildcard" };
      return bindingNameToAstTemplate(element.name, params);
    }),
  };
}

function identifierAstTemplate(name: string, params: readonly string[]): unknown {
  switch (name) {
    case "undefined":
      return { kind: "lit", ty: "undefined", value: null };
    case "NaN":
    case "Infinity":
      return { kind: "ident", name };
    default: {
      const index = paramIndex(name, params);
      return index > 0 ? { kind: "param_ref", index } : { kind: "ident", name };
    }
  }
}

function paramIndex(name: string, params: readonly string[]): number {
  const index = params.indexOf(name);
  return index < 0 ? 0 : index + 1;
}

function propertyAccessSegments(expr: ts.PropertyAccessExpression): string[] | null {
  const segments: string[] = [expr.name.text];
  let current: ts.Expression = expr.expression;
  while (ts.isPropertyAccessExpression(current)) {
    segments.unshift(current.name.text);
    current = current.expression;
  }
  if (ts.isIdentifier(current)) {
    segments.unshift(current.text);
    return segments;
  }
  if (current.kind === ts.SyntaxKind.ThisKeyword) {
    segments.unshift("this");
    return segments;
  }
  return null;
}

function fieldAstTemplateIfParamRoot(expr: ts.PropertyAccessExpression, params: readonly string[]): unknown | null {
  const members: string[] = [];
  let current: ts.Expression = expr;
  while (ts.isPropertyAccessExpression(current)) {
    members.unshift(current.name.text);
    current = current.expression;
  }
  if (!ts.isIdentifier(current)) return null;
  const index = paramIndex(current.text, params);
  if (index === 0) return null;
  let result: unknown = { kind: "param_ref", index };
  for (const member of members) {
    result = { kind: "field", base: result, member };
  }
  return result;
}

function propertyNameText(name: ts.PropertyName): string {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) return name.text;
  return name.getText();
}

function binaryOpTemplateName(kind: ts.SyntaxKind): string | null {
  switch (kind) {
    case ts.SyntaxKind.PlusToken: return "Add";
    case ts.SyntaxKind.MinusToken: return "Sub";
    case ts.SyntaxKind.AsteriskToken: return "Mul";
    case ts.SyntaxKind.SlashToken: return "Div";
    case ts.SyntaxKind.PercentToken: return "Rem";
    case ts.SyntaxKind.AmpersandToken: return "BitAnd";
    case ts.SyntaxKind.BarToken: return "BitOr";
    case ts.SyntaxKind.CaretToken: return "BitXor";
    case ts.SyntaxKind.LessThanLessThanToken: return "Shl";
    case ts.SyntaxKind.GreaterThanGreaterThanToken: return "Shr";
    case ts.SyntaxKind.AmpersandAmpersandToken: return "And";
    case ts.SyntaxKind.BarBarToken: return "Or";
    case ts.SyntaxKind.EqualsEqualsEqualsToken:
    case ts.SyntaxKind.EqualsEqualsToken: return "Eq";
    case ts.SyntaxKind.ExclamationEqualsEqualsToken:
    case ts.SyntaxKind.ExclamationEqualsToken: return "Ne";
    case ts.SyntaxKind.LessThanToken: return "Lt";
    case ts.SyntaxKind.LessThanEqualsToken: return "Le";
    case ts.SyntaxKind.GreaterThanEqualsToken: return "Ge";
    case ts.SyntaxKind.GreaterThanToken: return "Gt";
    default: return null;
  }
}

function otherAstTemplate(node: ts.Node): unknown {
  return { kind: "other", variant: syntaxKindName(node) };
}

function sugarBindingArgs(node: ts.FunctionDeclaration): {
  concept: string;
  library: string;
  loss: string[];
  observed_dimension: string | null;
  // #1357 / #1355: optional family + version pins, parallel to the
  // walk_rpc rust lifter. Both float (null) when @sugar.bind omits them;
  // the dispatch downstream narrows via these when present.
  family: string | null;
  version: string | null;
} | null {
  const decorators = decoratorNodes(node as unknown as ts.HasDecorators);
  for (const decorator of decorators) {
    const expr = decorator.expression;
    if (!ts.isCallExpression(expr) || !isSugarBindExpression(expr.expression)) continue;
    const first = expr.arguments[0];
    if (!first || !ts.isObjectLiteralExpression(first)) continue;
    const concept = stringProperty(first, "concept");
    const library = stringProperty(first, "library");
    if (concept && library) {
      const loss = arrayStringProperty(first, "loss");
      const observed_dimension = stringProperty(first, "observed_dimension");
      const family = stringProperty(first, "family");
      const version = stringProperty(first, "version");
      return { concept, library, loss, observed_dimension, family, version };
    }
  }
  return null;
}

function decoratorNodes(node: ts.HasDecorators): ts.Decorator[] {
  if (ts.canHaveDecorators(node)) return [...(ts.getDecorators(node) ?? [])];
  return [...((node as ts.FunctionDeclaration).modifiers ?? [])].filter((modifier): modifier is ts.Decorator => ts.isDecorator(modifier));
}

function isSugarBindExpression(expr: ts.Expression): boolean {
  if (!ts.isPropertyAccessExpression(expr) || expr.name.text !== "bind") return false;
  const receiver = expr.expression;
  return ts.isIdentifier(receiver) && receiver.text === "sugar";
}

function isSugarRefuseExpression(expr: ts.Expression): boolean {
  if (!ts.isPropertyAccessExpression(expr) || expr.name.text !== "refuse") return false;
  const receiver = expr.expression;
  return ts.isIdentifier(receiver) && receiver.text === "sugar";
}

function libraryRefusalEntryForClass(
  node: ts.ClassDeclaration,
  modulePath: string,
): TypeScriptLibraryRefusalMemento | null {
  const refuseArgs = sugarRefuseArgs(node);
  if (!refuseArgs) return null;
  return {
    kind: "refusal-memento",
    target_language: "typescript",
    surface: refuseArgs.surface,
    concept: refuseArgs.concept,
    reason: refuseArgs.reason,
    would_close_with_cluster: refuseArgs.would_close_with_cluster,
  };
}

function sugarRefuseArgs(node: ts.ClassDeclaration): { surface: string; concept: string; reason: string; would_close_with_cluster: string } | null {
  const decorators: ts.Decorator[] = ts.canHaveDecorators(node) ? [...(ts.getDecorators(node) ?? [])] : [];
  for (const decorator of decorators) {
    const expr = decorator.expression;
    if (!ts.isCallExpression(expr) || !isSugarRefuseExpression(expr.expression)) continue;
    const first = expr.arguments[0];
    if (!first || !ts.isObjectLiteralExpression(first)) continue;
    const surface = stringProperty(first, "surface");
    const concept = stringProperty(first, "concept");
    const reason = stringProperty(first, "reason");
    const would_close_with_cluster = stringProperty(first, "would_close_with_cluster");
    if (surface && concept && reason && would_close_with_cluster) {
      return { surface, concept, reason, would_close_with_cluster };
    }
  }
  return null;
}

function stringProperty(obj: ts.ObjectLiteralExpression, name: string): string | null {
  for (const prop of obj.properties) {
    if (!ts.isPropertyAssignment(prop)) continue;
    if (!ts.isIdentifier(prop.name) || prop.name.text !== name) continue;
    return ts.isStringLiteralLike(prop.initializer) ? prop.initializer.text : null;
  }
  return null;
}

function arrayStringProperty(obj: ts.ObjectLiteralExpression, name: string): string[] {
  for (const prop of obj.properties) {
    if (!ts.isPropertyAssignment(prop)) continue;
    if (!ts.isIdentifier(prop.name) || prop.name.text !== name) continue;
    if (!ts.isArrayLiteralExpression(prop.initializer)) return [];
    return prop.initializer.elements
      .filter(ts.isStringLiteralLike)
      .map((el) => el.text);
  }
  return [];
}

function locatorForNode(
  node: ts.Node,
  sourceFile: ts.SourceFile,
): { start_line: number; start_col: number; end_line: number; end_col: number } {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const end = sourceFile.getLineAndCharacterOfPosition(node.end);
  return {
    start_line: start.line + 1,
    start_col: start.character,
    end_line: end.line + 1,
    end_col: end.character,
  };
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
      panicLoci: [],
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
      ...(functionContext.panicLoci.length > 0 ? { panicLoci: [...functionContext.panicLoci] } : {}),
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
    const argTerm = stmt.expression ? emitExpression(stmt.expression, context) : unitConst();
    addEffect(context, { kind: "panics" });
    const start = sourcePosition(context.sourceFile, stmt);
    context.panicLoci.push({
      effectKind: "concept:panic-freedom",
      callee: RUNTIME_FAILURE_SITE_CONCEPT,
      subkind: "explicit-throw",
      argTerm,
      file: context.modulePath,
      line: start.line,
      col: start.col,
    });
    return ctor("ts:throw", argTerm);
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
  if (ts.isObjectLiteralExpression(expr)) return emitObjectLiteralExpression(expr, context);
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

function emitObjectLiteralExpression(expr: ts.ObjectLiteralExpression, context: FunctionContext): IrTerm {
  const properties: IrTerm[] = [];
  for (const property of expr.properties) {
    if (ts.isPropertyAssignment(property)) {
      const name = nameFromPropertyName(property.name);
      if (name === null) {
        throw new UnsupportedSyntaxError(property.name, "computed object literal property names are not handled");
      }
      if (name === "__proto__") {
        throw new UnsupportedSyntaxError(property.name, "__proto__ prototype setter object literal properties are not handled");
      }
      properties.push(ctor("ts:property", stringConst(name), emitExpression(property.initializer, context)));
      continue;
    }
    if (ts.isSpreadAssignment(property)) {
      throw new UnsupportedSyntaxError(property, "object literal spread properties are not handled");
    }
    if (ts.isShorthandPropertyAssignment(property)) {
      properties.push(ctor("ts:property", stringConst(property.name.text), emitExpression(property.name, context)));
      continue;
    }
    throw new UnsupportedSyntaxError(property, `object literal property kind ${syntaxKindName(property)} is not handled`);
  }
  return ctor("ts:object-literal", ...properties);
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

function sourcePosition(sourceFile: ts.SourceFile, node: ts.Node): { line: number; col: number } {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  return { line: start.line + 1, col: start.character };
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

function collectWorkspaceSourceFiles(
  workspaceRoot: string,
  sourcePaths: string[],
): {
  root: string;
  files: string[];
  diagnostics: TypeScriptSourceDiagnostic[];
  refusals: TypeScriptSourceRefusal[];
} {
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

  return { root, files, diagnostics, refusals };
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
    case "ts:object-literal": return ts.factory.createObjectLiteralExpression(args.map(objectLiteralPropertyFromTerm), false);
    default: throw new Error(`cannot compile operation ${term.name}`);
  }
}

function objectLiteralPropertyFromTerm(term: IrTerm): ts.ObjectLiteralElementLike {
  if (!isCtor(term, "ts:property")) {
    throw new Error(`cannot compile object literal property term ${term.kind === "ctor" ? term.name : term.kind}`);
  }
  const args = termArgs(term);
  const key = args[0];
  if (!key || key.kind !== "const" || typeof key.value !== "string") {
    throw new Error("cannot compile object literal property with non-string key");
  }
  const value = args[1];
  if (canEmitShorthandProperty(key.value, value)) {
    return ts.factory.createShorthandPropertyAssignment(key.value);
  }
  const propertyName = key.value === "__proto__"
    ? ts.factory.createComputedPropertyName(ts.factory.createStringLiteral(key.value))
    : ts.factory.createStringLiteral(key.value);
  return ts.factory.createPropertyAssignment(propertyName, expressionFromTerm(value));
}

function canEmitShorthandProperty(name: string, value: IrTerm | undefined): boolean {
  return value?.kind === "var" && value.name === name && /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name);
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
