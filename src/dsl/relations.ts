/**
 * A7c: Built-in relation registrations.
 *
 * Each relation self-registers on module import via registerRelation().
 * Importing this module (or calling registerBuiltinRelations()) is
 * sufficient to populate the registry.
 *
 * MVP implements: before, dominates.
 * Reserved (not implemented): post_dominates, data_source, data_flow_reaches,
 * encloses, always_exits, branch_reaches, mutates, literal_value, call_arity,
 * method_name, compound_assignment.
 */

import { registerRelation } from "./relationRegistry.js";
import type { BuiltinRelation } from "./ast.js";

export const BUILTIN_RELATIONS = new Set<BuiltinRelation>(["before", "dominates"]);

export function isBuiltinRelation(name: string): name is BuiltinRelation {
  return BUILTIN_RELATIONS.has(name as BuiltinRelation);
}

/**
 * Register all built-in relations. Called automatically on module import.
 * May also be called explicitly after _clearRelationRegistry() in tests.
 */
export function registerBuiltinRelations(): void {
  registerRelation({
    name: "before",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      const b = args[1]?.kind === "node" ? args[1].alias : null;
      if (!a || !b) throw new Error("before: both args must be node");
      return `(${a}.source_start < ${b}.source_start AND ${a}.file_id = ${b}.file_id)`;
    },
  });

  registerRelation({
    name: "dominates",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      const b = args[1]?.kind === "node" ? args[1].alias : null;
      if (!a || !b) throw new Error("dominates: both args must be node");
      return `EXISTS (SELECT 1 FROM dominance WHERE dominator = ${a}.id AND dominated = ${b}.id)`;
    },
  });
}

// Self-register on module import.
registerBuiltinRelations();
