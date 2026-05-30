$LOAD_PATH.unshift File.expand_path("../lib", __dir__)

require "provekit/ruby_lifter"
require "provekit/sugar_dict"

describe Provekit::RubyLifter do
  it "lifts methods, Sorbet sigs, comments, bindings, panics, and IO effects" do
    source = <<~RUBY
      sig { params(x: Integer).returns(Boolean) }
      # @pre x != nil
      # @post out == true
      def ok?(x)
        y = x
        File.read("input.txt")
        raise ArgumentError if y.nil?
        true
      end
    RUBY

    doc = Provekit::RubyLifter.new.lift_source(source, "ok.rb")
    decl = doc["declarations"].first

    expect(doc["kind"]).to eq("ir-document")
    expect(decl["name"]).to eq("ok?")
    expect(decl.dig("evidence", "sorbet_sig", "source")).to eq("sig { params(x: Integer).returns(Boolean) }")
    expect(decl.dig("evidence", "comments", "pre")).to eq(["x != nil"])
    expect(decl.dig("evidence", "values").first["name"]).to eq("y")
    expect(decl.dig("evidence", "effects").map { |effect| effect["kind"] }).to include("Panics")
    expect(decl.dig("evidence", "effects").map { |effect| effect["kind"] }).to include("Io")
  end

  it "emits materializable method body source and AST template" do
    source = <<~RUBY
      def fetch_url(url, headers)
        client.execute(url, headers)
      end
    RUBY

    doc = Provekit::RubyLifter.new.lift_source(source, "shim.rb")
    body = doc["declarations"].first["body_source"]

    expect(body["file"]).to eq("shim.rb")
    expect(body["body_text"]).to eq("client.execute(url, headers)")
    expect(body["param_names"]).to eq(["url", "headers"])
    expect(body["ast_template"]["kind"]).to eq("ruby:block")
    expect(body["ast_template"].to_s).to include("param_ref")
    expect(body["template_cid"].start_with?("blake3-512:")).to eq(true)
  end

  it "keeps method AST template CIDs stable under parameter renaming" do
    source_a = <<~RUBY
      def fetch_url(url, headers)
        client.execute(url, headers)
      end
    RUBY
    source_b = <<~RUBY
      def fetch_url(uri, h)
        client.execute(uri, h)
      end
    RUBY

    body_a = Provekit::RubyLifter.new.lift_source(source_a, "a.rb")["declarations"].first["body_source"]
    body_b = Provekit::RubyLifter.new.lift_source(source_b, "b.rb")["declarations"].first["body_source"]

    expect(body_a["ast_template"]).to eq(body_b["ast_template"])
    expect(body_a["template_cid"]).to eq(body_b["template_cid"])
  end

  it "attaches body_source templates to materializable sugar dictionary patterns" do
    Provekit::SugarDict.patterns.each do |pattern|
      next unless pattern[:ruby]

      body = pattern[:body_source]
      expect(body["body_text"].empty?).to eq(false)
      expect(body["ast_template"]["kind"]).to eq("ruby:block")
      expect(body["template_cid"].start_with?("blake3-512:")).to eq(true)
    end
  end
end
