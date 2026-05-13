$LOAD_PATH.unshift File.expand_path("../lib", __dir__)

require "provekit/ruby_lifter"
require "provekit/ruby_realizer"

describe Provekit::RubyRealizer do
  it "realizes lifted ProofIR back to Ruby with cited sugars" do
    source = <<~RUBY
      sig { params(x: Integer).returns(Boolean) }
      # @pre x != nil
      def ok?(x)
        true
      end
    RUBY

    lifted = Provekit::RubyLifter.new.lift_source(source, "ok.rb")
    realized = Provekit::RubyRealizer.new.realize(lifted, "sugar_cids" => ["provekit:sugar:ruby:sorbet-sigs:v1"])

    expect(realized["emitted_source"]).to include("sig { params(x: Integer).returns(Boolean) }")
    expect(realized["emitted_source"]).to include("def ok?(x)")
    expect(realized["observed_loss_record"]).to be_empty
    expect(realized["used_sugars"]).to include("provekit:sugar:ruby:sorbet-sigs:v1")
  end
end
