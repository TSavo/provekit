// 2026-04-26: mined from BugsJS (eslint-228). The bug class:
//   if (param) { ... }   // param is a string parameter
// silently treats empty-string, 0, and false as "missing" when they may be
// valid inputs. The fix: replace with explicit `typeof param === "string"`
// or `param != null`.
//
// Same shape as falsy-default but on `truthy_test` coercion_kind. Both are
// asserting parameter-flow to filter out cases where the truthy test is on a
// locally-known-non-falsy value.

predicate any_param_decl($x: node) {
  match $b: node where binding.binding_kind == "param"
}

principle truthy-test-loses-falsy {
  match $node: node where truthiness.coercion_kind == "truthy_test"
  require $witness: any_param_decl($node)
    where flows_from_param($node.truthiness.operand_node)
  report violation {
    at $node
    captures { node: $node }
    message "truthy test on parameter-derived value silently treats empty string / 0 / false as missing"
  }
}
