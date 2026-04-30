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
  // 2026-04-27: stale_assignment($if, $assn) — closes hard-bug 3
  // (variable-staleness on fall-through). True iff:
  //   1. $assn is structurally inside $if's `decides.consequent_node` (the
  //      then-branch), and
  //   2. $assn's value has data flow (transitive) to at least one node that
  //      is NOT enclosed by $if's source range — i.e., the assignment
  //      reaches a use on the fall-through path.
  //
  // The bug class: `let x = 0; if (cond) { x = 1; } use(x);` — when cond is
  // false, use(x) sees the unmodified default. The relation packages the
  // structural + data-flow constraints into a single SQL expression because
  // the DSL's require clause supports only one relation per principle.
  registerRelation({
    name: "stale_assignment",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null; // $if
      const b = args[1]?.kind === "node" ? args[1].alias : null; // $assn
      if (!a || !b) throw new Error("stale_assignment: both args must be node");
      return (
        `(EXISTS (` +
        // (1) $if has a decides row whose consequent_node structurally
        //     encloses $assn (source-range nesting on the consequent).
        `SELECT 1 FROM node_decides d, nodes c ` +
        `WHERE d.node_id = ${a}.id AND c.id = d.consequent_node ` +
        `AND c.file_id = ${b}.file_id ` +
        `AND c.source_start <= ${b}.source_start ` +
        `AND c.source_end >= ${b}.source_end` +
        `) AND EXISTS (` +
        // (2) The variable that $assn writes to has at least one OTHER
        //     use-site whose source range is NOT enclosed by $if. We pivot
        //     through node_assigns.target_node (the LHS use of the
        //     variable) and use the data_flow declaration-share pattern
        //     (same df.from_node = same declared variable) the same way
        //     same_value works.
        //
        //     2026-04-27: an earlier draft used `dft.from_node = $assn.id`
        //     which doesn't pair (data_flow tracks decl-to-use, not
        //     assignment-result-to-use). Pivoting through target_node is
        //     the correct same-variable semantic.
        `SELECT 1 FROM node_assigns assn_row, ` +
        `data_flow df1, data_flow df2, ` +
        `nodes use_n ` +
        `WHERE assn_row.node_id = ${b}.id ` +
        `AND df1.to_node = assn_row.target_node ` +
        `AND df2.from_node = df1.from_node ` +
        `AND df2.to_node = use_n.id ` +
        `AND use_n.id <> assn_row.target_node ` +
        `AND NOT (` +
        `use_n.file_id = ${a}.file_id ` +
        `AND use_n.source_start >= ${a}.source_start ` +
        `AND use_n.source_end <= ${a}.source_end` +
        `)` +
        `))`
      );
    },
  });

  // 2026-04-26: encloses($outer, $inner) — true iff $outer is an AST ancestor
  // of $inner (i.e. $outer's source range strictly contains $inner's). Used
  // by loop-accumulator-overflow ("augmented assignment inside a loop body")
  // and other principles needing parent-child structural containment.
  //
  // Source ranges in ts-morph are properly nested per AST contract, so source
  // span comparison is sufficient — no recursive closure needed. Compares are
  // cheap (single index seek on file_id).
  registerRelation({
    name: "encloses",
    paramCount: 2,
    paramTypes: ["node", "node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      const b = args[1]?.kind === "node" ? args[1].alias : null;
      if (!a || !b) throw new Error("encloses: both args must be node");
      return (
        `(${a}.file_id = ${b}.file_id AND ` +
        `${a}.source_start <= ${b}.source_start AND ` +
        `${a}.source_end >= ${b}.source_end AND ` +
        `${a}.id <> ${b}.id)`
      );
    },
  });

  // 2026-04-26: flows_from_param($n) — true iff $n receives data flow
  // (transitively) from a parameter binding declaration. Encodes the
  // "user-derived value" check the original tightening spec required:
  // a falsy-default `||` is only meaningful when the LHS could legitimately
  // be 0 / "" / false at runtime, i.e. when it traces back to external
  // input via a function parameter.
  //
  // SQL: there exists a transitive data_flow row from some param-bound
  // node into $n. The param-bound node is identified via node_binding rows
  // where binding_kind = 'param'.
  registerRelation({
    name: "flows_from_param",
    paramCount: 1,
    paramTypes: ["node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      if (!a) throw new Error("flows_from_param: arg must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM data_flow_transitive dft ` +
        `JOIN node_binding nb ON nb.node_id = dft.from_node ` +
        `WHERE dft.to_node = ${a}.id AND nb.binding_kind = 'param'` +
        `)`
      );
    },
  });

  // 2026-04-27: was_replaced_by_addition($preNode) — closes hard-bug 1
  // (diff-aware principle mining). True iff:
  //   1. $preNode pairs as `unchanged` to a post-side node in the active
  //      diff context (its fingerprint survived into the post tree), AND
  //   2. There exists an `added` post node whose source range strictly
  //      encloses that paired post node — i.e., $preNode's subtree was
  //      preserved but rewrapped inside new code.
  //
  // The bug class: `return x === "a" || x === "b"` (pre) is enclosed by
  // `return x === "a" || x === "b" || x === "c"` (post). The OR-chain
  // extension principle binds $preNode to the inner BinaryExpression and
  // detects the new-clause-extension by structural enclosure, not LLM
  // recognition. Generalizes to "any subtree extended by wrapping".
  //
  // Active context: requires diff_context_active to have a row. Without
  // it, the relation returns false (no diff in scope = no signal).
  // src/fix/harvest/diff.ts setActiveDiffContext() is the canonical setter.
  // 2026-04-27 round-2 tightening: require the enclosing added node to
  // itself be a BinaryExpression. The original relation accepted ANY
  // added enclosing node, which over-fired on wrappers — Hexo/12 added
  // `.toString()` around an unchanged OR (CallExpression encloses the
  // BinaryExpression but is not an OR-chain extension); eslint/184 had
  // unrelated added nodes enclosing the matched falsy_default.
  // The bug-class shape is "OR-chain wrapped by a wider OR-chain"; the
  // encloser must be a BinaryExpression to capture that shape. Other
  // shapes (e.g., addition wrapped by an addition) get a separate relation
  // when needed.
  registerRelation({
    name: "was_replaced_by_addition",
    paramCount: 1,
    paramTypes: ["node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      if (!a) throw new Error("was_replaced_by_addition: arg must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM pre_post_diff ppd_unc ` +
        `JOIN files f ON f.path = ppd_unc.file_path ` +
        `JOIN diff_context_active adc ON adc.context = ppd_unc.context ` +
        `WHERE f.id = ${a}.file_id ` +
        `AND ppd_unc.pre_start = ${a}.source_start ` +
        `AND ppd_unc.pre_kind = ${a}.kind ` +
        `AND ppd_unc.change_kind = 'unchanged' ` +
        `AND EXISTS (` +
        `SELECT 1 FROM pre_post_diff ppd_add ` +
        `WHERE ppd_add.context = ppd_unc.context ` +
        `AND ppd_add.file_path = ppd_unc.file_path ` +
        `AND ppd_add.change_kind = 'added' ` +
        `AND ppd_add.post_kind = 'BinaryExpression' ` +
        `AND ppd_add.post_start <= ppd_unc.post_start ` +
        `AND ppd_add.post_end >= ppd_unc.post_end ` +
        `AND NOT (ppd_add.post_start = ppd_unc.post_start ` +
        `AND ppd_add.post_end = ppd_unc.post_end)` +
        `)` +
        `)`
      );
    },
  });

  // 2026-04-27: is_in_dirty_set($node) — true iff $node corresponds to
  // a pre_post_diff row with change_kind != 'unchanged'. The diff-aware
  // counterpart of "this node was actually touched by the fix."
  //
  // Motivation: #115 step 2 manual-30 gate exposed a systemic over-
  // matching: arithmetic principles fired on stable code at the bug
  // locus (e.g., addition-overflow on `parentElements[0].loc.start.line`
  // when the actual fix was adding a null-guard). Without a way to
  // discriminate "node was modified" from "node happens to live near
  // the diff," static principles silently latch onto unchanged
  // ancestors of the actual fix and report violations.
  //
  // Mining context: $node is in the buggy SAST. Pre coordinates match
  // a row in pre_post_diff via (file_path, pre_start, pre_kind). The
  // change_kind says whether the fix touched it.
  //
  // Lint context: TBD — the lint-side variant would key on post coords
  // ("did the working-tree SAST node change vs HEAD?"). Add when lint's
  // diff-aware path lands.
  //
  // Without active diff context: returns false. Static-only runs see
  // no dirty-set effect — principles using this relation become dormant.
  registerRelation({
    name: "is_in_dirty_set",
    paramCount: 1,
    paramTypes: ["node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      if (!a) throw new Error("is_in_dirty_set: arg must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM pre_post_diff ppd ` +
        `JOIN files f ON f.path = ppd.file_path ` +
        `JOIN diff_context_active adc ON adc.context = ppd.context ` +
        `WHERE f.id = ${a}.file_id ` +
        `AND ppd.pre_start = ${a}.source_start ` +
        `AND ppd.pre_kind = ${a}.kind ` +
        `AND ppd.change_kind <> 'unchanged'` +
        `)`
      );
    },
  });

  // 2026-04-27: is_post_added($node) — lint-context counterpart. Fires
  // when $node was added in the post side relative to pre. Matched by
  // post coordinates (lint binds principles to the working-tree SAST).
  // Use this for "this code is new" queries; for "the BUGGY code that
  // got wrapped" queries, use `was_replaced_by_addition`.
  registerRelation({
    name: "is_post_added",
    paramCount: 1,
    paramTypes: ["node"],
    compile: ({ args }) => {
      const a = args[0]?.kind === "node" ? args[0].alias : null;
      if (!a) throw new Error("is_post_added: arg must be node");
      return (
        `EXISTS (` +
        `SELECT 1 FROM pre_post_diff ppd ` +
        `JOIN files f ON f.path = ppd.file_path ` +
        `JOIN diff_context_active adc ON adc.context = ppd.context ` +
        `WHERE f.id = ${a}.file_id ` +
        `AND ppd.post_start = ${a}.source_start ` +
        `AND ppd.post_kind = ${a}.kind ` +
        `AND ppd.change_kind = 'added'` +
        `)`
      );
    },
  });

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
