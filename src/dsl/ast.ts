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
 *   requireClause = "require" "no" "$" IDENT ":" predCall builtinRel "$" IDENT
 *   predCall      = IDENT "(" varRef ")"
 *   varRef        = "$" IDENT
 *   builtinRel    = "before" | "dominates"
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

export type BuiltinRelation = "before" | "dominates";
// Reserved for future: "post_dominates" | "data_source" | "data_flow_reaches"
// | "encloses" | "always_exits" | "branch_reaches" | "mutates"
// | "literal_value" | "call_arity" | "method_name" | "compound_assignment"

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
  /** Built-in relation applied between guard var and target var */
  relation: BuiltinRelation;
  /** The target variable (RHS of the relation) */
  targetVar: string;
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
