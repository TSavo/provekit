# SPDX-License-Identifier: Apache-2.0

require "json"
require "minitest/autorun"

ROOT = File.expand_path("../../..", __dir__)
SPECS = File.join(ROOT, "menagerie/ruby-language-signature/specs")

class TestRubyLanguageSignatureCatalog < Minitest::Test
  def load_spec(name)
    JSON.parse(File.read(File.join(SPECS, name)))
  end

  def op(name)
    load_spec("op_#{name}.spec.json")
  end

  def test_ruby_signature_catalog_has_draft_version_and_expected_ops
    signature = load_spec("language_signature_ruby.spec.json")

    assert_equal "language_signature", signature["kind"]
    assert_equal "ruby", signature["fn_name"]
    assert_equal "0.1.0-draft", signature["version"]
    assert_includes signature["operations"], "op_source-unit.spec.json"
    assert_includes signature["operations"], "op_send.spec.json"
    assert_includes signature["operations"], "op_ternary.spec.json"
    refute_includes signature["operations"], "op_unknown.spec.json"
    refute_includes signature["operations"], "op_binop.spec.json"
    refute_includes signature["operations"], "op_skip.spec.json"
  end

  def test_required_arity_shapes_are_explicit
    assert_equal(
      { "kind" => "named", "slots" => [{ "name" => "target" }, { "name" => "value" }] },
      op("assign")["post"]["arity_shape"],
    )
    assert_equal(
      { "kind" => "positional", "arity" => 2 },
      op("seq")["post"]["arity_shape"],
    )
    assert_equal(
      { "kind" => "named", "slots" => [{ "name" => "lhs" }, { "name" => "rhs" }] },
      op("add")["post"]["arity_shape"],
    )
    assert_equal(
      {
        "kind" => "named",
        "slots" => [
          { "name" => "lhs" },
          { "name" => "rhs", "evaluation" => "unevaluated" },
        ],
      },
      op("and")["post"]["arity_shape"],
    )
    assert_equal(
      {
        "kind" => "named",
        "slots" => [
          { "name" => "cond" },
          { "name" => "then_expr", "evaluation" => "unevaluated" },
          { "name" => "else_expr", "evaluation" => "unevaluated" },
        ],
      },
      op("ternary")["post"]["arity_shape"],
    )
    assert_equal(
      {
        "kind" => "named",
        "slots" => [
          { "name" => "bytes", "evaluation" => "unevaluated", "slot_sort" => "literal" },
          { "name" => "operational_term" },
        ],
      },
      op("source-unit")["post"]["arity_shape"],
    )
  end
end
