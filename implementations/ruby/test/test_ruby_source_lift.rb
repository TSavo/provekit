# SPDX-License-Identifier: Apache-2.0

require "json"
require "minitest/autorun"

require_relative "../lib/provekit/lift/ruby_source"

class TestRubySourceLift < Minitest::Test
  RubySource = Provekit::Lift::RubySource

  def canon(value)
    Provekit::IR::Jcs.encode(value)
  end

  def contract(ir, fn_name)
    ir.find { |item| item["fnName"] == fn_name } || flunk("missing contract #{fn_name}: #{ir.inspect}")
  end

  def ctor_names(node)
    case node
    when Hash
      names = node["kind"] == "ctor" ? [node["name"]] : []
      Array(node["args"]).each { |child| names.concat(ctor_names(child)) }
      names
    when Array
      node.flat_map { |child| ctor_names(child) }
    else
      []
    end
  end

  def test_lift_method_defs_emits_source_unit_unique_names_and_ruby_ops
    source = <<~RUBY
      module M
        class C
          def add_one(x)
            y = x + K
            return y
          end

          def self.twice(y)
            y * 2
          end
        end
      end
    RUBY

    result = RubySource.lift_source(source, "lib/m/c.rb")

    assert_equal [], result.refusals
    assert_equal ["<source-unit:lib/m/c.rb>", "M::C#add_one", "M::C.twice"], result.ir.map { |item| item["fnName"] }

    source_unit = result.ir.first["post"]["args"][1]
    assert_equal "ruby:source-unit", source_unit["name"]
    assert_equal source, source_unit["args"][0]["value"]

    add_one = contract(result.ir, "M::C#add_one")
    assert_equal ["x"], add_one["formals"]
    assert_equal [{ "kind" => "reads", "target" => "K" }], add_one["effects"]
    assert_equal ["ruby:seq", "ruby:assign", "ruby:add", "ruby:const", "ruby:return"], ctor_names(add_one["post"]["args"][1])

    twice = contract(result.ir, "M::C.twice")
    assert_equal ["y"], twice["formals"]
    assert_includes ctor_names(twice["post"]["args"][1]), "ruby:mul"
    refute_includes canon(result.ir), "ruby:unknown"
    refute_includes canon(result.ir), "ruby:skip"
  end

  def test_refuses_blocks_without_unknown_or_skip_terms
    source = <<~RUBY
      def bad(xs)
        xs.each { |x| puts x }
      end
    RUBY

    result = RubySource.lift_source(source, "bad.rb")

    assert_equal ["<source-unit:bad.rb>"], result.ir.map { |item| item["fnName"] }
    assert_equal 1, result.refusals.length
    refusal = result.refusals.first
    assert_equal "unhandled-syntax", refusal["kind"]
    assert_equal "<top>#bad", refusal["function"]
    assert_equal 2, refusal["line"]
    assert_match(/block/i, refusal["reason"])
    refute_includes canon(result.ir), "ruby:unknown"
    refute_includes canon(result.ir), "ruby:skip"
  end

  def test_effects_are_canonical_wire_shapes_sorted_and_loop_cid_is_blake3_512
    source = <<~RUBY
      def effectful(xs)
        $g = @@c + K
        while $g < 10
          $g = $g + 1
        end
        puts($g)
        helper($g)
        raise "bad"
      end
    RUBY

    result = RubySource.lift_source(source, "effects.rb")

    fx = contract(result.ir, "<top>#effectful")["effects"]
    assert_equal ["reads", "reads", "reads", "writes", "io", "panics", "unresolved_call", "opaque_loop"], fx.map { |effect| effect["kind"] }
    assert_equal "$g", fx[0]["target"]
    assert_equal "@@c", fx[1]["target"]
    assert_equal "K", fx[2]["target"]
    assert_equal "$g", fx[3]["target"]
    assert_equal "helper", fx[6]["name"]
    assert_match(/\Ablake3-512:[0-9a-f]{128}\z/, fx[7]["loopCid"])
  end

  def test_compile_lift_roundtrip_body_term_is_byte_identical
    source = <<~RUBY
      def f(x)
        y = x + 1
        return y
      end
    RUBY
    lifted = RubySource.lift_source(source, "roundtrip.rb")
    body = contract(lifted.ir, "<top>#f")["post"]["args"][1]

    compiled = RubySource.compile_body_term(body, fn_name: "f", formals: ["x"])
    relifted = RubySource.lift_source(compiled, "roundtrip.rb")
    relifted_body = contract(relifted.ir, "<top>#f")["post"]["args"][1]

    assert_equal Provekit::IR::Jcs.encode(body), Provekit::IR::Jcs.encode(relifted_body)
  end

  def test_rpc_initialize_declares_ruby_source_draft
    result = RubySource.initialize_result

    assert_equal "0.1.0-draft", result["version"]
    assert_equal "provekit-lift/1", result["protocol_version"]
    assert_equal "ruby-source", result["dialect"]
    assert_equal ["ruby-source"], result["capabilities"]["authoring_surfaces"]
    assert_equal false, result["capabilities"]["emits_signed_mementos"]
  end
end
