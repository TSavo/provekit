/**
 * A7b: DSL compiler.
 *
 * Compiles a parsed DSL Program into a query function:
 *   compilePrinciple(principle, predicates) → (db) => MatchRow[]
 *
 * The query function runs a raw SQL SELECT (via better-sqlite3 .prepare())
 * that produces one row per matched site. Capability resolution goes through
 * getCapability() / getCapabilityColumn() — never hardcoded.
 *
 * Compile-time errors:
 *   - Unknown capability name → CompileError with did-you-mean
 *   - Unknown column name → CompileError with did-you-mean
 *   - Closed-enum violation → CompileError listing allowed values
 *   - Unknown built-in relation → CompileError
 *   - Unbound variable → CompileError
 *
 * Design: for each principle, we generate a SQL query of the form:
 *
 *   SELECT
 *     alias_div.node_id AS __at,
 *     alias_div.node_id AS __cap_division,
 *     alias_den.node_id AS __cap_denominator,
 *     ...
 *   FROM node_arithmetic AS alias_div
 *   JOIN node_narrows AS alias_den ON <join condition>
 *   WHERE alias_div.op = '/'
 *     AND NOT EXISTS (subquery for require no)
 *
 * Variables correspond to capability table aliases. A variable is "introduced"
 * by its first match clause; subsequent predicates on the same variable add
 * WHERE conditions on the already-joined table.
 *
 * Cross-clause varDeref ($other.cap.col) emits a JOIN against the capability
 * table for `cap` with node_id = the column value on `$other`'s row.
 */

import type { Db } from "../db/index.js";
import {
  getCapability,
  listCapabilities,
} from "../sast/capabilityRegistry.js";
import type {
  PrincipleNode,
  PredicateDef,
  MatchClause,
  AtomPred,
  RHS,
  VarDeref,
  RequireClause,
  ReportBlock,
} from "./ast.js";

// ---------------------------------------------------------------------------
// Compile-time error
// ---------------------------------------------------------------------------

export class CompileError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CompileError";
  }
}

// ---------------------------------------------------------------------------
// Levenshtein distance (for did-you-mean)
// ---------------------------------------------------------------------------

function levenshtein(a: string, b: string): number {
  const m = a.length, n = b.length;
  const dp: number[][] = Array.from({ length: m + 1 }, (_, i) =>
    Array.from({ length: n + 1 }, (__, j) => (i === 0 ? j : j === 0 ? i : 0)),
  );
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] = a[i - 1] === b[j - 1]
        ? dp[i - 1][j - 1]
        : 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
    }
  }
  return dp[m][n];
}

function didYouMean(input: string, candidates: string[]): string | undefined {
  let best: string | undefined;
  let bestDist = Infinity;
  for (const c of candidates) {
    const d = levenshtein(input, c);
    if (d < bestDist) { bestDist = d; best = c; }
  }
  return bestDist <= 3 ? best : undefined;
}

// ---------------------------------------------------------------------------
// Column-name → SQL column name mapping
// ---------------------------------------------------------------------------

/**
 * Given a drizzle column reference (duck-typed), extract the SQL column name.
 * drizzle-orm stores it as column.name (the sql name) on the column object.
 */
function sqlColName(drizzleCol: any): string {
  // drizzle-orm BetterSQLiteColumn has .name or .columnName
  return drizzleCol?.name ?? drizzleCol?.columnName ?? String(drizzleCol);
}

/**
 * Given a drizzle table reference, extract the SQL table name.
 */
function sqlTableName(table: any): string {
  // drizzle-orm SQLiteTable exposes [Symbol('drizzle:Name')] or ._.name
  if (table?._ && typeof table._.name === "string") return table._.name;
  // fallback: search for symbol key
  const sym = Object.getOwnPropertySymbols(table).find(
    (s) => String(s).includes("Name") || String(s).includes("name"),
  );
  if (sym) return (table as any)[sym];
  throw new CompileError(`Cannot determine SQL table name for table ${String(table)}`);
}

// ---------------------------------------------------------------------------
// Variable binding tracker
// ---------------------------------------------------------------------------

interface VarBinding {
  varName: string;
  /** SQL alias for the capability table join, e.g. "cap_div" */
  tableAlias: string;
  /** Capability DSL name, e.g. "arithmetic" */
  capabilityName: string;
  /** SQL table name, e.g. "node_arithmetic" */
  sqlTable: string;
}

// ---------------------------------------------------------------------------
// Compiled match row type
// ---------------------------------------------------------------------------

export interface MatchRow {
  /** Node id for the `at $var` in the report block */
  atNodeId: string;
  /** Map from capture name to node id */
  captures: Record<string, string>;
}

export type CompiledPrincipleQuery = (db: Db) => MatchRow[];

// ---------------------------------------------------------------------------
// Predicate inlining
// ---------------------------------------------------------------------------

/**
 * Inline a predicate definition into a set of match clauses.
 *
 * When the predicate argument is a simple varRef ($argVar), we substitute
 * the predicate's paramVar with argVar everywhere.
 *
 * When the predicate argument is a varDeref ($outer.cap.col), we substitute
 * the predicate's paramVar with a new internal binding that represents the
 * value of that column. We rewrite occurrences of paramVar as the deref.
 *
 * Returns the renamed match clauses.
 */
function inlinePredicate(
  pred: PredicateDef,
  req: import("./ast.js").RequireClause,
  guardVarName: string,
): MatchClause[] {
  const argVarName = req.predArgVarName;
  const argDeref = req.predArgDeref;

  return pred.matchClauses.map((clause) => {
    const newVarName = clause.varName === pred.paramVar
      ? (argVarName ?? `${guardVarName}_param`)
      : `${guardVarName}_${clause.varName}`;

    const newAtoms = clause.where.operands.map((atom) => {
      const rhs = atom.rhs;
      let newRhs: typeof rhs = rhs;

      if (argVarName) {
        // Simple substitution: paramVar → argVarName
        if (rhs.kind === "varRef" && rhs.name === pred.paramVar) {
          newRhs = { ...rhs, name: argVarName };
        } else if (rhs.kind === "varDeref" && rhs.varName === pred.paramVar) {
          newRhs = { ...rhs, varName: argVarName };
        }
      } else if (argDeref) {
        // varDeref substitution: paramVar occurrences are replaced with the deref value.
        // When paramVar appears as varRef on RHS (meaning "the paramVar's node_id"),
        // we replace with a varDeref pointing to the same column.
        if (rhs.kind === "varRef" && rhs.name === pred.paramVar) {
          // Replace with the deref
          newRhs = { ...argDeref };
        } else if (rhs.kind === "varDeref" && rhs.varName === pred.paramVar) {
          newRhs = { ...rhs, varName: argDeref.varName };
        }
      }
      return { ...atom, rhs: newRhs };
    });
    return {
      ...clause,
      varName: newVarName,
      where: { ...clause.where, operands: newAtoms },
    };
  });
}

// ---------------------------------------------------------------------------
// Core compiler
// ---------------------------------------------------------------------------

/**
 * Validate a capability column reference and return the SQL column name.
 */
function resolveCapCol(
  capName: string,
  colName: string,
  contextDesc: string,
): { capSqlTable: string; colSqlName: string } {
  const cap = getCapability(capName);
  if (!cap) {
    const allCaps = listCapabilities().map((c) => c.dslName);
    const dym = didYouMean(capName, allCaps);
    throw new CompileError(
      `Unknown capability '${capName}'${contextDesc}` +
      (dym ? `. Did you mean '${dym}'?` : ""),
    );
  }
  const col = cap.columns[colName];
  if (!col) {
    const allCols = Object.keys(cap.columns);
    const dym = didYouMean(colName, allCols);
    throw new CompileError(
      `Unknown column '${colName}' on capability '${capName}'${contextDesc}` +
      (dym ? `. Did you mean '${dym}'?` : ""),
    );
  }
  return {
    capSqlTable: sqlTableName(cap.table),
    colSqlName: sqlColName(col.drizzleColumn),
  };
}

/**
 * Validate that a literal value matches the column's closed enum (if any).
 */
function validateEnum(capName: string, colName: string, value: string): void {
  const cap = getCapability(capName)!;
  const col = cap.columns[colName];
  if (col?.kindEnum && !col.kindEnum.includes(value)) {
    throw new CompileError(
      `'${value}' is not in the closed enum for '${capName}.${colName}'. ` +
      `Allowed: ${col.kindEnum.join(", ")}`,
    );
  }
}

/**
 * Compile a principle node into a query function.
 *
 * @param principle  The parsed principle to compile.
 * @param predicates Map of predicate name → PredicateDef (for inlining).
 */
export function compilePrinciple(
  principle: PrincipleNode,
  predicates: Map<string, PredicateDef>,
): CompiledPrincipleQuery {
  // Validate and build SQL for the principle's own match clauses.
  // We need to:
  //   1. For each match clause, assign a table alias.
  //   2. Collect JOIN / FROM clauses.
  //   3. Collect WHERE conditions.
  //   4. If there's a requireClause, build a NOT EXISTS subquery.
  //   5. SELECT node_id columns for each capture.

  // -------------------------------------------------------------------------
  // Determine primary capability for each match clause variable.
  //
  // Rule: the capability used in the WHERE clause's first atom determines
  // which table the variable is joined against. All atoms in the clause must
  // use the same capability (or reference other bound vars via varDeref).
  // -------------------------------------------------------------------------

  const varBindings = new Map<string, VarBinding>();
  const joinClauses: string[] = [];
  const whereConditions: string[] = [];
  let aliasCounter = 0;

  function nextAlias(prefix: string): string {
    return `${prefix}_${aliasCounter++}`;
  }

  function getOrBindVar(varName: string, capName: string): VarBinding {
    if (varBindings.has(varName)) {
      const existing = varBindings.get(varName)!;
      if (existing.capabilityName !== capName) {
        throw new CompileError(
          `Variable '$${varName}' is already bound to capability '${existing.capabilityName}' ` +
          `but clause also references capability '${capName}'`,
        );
      }
      return existing;
    }
    const cap = getCapability(capName);
    if (!cap) {
      const allCaps = listCapabilities().map((c) => c.dslName);
      const dym = didYouMean(capName, allCaps);
      throw new CompileError(
        `Unknown capability '${capName}'` + (dym ? `. Did you mean '${dym}'?` : ""),
      );
    }
    const alias = nextAlias(`cap_${capName}`);
    const sqlTable = sqlTableName(cap.table);
    const binding: VarBinding = { varName, tableAlias: alias, capabilityName: capName, sqlTable };
    varBindings.set(varName, binding);
    return binding;
  }

  /**
   * Process a list of match clauses: bind variables, emit JOINs and WHERE conditions.
   * Returns the list of aliases for the "node_id" column of each new variable.
   */
  function processMatchClauses(
    clauses: MatchClause[],
    isFirst: boolean,
    extraJoins: string[],
    extraWheres: string[],
  ): void {
    for (let ci = 0; ci < clauses.length; ci++) {
      const clause = clauses[ci];
      const varName = clause.varName;
      const atoms = clause.where.operands;

      // Determine the primary capability from the first atom's LHS.
      const firstAtom = atoms[0];
      if (!firstAtom) {
        throw new CompileError(`Match clause for '$${varName}' has no predicates`);
      }

      // Resolve capability for this variable.
      const capName = firstAtom.lhs.capability;
      const cap = getCapability(capName);
      if (!cap) {
        const allCaps = listCapabilities().map((c) => c.dslName);
        const dym = didYouMean(capName, allCaps);
        throw new CompileError(
          `Unknown capability '${capName}'` + (dym ? `. Did you mean '${dym}'?` : ""),
        );
      }

      const isAlreadyBound = varBindings.has(varName);
      const binding = getOrBindVar(varName, capName);

      if (!isAlreadyBound) {
        // Emit a JOIN (or FROM for the first table).
        if (isFirst && ci === 0) {
          extraJoins.push(`FROM ${binding.sqlTable} AS ${binding.tableAlias}`);
        } else {
          extraJoins.push(`JOIN ${binding.sqlTable} AS ${binding.tableAlias} ON 1=1`);
        }
      }

      // Process each atom in the WHERE clause.
      for (const atom of atoms) {
        const { capSqlTable: _capSqlTable, colSqlName } = resolveCapCol(
          atom.lhs.capability,
          atom.lhs.column,
          ` referenced by '$${varName}'`,
        );
        // Ensure all atoms on this variable use the same capability.
        if (atom.lhs.capability !== capName) {
          // The variable is bound to capName; this atom uses a different capability.
          // This is not allowed in the simple model — all atoms in one clause must
          // use the same capability table.
          throw new CompileError(
            `Match clause for '$${varName}' mixes capabilities '${capName}' and ` +
            `'${atom.lhs.capability}'. Each variable may only reference one capability.`,
          );
        }

        const lhsExpr = `${binding.tableAlias}.${colSqlName}`;

        const rhs = atom.rhs;
        if (rhs.kind === "string") {
          validateEnum(capName, atom.lhs.column, rhs.value);
          extraWheres.push(`${lhsExpr} = '${rhs.value.replace(/'/g, "''")}'`);
        } else if (rhs.kind === "number") {
          extraWheres.push(`${lhsExpr} = ${rhs.value}`);
        } else if (rhs.kind === "bool") {
          extraWheres.push(`${lhsExpr} = ${rhs.value ? 1 : 0}`);
        } else if (rhs.kind === "null") {
          extraWheres.push(`${lhsExpr} IS NULL`);
        } else if (rhs.kind === "varRef") {
          // $other — must be bound
          const otherBinding = varBindings.get(rhs.name);
          if (!otherBinding) {
            throw new CompileError(
              `Unbound variable '$${rhs.name}' referenced in predicate for '$${varName}'`,
            );
          }
          // Compare the node_id columns
          const otherNodeIdCol = resolveCapCol(otherBinding.capabilityName, "node_id", "").colSqlName;
          extraWheres.push(`${lhsExpr} = ${otherBinding.tableAlias}.${otherNodeIdCol}`);
        } else if (rhs.kind === "varDeref") {
          // $other.cap.col
          const deref = rhs as VarDeref;
          const otherBinding = varBindings.get(deref.varName);
          if (!otherBinding) {
            throw new CompileError(
              `Unbound variable '$${deref.varName}' in cross-clause reference '$${deref.varName}.${deref.capability}.${deref.column}'`,
            );
          }
          const { colSqlName: otherColSql } = resolveCapCol(
            deref.capability,
            deref.column,
            ` in cross-clause reference from '$${varName}'`,
          );
          if (otherBinding.capabilityName !== deref.capability) {
            throw new CompileError(
              `Cross-clause reference '$${deref.varName}.${deref.capability}.${deref.column}': ` +
              `variable '$${deref.varName}' is bound to capability '${otherBinding.capabilityName}', ` +
              `not '${deref.capability}'`,
            );
          }
          extraWheres.push(`${lhsExpr} = ${otherBinding.tableAlias}.${otherColSql}`);
        }
      }
    }
  }

  // -------------------------------------------------------------------------
  // Build the main query.
  // -------------------------------------------------------------------------

  const mainJoins: string[] = [];
  const mainWheres: string[] = [];

  processMatchClauses(principle.matchClauses, true, mainJoins, mainWheres);

  // Add a JOIN to the nodes table for the `at $var` to get file_id and source_start.
  // We need nodes info for the `before` relation in require clauses.
  // We'll join nodes for ALL bound variables (needed for position checks).
  const nodeTableAliases = new Map<string, string>(); // varName → node alias
  for (const [varName, binding] of varBindings) {
    const nodeAlias = `node_${varName}`;
    nodeTableAliases.set(varName, nodeAlias);
    const nodeIdCol = resolveCapCol(binding.capabilityName, "node_id", "").colSqlName;
    mainJoins.push(
      `JOIN nodes AS ${nodeAlias} ON ${nodeAlias}.id = ${binding.tableAlias}.${nodeIdCol}`,
    );
  }

  // -------------------------------------------------------------------------
  // Build NOT EXISTS subquery for requireClause.
  // -------------------------------------------------------------------------

  let notExistsSql = "";
  if (principle.requireClause) {
    const req = principle.requireClause;

    // Validate predicate exists.
    const pred = predicates.get(req.predName);
    if (!pred) {
      throw new CompileError(
        `Unknown predicate '${req.predName}' referenced in require clause`,
      );
    }

    // Validate predArg is bound in the main query.
    if (req.predArgVarName) {
      const predArgBinding = varBindings.get(req.predArgVarName);
      if (!predArgBinding) {
        throw new CompileError(
          `Unbound variable '$${req.predArgVarName}' used as predicate argument in require clause`,
        );
      }
    } else if (req.predArgDeref) {
      const deref = req.predArgDeref;
      const predArgBinding = varBindings.get(deref.varName);
      if (!predArgBinding) {
        throw new CompileError(
          `Unbound variable '$${deref.varName}' in predicate argument deref in require clause`,
        );
      }
    }

    // Validate targetVar is bound in the main query.
    const targetBinding = varBindings.get(req.targetVar);
    if (!targetBinding) {
      throw new CompileError(
        `Unbound variable '$${req.targetVar}' in require clause relation`,
      );
    }
    void targetBinding;

    // Inline the predicate.
    const inlinedClauses = inlinePredicate(pred, req, req.guardVar);

    // Build subquery variable scope: copy main varBindings, then add inlined clauses.
    // We need a separate alias counter scope to avoid collisions.
    const subAliasCounter = { n: 100 }; // start high to avoid main query alias collisions
    const subVarBindings = new Map<string, VarBinding>();

    // Import main bindings as-is (they're outer refs in the subquery).
    for (const [vn, vb] of varBindings) {
      subVarBindings.set(vn, vb);
    }

    const subJoins: string[] = [];
    const subWheres: string[] = [];
    let firstSubVarNodeAlias: string | null = null; // node alias for the first guard variable

    // Process inlined predicate clauses.
    for (let ci = 0; ci < inlinedClauses.length; ci++) {
      const clause = inlinedClauses[ci];
      const varName = clause.varName;
      const atoms = clause.where.operands;

      if (atoms.length === 0) {
        throw new CompileError(`Inlined predicate clause for '$${varName}' has no predicates`);
      }

      const capName = atoms[0].lhs.capability;
      const cap = getCapability(capName);
      if (!cap) {
        const allCaps = listCapabilities().map((c) => c.dslName);
        const dym = didYouMean(capName, allCaps);
        throw new CompileError(
          `Unknown capability '${capName}' in predicate '${req.predName}'` + (dym ? `. Did you mean '${dym}'?` : ""),
        );
      }

      if (!subVarBindings.has(varName)) {
        const alias = `sub_${capName}_${subAliasCounter.n++}`;
        const sqlTable = sqlTableName(cap.table);
        const subBinding: VarBinding = { varName, tableAlias: alias, capabilityName: capName, sqlTable };
        subVarBindings.set(varName, subBinding);
        if (ci === 0) {
          subJoins.push(`FROM ${sqlTable} AS ${alias}`);
        } else {
          subJoins.push(`JOIN ${sqlTable} AS ${alias} ON 1=1`);
        }
      }

      const subBinding = subVarBindings.get(varName)!;

      for (const atom of atoms) {
        if (atom.lhs.capability !== capName) {
          throw new CompileError(
            `Predicate clause for '$${varName}' mixes capabilities`,
          );
        }
        const { colSqlName } = resolveCapCol(atom.lhs.capability, atom.lhs.column, "");
        const lhsExpr = `${subBinding.tableAlias}.${colSqlName}`;

        const rhs = atom.rhs;
        if (rhs.kind === "string") {
          validateEnum(capName, atom.lhs.column, rhs.value);
          subWheres.push(`${lhsExpr} = '${rhs.value.replace(/'/g, "''")}'`);
        } else if (rhs.kind === "number") {
          subWheres.push(`${lhsExpr} = ${rhs.value}`);
        } else if (rhs.kind === "bool") {
          subWheres.push(`${lhsExpr} = ${rhs.value ? 1 : 0}`);
        } else if (rhs.kind === "null") {
          subWheres.push(`${lhsExpr} IS NULL`);
        } else if (rhs.kind === "varRef") {
          const otherBinding = subVarBindings.get(rhs.name);
          if (!otherBinding) {
            throw new CompileError(`Unbound variable '$${rhs.name}' in predicate '${req.predName}'`);
          }
          const otherNodeIdCol = resolveCapCol(otherBinding.capabilityName, "node_id", "").colSqlName;
          subWheres.push(`${lhsExpr} = ${otherBinding.tableAlias}.${otherNodeIdCol}`);
        } else if (rhs.kind === "varDeref") {
          const deref = rhs as VarDeref;
          const otherBinding = subVarBindings.get(deref.varName);
          if (!otherBinding) {
            throw new CompileError(`Unbound variable '$${deref.varName}' in predicate '${req.predName}'`);
          }
          if (otherBinding.capabilityName !== deref.capability) {
            throw new CompileError(
              `Cross-clause reference '$${deref.varName}.${deref.capability}.${deref.column}': ` +
              `variable '$${deref.varName}' is bound to '${otherBinding.capabilityName}'`,
            );
          }
          const { colSqlName: otherColSql } = resolveCapCol(deref.capability, deref.column, "");
          subWheres.push(`${lhsExpr} = ${otherBinding.tableAlias}.${otherColSql}`);
        }
      }

      // Add nodes join for the guard variable (for position check in built-in relation).
      if (!subVarBindings.has(`__node_${varName}`)) {
        const nodeAlias = `sub_node_${varName}`;
        const nodeIdCol = resolveCapCol(capName, "node_id", "").colSqlName;
        subJoins.push(`JOIN nodes AS ${nodeAlias} ON ${nodeAlias}.id = ${subBinding.tableAlias}.${nodeIdCol}`);
        subVarBindings.set(`__node_${varName}`, {
          varName: `__node_${varName}`,
          tableAlias: nodeAlias,
          capabilityName: "nodes",
          sqlTable: "nodes",
        });
        // Track the node alias for the first guard variable.
        if (firstSubVarNodeAlias === null) {
          firstSubVarNodeAlias = nodeAlias;
        }
      }
    }

    // Apply the built-in relation between guardVar and targetVar.
    // The guard node alias is the node alias for the first inlined variable
    // (which represents the guard in the source — the narrows/other matching row).
    const guardNodeAlias = firstSubVarNodeAlias ?? `sub_node_${req.guardVar}`;

    const targetNodeAlias = nodeTableAliases.get(req.targetVar);
    if (!targetNodeAlias) {
      throw new CompileError(`No nodes alias for '$${req.targetVar}' in require clause`);
    }

    if (req.relation === "before") {
      // guard.source_start < target.source_start AND same file_id
      subWheres.push(
        `${guardNodeAlias}.source_start < ${targetNodeAlias}.source_start`,
      );
      subWheres.push(
        `${guardNodeAlias}.file_id = ${targetNodeAlias}.file_id`,
      );
    } else if (req.relation === "dominates") {
      // EXISTS (SELECT 1 FROM dominance WHERE dominator = guard.id AND dominated = target.id)
      subWheres.push(
        `EXISTS (SELECT 1 FROM dominance WHERE dominator = ${guardNodeAlias}.id AND dominated = ${targetNodeAlias}.id)`,
      );
    }

    void 0; // nothing to do here

    const subFromClause = subJoins.join("\n    ");
    const subWhereStr = subWheres.length > 0 ? `WHERE ${subWheres.join("\n      AND ")}` : "";
    notExistsSql = `NOT EXISTS (\n  SELECT 1\n  ${subFromClause}\n  ${subWhereStr}\n)`;
  }

  // -------------------------------------------------------------------------
  // SELECT columns: `at` and each capture.
  // -------------------------------------------------------------------------

  const report: ReportBlock = principle.reportBlock;

  // Validate `at $var` is bound.
  const atBinding = varBindings.get(report.atVar);
  if (!atBinding) {
    throw new CompileError(`Unbound variable '$${report.atVar}' in report block 'at' clause`);
  }
  const atNodeIdCol = resolveCapCol(atBinding.capabilityName, "node_id", "").colSqlName;
  const atNodeAlias = nodeTableAliases.get(report.atVar)!;

  const selectCols: string[] = [
    `${atBinding.tableAlias}.${atNodeIdCol} AS __at`,
    `${atNodeAlias}.file_id AS __file_id`,
  ];

  for (const cap of report.captures) {
    const capBinding = varBindings.get(cap.varName);
    if (!capBinding) {
      throw new CompileError(`Unbound variable '$${cap.varName}' in captures block`);
    }
    const capNodeIdCol = resolveCapCol(capBinding.capabilityName, "node_id", "").colSqlName;
    selectCols.push(`${capBinding.tableAlias}.${capNodeIdCol} AS __cap_${cap.name}`);
  }

  // -------------------------------------------------------------------------
  // Assemble final SQL.
  // -------------------------------------------------------------------------

  const fromAndJoins = mainJoins.join("\n");
  const allWheres = [...mainWheres];
  if (notExistsSql) allWheres.push(notExistsSql);
  const whereStr = allWheres.length > 0 ? `WHERE ${allWheres.join("\n  AND ")}` : "";

  const sql = [
    `SELECT`,
    `  ${selectCols.join(",\n  ")}`,
    fromAndJoins,
    whereStr,
  ].filter(Boolean).join("\n");

  // -------------------------------------------------------------------------
  // Return query function.
  // -------------------------------------------------------------------------

  const captureNames = report.captures.map((c) => c.name);
  const message = report.message;

  return function runQuery(db: Db): MatchRow[] {
    const rawDb = db.$client as import("better-sqlite3").Database;
    const stmt = rawDb.prepare(sql);
    const rows = stmt.all() as Record<string, string>[];
    return rows.map((row) => {
      const captures: Record<string, string> = {};
      for (const name of captureNames) {
        captures[name] = row[`__cap_${name}`] ?? "";
      }
      return {
        atNodeId: row["__at"] ?? "",
        captures,
        message,
      };
    });
  };
}

/**
 * Compile a full DSL Program and return the query functions for all principles.
 */
export function compileProgram(
  nodes: import("./ast.js").TopLevelNode[],
): Map<string, CompiledPrincipleQuery> {
  const predicates = new Map<string, PredicateDef>();
  const principles: PrincipleNode[] = [];

  for (const node of nodes) {
    if (node.kind === "predicate") {
      predicates.set(node.name, node);
    } else {
      principles.push(node);
    }
  }

  const result = new Map<string, CompiledPrincipleQuery>();
  for (const principle of principles) {
    result.set(principle.name, compilePrinciple(principle, predicates));
  }
  return result;
}
