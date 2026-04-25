/**
 * A7c: Built-in relation registrations.
 *
 * Each relation self-registers on module import via registerRelation().
 * Importing this module (or calling registerBuiltinRelations()) is
 * sufficient to populate the registry.
 *
 * Current relations: before, dominates, same_value, data_flow_reaches.
 * Reserved (not implemented): post_dominates, data_source,
 * encloses, always_exits, branch_reaches, mutates, literal_value, call_arity,
 * method_name, compound_assignment.
 *
 * The DSL parser now accepts any IDENT as a relation name in the requireClause
 * position ("require no $g: pred($arg) RELATION_NAME $var"). Validation is
 * deferred to compile time via getRelation(). Principle migrations that need
 * same_value are unblocked at the parser layer.
 *
 * Remaining grammar limitation: relations can only appear in the requireClause
 * position — both arguments must be whole-node variables. Relations on column
 * dereferences (e.g. same_value(narrows.target_node, $den)) or inside predicate
 * where-clause atoms are not yet supported. That is a grammar extension for a
 * future iteration.
 */

import { registerRelation } from "./relationRegistry.js";

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

  // A8b: same_value — holds iff two nodes reference the same declared variable.
  // Two use-site nodes share a semantic variable when they share a from_node in
  // the data_flow table (the declaration node is the common ancestor).
  // Self-identity (a === b) is covered: a node shares its own from_node with itself.
  registerRelation({
    name: "same_value",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      const b = args[1]?.kind === "node" ? args[1].alias : null;
      if (!a || !b) throw new Error("same_value: both args must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM data_flow df1 ` +
        `JOIN data_flow df2 ON df1.from_node = df2.from_node ` +
        `WHERE df1.to_node = ${a}.id AND df2.to_node = ${b}.id` +
        `)`
      );
    },
  });

  // Leak 2 substrate prerequisite: data_flow_reaches(source, sink) — true iff
  // the value of `source` can flow (transitively) into `sink` via 0+ hops in
  // the data_flow graph. Backed by data_flow_transitive, which now contains
  // real chains thanks to the chain-formation init edges in
  // src/sast/dataFlow.ts (the bipartite-graph limitation is resolved).
  //
  // Direction: data_flow rows are (to_node, from_node) where from_node's value
  // flows TO to_node. So data_flow_reaches(source, sink) ⇔
  // data_flow_transitive row with from_node = source.id AND to_node = sink.id.
  //
  // Note: data_flow_transitive contains all direct edges PLUS multi-hop
  // ancestors, so 1-hop reachability is included. Self-reach (source === sink)
  // is NOT included here since data_flow_transitive does not include zero-hop
  // identity rows. Callers needing reflexive closure can OR with `same_node`.
  registerRelation({
    name: "data_flow_reaches",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      const b = args[1]?.kind === "node" ? args[1].alias : null;
      if (!a || !b) throw new Error("data_flow_reaches: both args must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM data_flow_transitive ` +
        `WHERE from_node = ${a}.id AND to_node = ${b}.id` +
        `)`
      );
    },
  });
}

// Self-register on module import.
registerBuiltinRelations();
