/**
 * A7b: Hand-rolled recursive-descent parser for the provekit DSL.
 * No third-party PEG libraries.
 */

import type {
  Program,
  TopLevelNode,
  PrincipleNode,
  PredicateDef,
  MatchClause,
  WherePred,
  AtomPred,
  CapColRef,
  RHS,
  Literal,
  VarRef,
  VarDeref,
  RelationArg,
  RequireClause,
  ReportBlock,
  CaptureEntry,
  Severity,
  BuiltinRelation,
  SourceLoc,
} from "./ast.js";

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

type TK =
  | "IDENT"
  | "VAR"       // $ident
  | "STRING"
  | "NUMBER"
  | "DOT"
  | "LBRACE"
  | "RBRACE"
  | "LPAREN"
  | "RPAREN"
  | "COMMA"
  | "SEMI"
  | "COLON"
  | "EQEQ"
  | "EOF";

interface Token {
  type: TK;
  value: string;
  line: number;
  col: number;
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

export class ParseError extends Error {
  constructor(
    message: string,
    public line: number,
    public col: number,
  ) {
    super(`Parse error at ${line}:${col}: ${message}`);
    this.name = "ParseError";
  }
}

function tokenize(src: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  let line = 1;
  let col = 1;

  function advance(n = 1): void {
    for (let k = 0; k < n; k++) {
      if (src[i] === "\n") { line++; col = 1; }
      else { col++; }
      i++;
    }
  }

  function peek(offset = 0): string {
    return src[i + offset] ?? "";
  }

  while (i < src.length) {
    // Skip whitespace
    if (/\s/.test(src[i])) { advance(); continue; }

    // Line comments: // ...
    if (src[i] === "/" && peek(1) === "/") {
      while (i < src.length && src[i] !== "\n") advance();
      continue;
    }

    const startLine = line;
    const startCol = col;

    // String literal
    if (src[i] === '"') {
      advance();
      let val = "";
      while (i < src.length && src[i] !== '"') {
        if (src[i] === "\\") { advance(); val += src[i]; }
        else { val += src[i]; }
        advance();
      }
      if (i >= src.length) throw new ParseError("Unterminated string literal", startLine, startCol);
      advance(); // closing "
      tokens.push({ type: "STRING", value: val, line: startLine, col: startCol });
      continue;
    }

    // == operator
    if (src[i] === "=" && peek(1) === "=") {
      advance(2);
      tokens.push({ type: "EQEQ", value: "==", line: startLine, col: startCol });
      continue;
    }

    // Single-char tokens
    const singles: Record<string, TK> = {
      ".": "DOT", "{": "LBRACE", "}": "RBRACE",
      "(": "LPAREN", ")": "RPAREN", ",": "COMMA",
      ";": "SEMI", ":": "COLON",
    };
    if (singles[src[i]]) {
      const t = singles[src[i]] as TK;
      advance();
      tokens.push({ type: t, value: src[i - 1] || ".", line: startLine, col: startCol });
      continue;
    }

    // Number
    if (/[0-9]/.test(src[i])) {
      let val = "";
      while (i < src.length && /[0-9.]/.test(src[i])) { val += src[i]; advance(); }
      tokens.push({ type: "NUMBER", value: val, line: startLine, col: startCol });
      continue;
    }

    // Variable: $ident
    if (src[i] === "$") {
      advance();
      let val = "";
      while (i < src.length && /[a-zA-Z0-9_-]/.test(src[i])) { val += src[i]; advance(); }
      if (!val) throw new ParseError("Expected identifier after '$'", startLine, startCol);
      tokens.push({ type: "VAR", value: val, line: startLine, col: startCol });
      continue;
    }

    // Identifier (includes keywords)
    if (/[a-zA-Z_]/.test(src[i])) {
      let val = "";
      while (i < src.length && /[a-zA-Z0-9_-]/.test(src[i])) { val += src[i]; advance(); }
      tokens.push({ type: "IDENT", value: val, line: startLine, col: startCol });
      continue;
    }

    throw new ParseError(`Unexpected character '${src[i]}'`, startLine, startCol);
  }

  tokens.push({ type: "EOF", value: "", line, col });
  return tokens;
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

class Parser {
  private tokens: Token[];
  private pos = 0;

  constructor(tokens: Token[]) {
    this.tokens = tokens;
  }

  private peek(offset = 0): Token {
    return this.tokens[this.pos + offset] ?? { type: "EOF", value: "", line: 0, col: 0 };
  }

  private consume(): Token {
    const t = this.tokens[this.pos];
    this.pos++;
    return t;
  }

  private expect(type: TK, value?: string): Token {
    const t = this.peek();
    if (t.type !== type) {
      throw new ParseError(
        `Expected ${type}${value ? ` '${value}'` : ""} but got ${t.type} '${t.value}'`,
        t.line, t.col,
      );
    }
    if (value !== undefined && t.value !== value) {
      throw new ParseError(
        `Expected '${value}' but got '${t.value}'`,
        t.line, t.col,
      );
    }
    return this.consume();
  }

  private expectIdent(value?: string): Token {
    return this.expect("IDENT", value);
  }

  private expectVar(): Token {
    return this.expect("VAR");
  }

  private loc(): SourceLoc {
    const t = this.peek();
    return { line: t.line, col: t.col };
  }

  // ---------------------------------------------------------------------------

  parseProgram(): Program {
    const nodes: TopLevelNode[] = [];
    while (this.peek().type !== "EOF") {
      const kw = this.peek();
      if (kw.type !== "IDENT") {
        throw new ParseError(
          `Expected 'principle' or 'predicate' but got '${kw.value}'`,
          kw.line, kw.col,
        );
      }
      if (kw.value === "principle") {
        nodes.push(this.parsePrinciple());
      } else if (kw.value === "predicate") {
        nodes.push(this.parsePredicate());
      } else {
        throw new ParseError(
          `Unknown top-level keyword '${kw.value}'. Expected 'principle' or 'predicate'`,
          kw.line, kw.col,
        );
      }
    }
    return { nodes };
  }

  private parsePrinciple(): PrincipleNode {
    const loc = this.loc();
    this.expectIdent("principle");
    const nameTok = this.expectIdent();
    const name = nameTok.value;
    this.expect("LBRACE");

    // match clauses
    this.expectIdent("match");
    const matchClauses: MatchClause[] = [];
    matchClauses.push(this.parseMatchClause());
    // Additional match clauses: peek for $var: node
    while (this.peek().type === "VAR") {
      matchClauses.push(this.parseMatchClause());
    }

    // Optional require clause
    let requireClause = null;
    if (this.peek().type === "IDENT" && this.peek().value === "require") {
      requireClause = this.parseRequireClause();
    }

    // Report block
    const reportBlock = this.parseReportBlock();

    this.expect("RBRACE");
    return { kind: "principle", name, matchClauses, requireClause, reportBlock, loc };
  }

  private parsePredicate(): PredicateDef {
    const loc = this.loc();
    this.expectIdent("predicate");
    const nameTok = this.expectIdent();
    const name = nameTok.value;

    this.expect("LPAREN");
    const paramTok = this.expectVar();
    const paramVar = paramTok.value;
    this.expect("COLON");
    this.expectIdent("node");
    this.expect("RPAREN");

    this.expect("LBRACE");
    this.expectIdent("match");
    const matchClauses: MatchClause[] = [];
    matchClauses.push(this.parseMatchClause());
    while (this.peek().type === "VAR") {
      matchClauses.push(this.parseMatchClause());
    }
    this.expect("RBRACE");

    return { kind: "predicate", name, paramVar, matchClauses, loc };
  }

  private parseMatchClause(): MatchClause {
    const loc = this.loc();
    const varTok = this.expectVar();
    const varName = varTok.value;
    this.expect("COLON");
    this.expectIdent("node");
    this.expectIdent("where");
    const where = this.parseWherePred();
    return { varName, where, loc };
  }

  private parseWherePred(): WherePred {
    const loc = this.loc();
    const operands: AtomPred[] = [];
    operands.push(this.parseAtomPred());
    while (this.peek().type === "IDENT" && this.peek().value === "and") {
      this.consume(); // eat "and"
      operands.push(this.parseAtomPred());
    }
    return { kind: "andPred", operands, loc };
  }

  private parseAtomPred(): AtomPred {
    const loc = this.loc();
    const capColRef = this.parseCapCol();
    this.expect("EQEQ");
    const rhs = this.parseRHS();
    return { kind: "atomPred", lhs: capColRef, rhs, loc };
  }

  private parseCapCol(): CapColRef {
    const loc = this.loc();
    const capTok = this.expectIdent();
    this.expect("DOT");
    const colTok = this.expectIdent();
    return { capability: capTok.value, column: colTok.value, loc };
  }

  private parseRHS(): RHS {
    const t = this.peek();

    if (t.type === "STRING") {
      this.consume();
      return { kind: "string", value: t.value, loc: { line: t.line, col: t.col } } satisfies Literal;
    }

    if (t.type === "NUMBER") {
      this.consume();
      return { kind: "number", value: Number(t.value), loc: { line: t.line, col: t.col } } satisfies Literal;
    }

    if (t.type === "IDENT") {
      if (t.value === "true") {
        this.consume();
        return { kind: "bool", value: true, loc: { line: t.line, col: t.col } } satisfies Literal;
      }
      if (t.value === "false") {
        this.consume();
        return { kind: "bool", value: false, loc: { line: t.line, col: t.col } } satisfies Literal;
      }
      if (t.value === "null") {
        this.consume();
        return { kind: "null", loc: { line: t.line, col: t.col } } satisfies Literal;
      }
      throw new ParseError(`Unexpected identifier '${t.value}' on RHS`, t.line, t.col);
    }

    if (t.type === "VAR") {
      // Could be $var or $var.cap.col
      const varTok = this.consume();
      if (this.peek().type === "DOT") {
        this.consume(); // eat first dot
        const capTok = this.expectIdent();
        this.expect("DOT");
        const colTok = this.expectIdent();
        return {
          kind: "varDeref",
          varName: varTok.value,
          capability: capTok.value,
          column: colTok.value,
          loc: { line: varTok.line, col: varTok.col },
        } satisfies VarDeref;
      }
      return {
        kind: "varRef",
        name: varTok.value,
        loc: { line: varTok.line, col: varTok.col },
      } satisfies VarRef;
    }

    throw new ParseError(`Expected RHS value (string, number, bool, null, or $var) but got ${t.type} '${t.value}'`, t.line, t.col);
  }

  private parseRequireClause(): RequireClause {
    const loc = this.loc();
    this.expectIdent("require");
    const noTok = this.peek();
    if (noTok.type !== "IDENT" || noTok.value !== "no") {
      throw new ParseError(
        `Expected 'no' after 'require' but got '${noTok.value}'`,
        noTok.line, noTok.col,
      );
    }
    this.consume(); // eat "no"

    const guardVarTok = this.expectVar();
    const guardVar = guardVarTok.value;
    this.expect("COLON");

    // predName($var) or predName($var.cap.col)
    const predNameTok = this.expectIdent();
    const predName = predNameTok.value;
    this.expect("LPAREN");
    const predArgTok = this.expectVar();
    let predArgVarName: string | null = null;
    let predArgDeref: import("./ast.js").VarDeref | null = null;
    if (this.peek().type === "DOT") {
      this.consume(); // eat first dot
      const capTok = this.expectIdent();
      this.expect("DOT");
      const colTok = this.expectIdent();
      predArgDeref = {
        kind: "varDeref",
        varName: predArgTok.value,
        capability: capTok.value,
        column: colTok.value,
        loc: { line: predArgTok.line, col: predArgTok.col },
      };
    } else {
      predArgVarName = predArgTok.value;
    }
    this.expect("RPAREN");

    // Peek: if next token is IDENT "where", parse NEW explicit relation-call form.
    // Otherwise fall through to OLD RELATION $target form.
    if (this.peek().type === "IDENT" && this.peek().value === "where") {
      this.consume(); // eat "where"

      // Parse: IDENT "(" relationArg "," relationArg ")"
      const relNameTok = this.expectIdent();
      const relation: BuiltinRelation = relNameTok.value;
      this.expect("LPAREN");
      const arg0 = this.parseRelationArg();
      this.expect("COMMA");
      const arg1 = this.parseRelationArg();
      this.expect("RPAREN");

      return {
        guardVar,
        predName,
        predArgVarName,
        predArgDeref,
        relation,
        relationArgs: [arg0, arg1],
        targetVarName: null,
        targetVarDeref: null,
        loc,
      };
    }

    // OLD form: relation name — any IDENT; compiler validates against registry
    const relTok = this.peek();
    if (relTok.type !== "IDENT") {
      throw new ParseError(
        `Expected relation name (identifier) or 'where' but got '${relTok.value}'`,
        relTok.line, relTok.col,
      );
    }
    this.consume();
    const relation: BuiltinRelation = relTok.value;

    const targetVarTok = this.expectVar();
    let targetVarName: string | null = null;
    let targetVarDeref: import("./ast.js").VarDeref | null = null;
    if (this.peek().type === "DOT") {
      this.consume(); // eat first dot
      const capTok = this.expectIdent();
      this.expect("DOT");
      const colTok = this.expectIdent();
      targetVarDeref = {
        kind: "varDeref",
        varName: targetVarTok.value,
        capability: capTok.value,
        column: colTok.value,
        loc: { line: targetVarTok.line, col: targetVarTok.col },
      };
    } else {
      targetVarName = targetVarTok.value;
    }

    return { guardVar, predName, predArgVarName, predArgDeref, relation, relationArgs: null, targetVarName, targetVarDeref, loc };
  }

  private parseRelationArg(): RelationArg {
    const varTok = this.expectVar();
    if (this.peek().type === "DOT") {
      this.consume(); // eat first dot
      const capTok = this.expectIdent();
      this.expect("DOT");
      const colTok = this.expectIdent();
      const deref: VarDeref = {
        kind: "varDeref",
        varName: varTok.value,
        capability: capTok.value,
        column: colTok.value,
        loc: { line: varTok.line, col: varTok.col },
      };
      return { name: varTok.value, deref };
    }
    return { name: varTok.value, deref: null };
  }

  private parseReportBlock(): ReportBlock {
    const loc = this.loc();
    this.expectIdent("report");
    const sevTok = this.peek();
    if (sevTok.type !== "IDENT" || !["violation", "warning", "info"].includes(sevTok.value)) {
      throw new ParseError(
        `Expected severity ('violation', 'warning', or 'info') but got '${sevTok.value}'`,
        sevTok.line, sevTok.col,
      );
    }
    this.consume();
    const severity = sevTok.value as Severity;

    this.expect("LBRACE");

    // at $var
    this.expectIdent("at");
    const atVarTok = this.expectVar();
    const atVar = atVarTok.value;

    // captures { ... }
    this.expectIdent("captures");
    this.expect("LBRACE");
    const captures: CaptureEntry[] = [];
    while (this.peek().type !== "RBRACE") {
      const captureLoc = this.loc();
      const nameTok = this.expectIdent();
      this.expect("COLON");
      const varTok = this.expectVar();
      captures.push({ name: nameTok.value, varName: varTok.value, loc: captureLoc });
      if (this.peek().type === "COMMA") this.consume();
    }
    this.expect("RBRACE");

    // message "..."
    this.expectIdent("message");
    const msgTok = this.expect("STRING");
    const message = msgTok.value;

    this.expect("RBRACE");

    return { severity, atVar, captures, message, loc };
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function parseDSL(src: string): Program {
  const tokens = tokenize(src);
  const parser = new Parser(tokens);
  return parser.parseProgram();
}
