import { Project, SyntaxKind, Node, SourceFile, FunctionLikeDeclaration, Block } from "ts-morph";

export interface InstrumentOptions {
  signalLine: number;
  captureNames: string[];
  /**
   * Name of the sandbox-global function the inserted snapshot call invokes.
   * The harness installs a function under this name and reads captures back.
   *
   * Must be unique per concurrent harness run. Two concurrent runs that
   * share a global-scope name clobber each other's snapshot arrays — one
   * run's instrumented code will call the other run's function, and the
   * first run sees zero snapshots.
   *
   * Optional. Default `__neurallog_snapshot__` is only safe for single-
   * instance synchronous tests of this module itself.
   */
  snapshotFnName?: string;
}

const DEFAULT_SNAPSHOT_FN = "__neurallog_snapshot__";

function isValidIdentifier(name: string): boolean {
  // Conservative: ASCII letters, digits, underscore; can't start with digit.
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);
}

export function instrumentForSnapshot(source: string, opts: InstrumentOptions): string {
  const snapshotFnName = opts.snapshotFnName ?? DEFAULT_SNAPSHOT_FN;
  if (!isValidIdentifier(snapshotFnName)) {
    throw new Error(
      `snapshotFnName must be a valid JS identifier; got ${JSON.stringify(snapshotFnName)}`,
    );
  }

  const project = new Project({ useInMemoryFileSystem: true });
  const file = project.createSourceFile("input.ts", source);

  const targetFn = findFunctionAtLine(file, opts.signalLine);
  if (!targetFn) return source;

  const fnName = getFunctionName(targetFn) || "<anonymous>";
  const body = ensureBlockBody(targetFn);
  if (!body) return source;

  const stmt = findStatementAtLine(body, opts.signalLine);
  if (!stmt) return source;

  const capturesObj = `{ ${opts.captureNames.join(", ")} }`;
  const snapshotCall = `${snapshotFnName}(${JSON.stringify(fnName)}, ${opts.signalLine}, ${capturesObj});`;

  stmt.replaceWithText((writer) => {
    writer.writeLine(snapshotCall);
    writer.write(stmt.getText());
  });

  return file.getFullText();
}

function findFunctionAtLine(file: SourceFile, line: number): FunctionLikeDeclaration | null {
  let best: FunctionLikeDeclaration | null = null;
  file.forEachDescendant((node) => {
    if (
      Node.isFunctionDeclaration(node) ||
      Node.isFunctionExpression(node) ||
      Node.isArrowFunction(node) ||
      Node.isMethodDeclaration(node)
    ) {
      const startLine = node.getStartLineNumber();
      const endLine = node.getEndLineNumber();
      if (startLine <= line && line <= endLine) {
        best = node;
      }
    }
  });
  return best;
}

function getFunctionName(fn: FunctionLikeDeclaration): string | undefined {
  if (Node.isFunctionDeclaration(fn)) return fn.getName();
  if (Node.isMethodDeclaration(fn)) return fn.getName();
  const parent = fn.getParent();
  if (parent && Node.isVariableDeclaration(parent)) return parent.getName();
  return undefined;
}

function ensureBlockBody(fn: FunctionLikeDeclaration): Block | null {
  const body = fn.getBody();
  if (!body) return null;
  if (Node.isBlock(body)) return body;
  if (Node.isArrowFunction(fn)) {
    // Expression body: replace the entire arrow function body with a block
    const expr = body.getText();
    // Use replaceWithText on the body node to convert expression to block
    body.replaceWithText(`{\n    return ${expr};\n}`);
    const newBody = fn.getBody();
    return Node.isBlock(newBody!) ? (newBody as Block) : null;
  }
  return null;
}

function findStatementAtLine(body: Block, line: number) {
  const statements = body.getStatements();
  for (const s of statements) {
    if (s.getStartLineNumber() <= line && line <= s.getEndLineNumber()) {
      return s;
    }
  }
  return statements.length ? statements[statements.length - 1] : null;
}
