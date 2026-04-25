/**
 * A7b: DSL AST types.
 *
 * Grammar choice for cross-clause joins:
 *   A `wherePred` atom may reference a column from another bound variable:
 *       capability.col == $other.cap2.col2
 *   where `$other` must be a previously-bound variable and `cap2.col2` refers to
 *   a column on the capability that was used to introduce `$other`. This is
 *   syntactic sugar for a JOIN: the compiler adds a JOIN on the referenced
 *   capability row and emits a WHERE equality between the two drizzle columns.
 *
 *   Full grammar (supports division-by-zero principle):
 *
 *   program       = (principle | predicate)*
 *
 *   principle     = "principle" IDENT "{" "match" matchClause+
 *                   requireClause? reportBlock "}"
 *
 *   predicate     = "predicate" IDENT "(" param ")" "{" "match" matchClause+ "}"
 *   param         = "$" IDENT ":" "node"
 *
 *   matchClause   = "$" IDENT ":" "node" "where" wherePred
 *
 *   wherePred     = atomPred ("and" atomPred)*
 *   atomPred      = capCol "==" rhs
 *   capCol        = IDENT "." IDENT
 *   rhs           = varDeref | literal
 *   varDeref      = "$" IDENT "." IDENT "." IDENT    -- cross-clause ref: $var.cap.col
 *                 | "$" IDENT                         -- direct var ref (node id comparison)
 *   literal       = STRING | NUMBER | "true" | "false" | "null"
 *
 *   requireClause = "require" "no" "$" IDENT ":" predCall "where" relationCall     (NEW form)
 *                 | "require" "no" "$" IDENT ":" predCall relationName targetRef   (OLD compat form)
 *   relationCall  = IDENT "(" relationArg "," relationArg ")"
 *   relationArg   = "$" IDENT ("." IDENT "." IDENT)?
 *   predCall      = IDENT "(" predArg ")"
 *   predArg       = varDeref | varRef
 *   targetRef     = varDeref | varRef
 *   relationName  = IDENT   -- any registered relation name; validated at compile time
 *
 *   reportBlock   = "report" severity "{" "at" varRef capturesBlock messageLine "}"
 *   severity      = "violation" | "warning" | "info"
 *   capturesBlock = "captures" "{" capture ("," capture)* "}"
 *   capture       = IDENT ":" varRef
 *   messageLine   = "message" STRING
 *
 *   IDENT         = [a-zA-Z_][a-zA-Z0-9_-]*
 *   STRING        = '"' [^"]* '"'
 *   NUMBER        = [0-9]+ ("." [0-9]+)?
 */

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

export type Severity = "violation" | "warning" | "info";

/**
 * Relation name in a require-clause. Accepts any identifier; the compiler
 * validates against the relation registry (getRelation) at compile time and
 * throws CompileError with the list of registered names on a miss.
 *
 * Previously "before" | "dominates" — opened to string so the registry-based
 * architecture is symmetric with how capability column references work.
 */
export type BuiltinRelation = string;

export interface SourceLoc {
  line: number;
  col: number;
}

// ---------------------------------------------------------------------------
// Literals and RHS values
// ---------------------------------------------------------------------------

export interface StringLiteral {
  kind: "string";
  value: string;
  loc: SourceLoc;
}

export interface NumberLiteral {
  kind: "number";
  value: number;
  loc: SourceLoc;
}

export interface BoolLiteral {
  kind: "bool";
  value: boolean;
  loc: SourceLoc;
}

export interface NullLiteral {
  kind: "null";
  loc: SourceLoc;
}

export type Literal = StringLiteral | NumberLiteral | BoolLiteral | NullLiteral;

/** Direct variable reference: `$varName` (the node id of that bound variable). */
export interface VarRef {
  kind: "varRef";
  name: string; // without leading $
  loc: SourceLoc;
}

/**
 * Cross-clause dereference: `$varName.capabilityName.columnName`
 * Resolves to the value of capabilityName.columnName for the row
 * where capabilityName.node_id = $varName.
 */
export interface VarDeref {
  kind: "varDeref";
  varName: string;    // without leading $
  capability: string; // e.g. "arithmetic"
  column: string;     // e.g. "rhs_node"
  loc: SourceLoc;
}

export type RHS = Literal | VarRef | VarDeref;

// ---------------------------------------------------------------------------
// Capability column reference on LHS
// ---------------------------------------------------------------------------

export interface CapColRef {
  capability: string; // e.g. "arithmetic"
  column: string;     // e.g. "op"
  loc: SourceLoc;
}

// ---------------------------------------------------------------------------
// Where predicates
// ---------------------------------------------------------------------------

export interface AtomPred {
  kind: "atomPred";
  lhs: CapColRef;
  rhs: RHS;
  loc: SourceLoc;
}

export interface AndPred {
  kind: "andPred";
  operands: AtomPred[];
  loc: SourceLoc;
}

export type WherePred = AndPred;

// ---------------------------------------------------------------------------
// Match clause
// ---------------------------------------------------------------------------

export interface MatchClause {
  varName: string; // without leading $
  where: WherePred;
  loc: SourceLoc;
}

// ---------------------------------------------------------------------------
// Require clause
// ---------------------------------------------------------------------------

/** A single argument in an explicit relation call: `$var` or `$var.cap.col`. */
export interface RelationArg {
  /** Variable name without leading `$`. */
  name: string;
  /** When present, this is a `$var.cap.col` deref; when null, it's a bare `$var`. */
  deref: VarDeref | null;
}

export interface RequireClause {
  /** The guard variable introduced by `require no $guard: ...` */
  guardVar: string;
  /** The predicate call: predName($varRef) or predName($var.cap.col) */
  predName: string;
  /**
   * Predicate argument: either a simple variable ref or a varDeref.
   * When it's a varRef, predArgVarName is set and predArgDeref is null.
   * When it's a varDeref, predArgDeref is set and predArgVarName is null.
   */
  predArgVarName: string | null;
  predArgDeref: VarDeref | null;
  /** Built-in relation applied between guard var and target */
  relation: BuiltinRelation;
  /**
   * NEW: explicit relation call args from `where RELATION(LHS, RHS)` syntax.
   * When present (non-null), the compiler resolves both args as relation sides.
   * When null, the compiler falls back to the OLD form using firstSubVarNodeAlias
   * as LHS and (targetVarName / targetVarDeref) as RHS.
   */
  relationArgs: [RelationArg, RelationArg] | null;
  /**
   * OLD form: relation target — either a bare var ref or a varDeref.
   * When it's a varRef, targetVarName is set and targetVarDeref is null.
   * When it's a varDeref, targetVarDeref is set and targetVarName is null.
   * Ignored when `relationArgs` is non-null.
   */
  targetVarName: string | null;
  targetVarDeref: VarDeref | null;
  loc: SourceLoc;
}

// ---------------------------------------------------------------------------
// Report block
// ---------------------------------------------------------------------------

export interface CaptureEntry {
  name: string;   // capture label (no $)
  varName: string; // bound variable (no $)
  loc: SourceLoc;
}

export interface ReportBlock {
  severity: Severity;
  atVar: string; // variable referenced by `at $var`
  captures: CaptureEntry[];
  message: string;
  loc: SourceLoc;
}

// ---------------------------------------------------------------------------
// Top-level nodes
// ---------------------------------------------------------------------------

export interface PrincipleNode {
  kind: "principle";
  name: string;
  matchClauses: MatchClause[];
  requireClause: RequireClause | null;
  reportBlock: ReportBlock;
  loc: SourceLoc;
}

export interface PredicateDef {
  kind: "predicate";
  name: string;
  paramVar: string; // without leading $
  matchClauses: MatchClause[];
  loc: SourceLoc;
}

export type TopLevelNode = PrincipleNode | PredicateDef;

export interface Program {
  nodes: TopLevelNode[];
}
