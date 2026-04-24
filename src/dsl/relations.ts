/**
 * A7b: Built-in relations registry.
 *
 * MVP implements: before, dominates.
 * Reserved (not implemented): post_dominates, data_source, data_flow_reaches,
 * encloses, always_exits, branch_reaches, mutates, literal_value, call_arity,
 * method_name, compound_assignment.
 *
 * Built-in relations are part of the language spec; they are NOT extensible via
 * the capability registry (that's for data tables, not structural relations).
 */

import type { BuiltinRelation } from "./ast.js";

export const BUILTIN_RELATIONS = new Set<BuiltinRelation>(["before", "dominates"]);

export function isBuiltinRelation(name: string): name is BuiltinRelation {
  return BUILTIN_RELATIONS.has(name as BuiltinRelation);
}
