$LOAD_PATH.unshift File.expand_path("../lib", __dir__)

require "provekit/ruby_lifter"
require "provekit/ruby_realizer"

describe "Ruby kit round trip" do
  it "preserves a Sorbet contract CID through ProofIR and realization" do
    source = <<~RUBY
      sig { params(x: Integer).returns(Boolean) }
      # @pre x != nil
      def ok?(x)
        true
      end
    RUBY

    lifter = Provekit::RubyLifter.new
    first = lifter.lift_source(source, "ok.rb")
    realized = Provekit::RubyRealizer.new.realize(
      first,
      "sugar_cids" => ["provekit:sugar:ruby:sorbet-sigs:v1", "provekit:sugar:ruby:comment-roles:v1"]
    )
    second = lifter.lift_source(realized["emitted_source"], "ok.realized.rb")

    first_sig = first["declarations"].first.dig("evidence", "sorbet_sig", "source")
    second_sig = second["declarations"].first.dig("evidence", "sorbet_sig", "source")

    expect(second_sig).to eq(first_sig)
    expect(second["declarations"].first["contract_cid"]).to eq(first["declarations"].first["contract_cid"])
    expect(realized["observed_loss_record"]).to be_empty
  end
end
