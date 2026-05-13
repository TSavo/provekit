module Provekit
  module Sugars
    module ContractsRuby
      CID = "provekit:sugar:ruby:contracts-ruby:v1"

      def self.patterns
        [
          {
            proofir: "requires(Numeric, Numeric) and returns(Numeric)",
            ruby: lambda { "Contract Numeric, Numeric => Numeric" },
            inverse: lambda { |node| node.to_s.include?("Contract") ? "requires(Numeric, Numeric) and returns(Numeric)" : nil },
            role: "require",
            cid: CID
          },
          {
            proofir: "assert(condition)",
            ruby: lambda { |condition| "raise ArgumentError if !(#{condition})" },
            inverse: lambda { |node| node.to_s.include?("raise ArgumentError") ? "assert(condition)" : nil },
            role: "witness",
            cid: CID
          }
        ]
      end
    end
  end
end
