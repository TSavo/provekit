// A8: DSL translation of unguarded-await.json
// Match: await expression (yield_kind == "await") NOT inside a try/catch.
// yields.yield_kind == "await" + is_inside_handler equivalent.
//
// NOTE: The throws capability tracks is_inside_handler for throw nodes.
// The yields capability does NOT have an is_inside_handler column — the extractor
// records yield_kind and source_call_node only. The TemplateEngine's isInsideTryCatch
// check for await_expression has no direct DSL equivalent without an
// is_inside_handler column on yields.
//
// This DSL file matches ALL await expressions (over-matches guarded ones).
//
// FIXME(capability-gap): yields table lacks is_inside_handler column.
// Workaround: a `try_enclosure` relation or adding the column to yields (minor substrate
// extension). See capability-gaps.md.

principle unguarded-await {
  match $await: node where yields.yield_kind == "await"
  report violation {
    at $await
    captures { await: $await }
    message "await expression outside try/catch; rejection propagates uncaught"
  }
}
