/**
 * A7b: DSL evaluator.
 *
 * Parses a DSL source string, compiles all principles, executes them against
 * the given DB, writes results to principle_matches + principle_match_captures,
 * and returns PrincipleMatch objects.
 */

import { parseDSL } from "./parser.js";
import { compileProgram } from "./compiler.js";
import type { Db } from "../db/index.js";
import { principleMatches, principleMatchCaptures } from "../db/schema/principleMatches.js";
import type { Severity } from "./ast.js";

export interface PrincipleMatch {
  matchId: number;       // primary key from principle_matches table
  principleName: string;
  rootNodeId: string;    // node referenced by `at $var`
  severity: Severity;
  message: string;
  captures: Record<string, string>;  // capture name → node id
}

/**
 * Compile a DSL source string and run it against the given DB.
 * Returns the matches, also writes them to principle_matches +
 * principle_match_captures tables for inspection.
 *
 * @param db         Open provekit database (with migrations applied).
 * @param dslSource  DSL source text (may contain multiple principles/predicates).
 */
export function evaluatePrinciple(
  db: Db,
  dslSource: string,
): PrincipleMatch[] {
  const program = parseDSL(dslSource);
  const queries = compileProgram(program.nodes);

  // Determine severity map from parsed principles.
  const severityMap = new Map<string, Severity>();
  const messageMap = new Map<string, string>();
  for (const node of program.nodes) {
    if (node.kind === "principle") {
      severityMap.set(node.name, node.reportBlock.severity);
      messageMap.set(node.name, node.reportBlock.message);
    }
  }

  const results: PrincipleMatch[] = [];

  for (const [principleName, queryFn] of queries) {
    const rows = queryFn(db);
    const severity = severityMap.get(principleName) ?? "violation";
    const message = messageMap.get(principleName) ?? "";

    for (const row of rows) {
      // Insert into principle_matches.
      const inserted = db.insert(principleMatches).values({
        principleName,
        fileId: row.fileId,
        rootMatchNodeId: row.atNodeId,
        severity,
        message,
      }).returning({ id: principleMatches.id }).get();

      const matchId = inserted?.id ?? 0;

      // Insert captures.
      for (const [captureName, capturedNodeId] of Object.entries(row.captures)) {
        if (!capturedNodeId) continue;
        db.insert(principleMatchCaptures).values({
          matchId,
          captureName,
          capturedNodeId,
        }).run();
      }

      results.push({
        matchId,
        principleName,
        rootNodeId: row.atNodeId,
        severity,
        message,
        captures: row.captures,
      });
    }
  }

  return results;
}
