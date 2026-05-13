$LOAD_PATH.unshift File.expand_path("../lib", __dir__)

require "provekit/ruby_lifter"

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
end
